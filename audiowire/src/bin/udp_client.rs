use std::{
    net::{SocketAddr, ToSocketAddrs},
    ops::{Deref, DerefMut},
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use anyhow::{Ok as _Ok, Result};
use audiowire::{
    HEARTBEAT_GRACE_PERIOD, HEARTBEAT_INTERVAL,
    backend::{config::Config, util::audio_check},
    command::{DeviceConfig, add_device_args},
    logging,
    packet::{
        command::{
            client::{ClientClose, ClientCommand, ClientHeartbeat},
            server::ServerCommand,
        },
        data::{IncomingServerData, OutgoingClientData},
        handshake::{Handshake, HandshakeAck, HandshakeInit, HandshakeReply},
        message::{IncomingMessage, OutgoingMessage},
        socket::{UdpWrapper, wrap_udp, wrap_udp_owned},
        stream::{StreamFlags, StreamId},
        time::{NetworkTime, get_current_timestamp},
    },
    stream::{Peer, handle_playback, handle_record},
};
use audiowire_serde::{Deserialize, Serialize};
use bytes::{Buf, Bytes};
use clap::{Arg, Command};
use slog::{Logger, debug, error, info, o};
use tokio::{
    net::UdpSocket,
    sync::{Notify, RwLock},
    time::sleep,
};

static RUNNING: AtomicBool = AtomicBool::new(true);
static NOTIFY: Notify = Notify::const_new();

struct Client {
    inner: Peer,
    log: Logger,
    last_heartbeat: Arc<RwLock<Instant>>,
}

impl Client {
    async fn handle_packet(&mut self, mut buf: Bytes) -> Result<()> {
        match IncomingMessage::deserialize(&mut buf)? {
            IncomingMessage::Data(mut buf) => {
                let data = IncomingServerData::deserialize(&mut buf)?;
                self.handle_data(data);
            }
            IncomingMessage::ServerCommand(cmd) => match cmd {
                ServerCommand::Heartbeat(_) => {
                    info!(self.log, "Received server heartbeat");
                    let mut value = self.last_heartbeat.write().await;
                    *value = Instant::now();
                }
                ServerCommand::Close(_) => {
                    info!(self.log, "Server closed");
                    stop();
                }
                _ => (),
            },
            _ => (),
        }
        Ok(())
    }

    fn handle_data(&mut self, data: IncomingServerData<Bytes>) {
        self.write(data.0)
            .map_err(|e| error!(self.log, "Failed to decode Opus packet"; "error" => e.to_string()))
            .ok();
    }
}

impl Deref for Client {
    type Target = Peer;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Client {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

struct HeartbeatWorker {
    log: Logger,
    stream_id: StreamId,
    sock: Arc<UdpSocket>,
    addr: SocketAddr,
    last_heartbeat: Arc<RwLock<Instant>>,
}

impl HeartbeatWorker {
    async fn run(self) {
        let mut sock = wrap_udp(&self.sock);
        while RUNNING.load(Ordering::Relaxed) {
            if !self.check().await {
                break;
            }
            self.pulse(sock.enter()).await;
        }
    }

    async fn check(&self) -> bool {
        tokio::select! {
            _ = sleep(HEARTBEAT_INTERVAL) => (),
            _ = NOTIFY.notified() => return false
        }
        let elapsed = self.last_heartbeat.read().await.elapsed();
        if elapsed > HEARTBEAT_GRACE_PERIOD {
            info!(self.log, "Closing due to server inactivity");
            stop();
            false
        } else {
            true
        }
    }

    async fn pulse<U, S>(&self, mut sock: S)
    where
        U: UdpWrapper,
        S: AsMut<U>,
    {
        let cmd: ClientCommand = ClientHeartbeat(self.stream_id).into();
        sock.as_mut()
            .send_message_to(cmd, self.addr)
            .await
            .map_err(|e| error!(self.log, "Failed to send heartbeat"; "error" => e))
            .ok();
    }
}

fn cmd() -> Command {
    let cmd = Command::new("audiowire-udp-client")
        .about("AudioWire server using UDP packets")
        .arg_required_else_help(true)
        .arg(
            Arg::new("addr")
                .help("Address of the AudioWire server (eg. localhost:8760)")
                .required(true),
        );
    add_device_args(cmd)
}

fn main() -> Result<ExitCode> {
    let matches = cmd().get_matches();
    let addr = matches.get_one("addr").map(String::to_owned).unwrap();
    let saddr = addr
        .to_socket_addrs()?
        .next()
        .expect("Unable to resolve server address");

    let device = DeviceConfig::from(&matches);
    let config = Config::default();
    let log = logging::initialize();

    audiowire::initialize()?;
    info!(log, "Starting audio check");
    audio_check(&log, &config, &device)?;
    info!(log, "Audio check finished");

    let result = tokio::runtime::Runtime::new()?
        .block_on(async move { run(log, config, device, addr, saddr).await });

    audiowire::terminate()?;

    result
}

async fn run(
    log: Logger,
    config: Config,
    device: DeviceConfig,
    name: String,
    saddr: SocketAddr,
) -> Result<ExitCode> {
    let DeviceConfig {
        source_name,
        sink_name,
        source_enabled,
        sink_enabled,
        opus_enabled,
    } = device;
    let mut sock = wrap_udp_owned(UdpSocket::bind(":::0").await?);

    info!(log, "Initiating handshake with server"; "addr" => saddr);
    let org_timestamp = get_current_timestamp();
    let init: Handshake = HandshakeInit {
        flags: StreamFlags {
            source_enabled,
            sink_enabled,
            opus_enabled,
        },
    }
    .into();
    sock.send_message_to(init, &saddr).await?;

    let (message, addr) = sock.recv_message_from().await?;
    let rec_timestamp = get_current_timestamp();

    let HandshakeReply {
        stream_id,
        flags,
        time,
    } = if let IncomingMessage::Handshake(Handshake::Reply(reply)) = message {
        reply
    } else {
        error!(log, "We should get handshake reply here");
        return _Ok(ExitCode::FAILURE);
    };
    let opus_enabled = device.opus_enabled && flags.opus_enabled;

    info!(
        log,
        "Got handshake reply";
        "stream_id" => stream_id,
        "stream_flags" => flags,
        "rec_timestamp" => time.rec_timestamp,
        "xmt_timestamp" => time.xmt_timestamp
    );

    let ack: Handshake = HandshakeAck {
        stream_id,
        time: NetworkTime {
            rec_timestamp,
            xmt_timestamp: get_current_timestamp(),
        },
    }
    .into();
    sock.send_message_to(ack, &saddr).await?;

    let sock = Arc::new(sock.into_inner());
    let record = if device.source_enabled && flags.sink_enabled {
        let log = log.new(o!("stream" => "record"));
        let stream = handle_record(
            &log,
            &config,
            name.as_str(),
            source_name,
            Arc::clone(&sock),
            addr,
            move |src, dst| {
                OutgoingMessage::from(OutgoingClientData(stream_id, src)).serialize(dst)
            },
            opus_enabled,
        )?;
        Some(stream)
    } else {
        None
    };

    let playback = if device.sink_enabled && flags.source_enabled {
        let log = log.new(o!("stream" => "playback"));
        let stream = handle_playback(&log, &config, name.as_str(), sink_name, opus_enabled)?;
        Some(stream)
    } else {
        None
    };

    let last_heartbeat = Arc::new(RwLock::new(Instant::now()));
    let mut client = Client {
        inner: Peer::new(record, playback, &time, org_timestamp, rec_timestamp),
        log: log.clone(),
        last_heartbeat: Arc::clone(&last_heartbeat),
    };
    let mut handles = Vec::new();

    // Cancel handler
    handles.push(tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        stop();
    }));

    // Heartbeat monitor
    handles.push({
        let worker = HeartbeatWorker {
            log: log.new(o!("worker" => "heartbeat")),
            stream_id,
            sock: Arc::clone(&sock),
            addr,
            last_heartbeat,
        };
        tokio::spawn(worker.run())
    });

    let mut sock = wrap_udp(sock);
    while RUNNING.load(Ordering::Relaxed) {
        let mut sock = sock.enter();
        let (buf, addr) = tokio::select! {
            result = sock.raw_recv_from() => result?,
            _ = NOTIFY.notified() => break
        };
        debug!(log, "Received data {} bytes", buf.remaining(); "addr" => addr);
        if let Err(e) = client.handle_packet(buf).await {
            error!(log, "Failed to handle packet"; "addr" => addr, "error" => e);
        }
    }
    let cmd: ClientCommand = ClientClose(stream_id).into();
    sock.send_message_to(cmd, &addr).await?;

    for handle in handles {
        handle.abort();
        handle.await.ok();
    }

    Ok(ExitCode::SUCCESS)
}

fn stop() {
    RUNNING.store(false, Ordering::Relaxed);
    NOTIFY.notify_waiters();
}
