use std::{
    io::{ErrorKind, Read},
    net::{SocketAddr, ToSocketAddrs, UdpSocket},
    ops::{Deref, DerefMut},
    process::ExitCode,
    sync::{
        Arc, Condvar, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Instant, SystemTime},
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
        data::{IncomingAudioData, IncomingServerData, OutgoingAudioData, OutgoingClientData},
        handshake::{Handshake, HandshakeAck, HandshakeInit, HandshakeReply},
        message::{IncomingMessage, OutgoingMessage},
        socket::{SharedUdpWrapper, UdpWrapper, wrap_udp, wrap_udp_owned},
        stream::{StreamFlags, StreamId},
        time::NetworkTime,
    },
    stream::{Peer, handle_playback, handle_record},
};
use audiowire_serde::Serialize;
use clap::{Arg, Command};
use slog::{Logger, debug, error, info, o};

static NOTIFY: (Mutex<bool>, Condvar) = (Mutex::new(true), Condvar::new());
static RUNNING: AtomicBool = AtomicBool::new(true);

struct Client {
    inner: Peer,
    log: Logger,
    last_heartbeat: Arc<RwLock<Instant>>,
}

impl Client {
    fn handle_packet(&mut self, mut buf: &[u8]) -> Result<()> {
        match IncomingMessage::deserialize(&mut buf)? {
            IncomingMessage::Data(buf) => {
                let data = IncomingAudioData::deserialize(IncomingServerData::deserialize(buf)?)?;
                self.handle_data(data);
            }
            IncomingMessage::ServerCommand(cmd) => match cmd {
                ServerCommand::Heartbeat(_) => {
                    debug!(self.log, "Received server heartbeat");
                    let mut value = self.last_heartbeat.write().unwrap();
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

    fn handle_data<R: Read>(&mut self, data: IncomingAudioData<R>) {
        self.write(data)
            .map_err(|e| error!(self.log, "Failed to write incoming audio data"; "error" => e.to_string()))
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
    sock: SharedUdpWrapper<Arc<UdpSocket>>,
    addr: SocketAddr,
    last_heartbeat: Arc<RwLock<Instant>>,
}

impl HeartbeatWorker {
    fn run(mut self) {
        let (lock, cvar) = &NOTIFY;
        let mut running = lock.lock().unwrap();
        while *running {
            let (update, _) = cvar
                .wait_timeout_while(running, HEARTBEAT_INTERVAL, |val| *val && is_running())
                .unwrap();
            running = update;
            if !self.check() {
                break;
            }
            self.pulse();
        }
    }

    fn check(&mut self) -> bool {
        let elapsed = self.last_heartbeat.read().unwrap().elapsed();
        if elapsed > HEARTBEAT_GRACE_PERIOD {
            info!(self.log, "Closing due to server inactivity");
            background_stop();
            false
        } else {
            true
        }
    }

    fn pulse(&mut self) {
        let cmd: ClientCommand = ClientHeartbeat(self.stream_id).into();
        self.sock
            .send_message_to(cmd, self.addr)
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

    let result = run(log, config, device, addr, saddr);
    audiowire::terminate()?;

    result
}

fn run(
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
    let mut sock = wrap_udp_owned(UdpSocket::bind(":::0")?);

    info!(log, "Initiating handshake with server"; "addr" => saddr);
    let org_timestamp = SystemTime::now();
    let init: Handshake = HandshakeInit {
        flags: StreamFlags {
            source_enabled,
            sink_enabled,
            opus_enabled,
        },
    }
    .into();
    sock.send_message_to(init, &saddr)?;

    let (message, addr) = sock.recv_message_from()?;
    let rec_timestamp = SystemTime::now();

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
        "rec_timestamp" => logging::Timestamp(time.rec_timestamp),
        "xmt_timestamp" => logging::Timestamp(time.xmt_timestamp)
    );

    let ack: Handshake = HandshakeAck {
        stream_id,
        time: NetworkTime {
            rec_timestamp,
            xmt_timestamp: SystemTime::now(),
        },
    }
    .into();
    sock.send_message_to(ack, &saddr)?;

    let sock = Arc::new(sock.into_inner());
    sock.set_nonblocking(true)?;

    let mut sequence = 0;
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
                sequence += 1;
                OutgoingMessage::from(OutgoingClientData(
                    stream_id,
                    OutgoingAudioData {
                        sequence,
                        timestamp: SystemTime::now(),
                        data: src,
                    },
                ))
                .serialize(dst)
            },
            opus_enabled,
        )?;
        Some(stream)
    } else {
        None
    };

    let playback = if device.sink_enabled && flags.source_enabled {
        let log = log.new(o!("stream" => "playback"));
        let stream = handle_playback(
            &log,
            &config,
            name.as_str(),
            sink_name,
            &time,
            org_timestamp,
            rec_timestamp,
            opus_enabled,
        )?;
        Some(stream)
    } else {
        None
    };

    let last_heartbeat = Arc::new(RwLock::new(Instant::now()));
    let mut client = Client {
        inner: Peer { record, playback },
        log: log.clone(),
        last_heartbeat: Arc::clone(&last_heartbeat),
    };
    let mut handles = Vec::new();

    // Cancel handler
    ctrlc::set_handler(|| stop()).unwrap();

    // Heartbeat monitor
    handles.push({
        let worker = HeartbeatWorker {
            log: log.new(o!("worker" => "heartbeat")),
            stream_id,
            sock: wrap_udp(Arc::clone(&sock)),
            addr,
            last_heartbeat,
        };
        thread::spawn(|| worker.run())
    });

    let interval = config.buffer_duration() / 4;
    let mut exit_code = ExitCode::SUCCESS;
    let mut sock = wrap_udp(sock);
    while is_running() {
        let (buf, addr) = match sock.raw_recv_from() {
            Ok(value) => value,
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                thread::sleep(interval);
                continue;
            }
            Err(e) => {
                error!(log, "Socket recv_from returns error"; "error" => e);
                exit_code = ExitCode::FAILURE;
                break;
            }
        };
        debug!(log, "Received data {} bytes", buf.len(); "addr" => addr);
        if let Err(e) = client.handle_packet(buf) {
            error!(log, "Failed to handle packet"; "addr" => addr, "error" => e);
        }
    }
    let cmd: ClientCommand = ClientClose(stream_id).into();
    sock.send_message_to(cmd, &addr)?;

    for handle in handles {
        handle.join().unwrap();
    }

    Ok(exit_code)
}

fn is_running() -> bool {
    RUNNING.load(Ordering::Relaxed)
}

fn stop() {
    background_stop();
    let (lock, cvar) = &NOTIFY;
    let mut running = lock.lock().unwrap();
    *running = false;
    cvar.notify_all();
}

fn background_stop() {
    RUNNING.store(false, Ordering::Relaxed);
}
