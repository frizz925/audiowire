use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    ops::{Deref, DerefMut},
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
            server::{SERVER_CLOSE, SERVER_HEARTBEAT},
        },
        data::{IncomingClientData, OutgoingServerData},
        handshake::{Handshake, HandshakeAck, HandshakeInit, HandshakeReply},
        message::{IncomingMessage, OutgoingMessage},
        socket::{UdpWrapper, wrap_udp},
        stream::{AtomicStreamId, StreamFlags, StreamId},
        time::{NetworkTime, get_current_timestamp},
    },
    stream::{Peer, handle_playback, handle_record},
};
use audiowire_serde::{Deserialize, Serialize};
use bytes::{Buf, Bytes};
use clap::{Arg, Command, value_parser};
use slog::{Logger, debug, error, info, o, warn};
use tokio::{
    net::{ToSocketAddrs, UdpSocket},
    sync::{Notify, RwLock},
    time::sleep,
};

static RUNNING: AtomicBool = AtomicBool::new(true);
static NOTIFY: Notify = Notify::const_new();

struct Context<'a> {
    log: &'a Logger,
    addr: &'a SocketAddr,
    rec_timestamp: u64,
}

struct Server {
    config: Config,
    device: DeviceConfig,
    sock: Arc<UdpSocket>,

    next_stream_id: AtomicStreamId,
    clients: RwLock<HashMap<StreamId, Client>>,
}

impl Server {
    fn new(config: Config, device: DeviceConfig, sock: UdpSocket) -> Self {
        Self {
            config,
            device,
            sock: Arc::new(sock),

            next_stream_id: AtomicStreamId::new(1),
            clients: RwLock::new(HashMap::new()),
        }
    }

    async fn handle_packet<'a>(&self, context: Context<'a>, mut buf: impl Buf) -> Result<()> {
        let message = IncomingMessage::deserialize(&mut buf)?;
        match message {
            IncomingMessage::Handshake(hs) => {
                self.handle_handshake(context, hs).await?;
            }
            IncomingMessage::ClientCommand(cmd) => {
                self.handle_command(context, cmd).await?;
            }
            IncomingMessage::Data(mut buf) => {
                let data = IncomingClientData::deserialize(&mut buf)?;
                self.handle_data(context, data).await;
            }
            _ => (),
        }
        Ok(())
    }

    async fn handle_handshake<'a>(&self, context: Context<'a>, hs: Handshake) -> Result<()> {
        match hs {
            Handshake::Init(init) => {
                self.handle_handshake_init(context, init).await?;
            }
            Handshake::Ack(ack) => {
                self.handle_handshake_ack(context, ack).await?;
            }
            _ => (),
        }
        Ok(())
    }

    async fn handle_command<'a>(&self, context: Context<'a>, cmd: ClientCommand) -> Result<()> {
        let Context { log, .. } = context;
        match cmd {
            ClientCommand::Heartbeat(ClientHeartbeat(stream_id)) => {
                if let Some(Client::Running(c)) = self.clients.write().await.get_mut(&stream_id) {
                    info!(log, "Received client heartbeat"; "stream_id" => stream_id);
                    c.last_heartbeat = Instant::now();
                }
            }
            ClientCommand::Close(ClientClose(stream_id)) => {
                if let Some(_) = self.clients.write().await.remove(&stream_id) {
                    info!(log, "Client closed"; "stream_id" => stream_id);
                }
            }
            _ => (),
        }
        Ok(())
    }

    async fn handle_data<'a>(&self, context: Context<'a>, data: IncomingClientData<Bytes>) {
        let Context { log, .. } = context;
        let IncomingClientData(stream_id, buf) = data;
        let log = log.new(o!("stream_id" => stream_id));
        if let Some(client) = self.clients.write().await.get_mut(&stream_id) {
            if let Client::Running(c) = client {
                c.write(buf)
                    .map_err(
                        |e| error!(log, "Failed to decode Opus packet"; "error" => e.to_string()),
                    )
                    .ok();
            } else {
                warn!(log, "Received data packet for client that is not running");
            }
        }
    }

    async fn handle_handshake_init<'a>(
        &self,
        context: Context<'a>,
        init: HandshakeInit,
    ) -> Result<()> {
        let DeviceConfig {
            source_enabled,
            sink_enabled,
            opus_enabled,
            ..
        } = self.device.to_owned();
        let Context {
            log,
            addr,
            rec_timestamp,
        } = context;
        let HandshakeInit { flags } = init;
        info!(log, "Got handshake init"; "stream_flags" => flags);

        let stream_id = self.next_stream_id.fetch_add(1, Ordering::Acquire);
        let xmt_timestamp = {
            let org_timestamp = get_current_timestamp();
            let client = ClientHandshake {
                flags,
                org_timestamp,
                last_handshake: Instant::now(),
            }
            .into();
            self.clients.write().await.insert(stream_id, client);
            org_timestamp
        };

        let msg: OutgoingMessage<_> = Handshake::from(HandshakeReply {
            stream_id,
            flags: StreamFlags {
                source_enabled: source_enabled && flags.sink_enabled,
                sink_enabled: sink_enabled && flags.source_enabled,
                opus_enabled: opus_enabled && flags.opus_enabled,
            },
            time: NetworkTime {
                rec_timestamp,
                xmt_timestamp,
            },
        })
        .into();
        self.sock.send_to(msg.into_bytes().as_ref(), addr).await?;
        Ok(())
    }

    async fn handle_handshake_ack<'a>(
        &self,
        context: Context<'a>,
        ack: HandshakeAck,
    ) -> Result<()> {
        let Context {
            log,
            addr,
            rec_timestamp,
        } = context;
        let HandshakeAck { stream_id, time } = ack;
        let log = log.new(o!("stream_id" => stream_id));
        info!(log,
            "Got handshake ack";
            "rec_timestamp" => time.rec_timestamp,
            "xmt_timestamp" => time.xmt_timestamp
        );

        let client = self.clients.write().await.remove(&stream_id);
        let hs = if let Some(client) = client {
            match client {
                Client::Handshake(hs) => hs,
                Client::Running(_) => {
                    warn!(
                        log,
                        "Received handshake ack while the stream is already running"
                    );
                    return Ok(());
                }
            }
        } else {
            warn!(log, "Client not found");
            return Ok(());
        };

        let ClientHandshake {
            flags,
            org_timestamp,
            ..
        } = hs;
        let opus_enabled = self.device.opus_enabled && flags.opus_enabled;

        let record = if self.device.source_enabled && flags.sink_enabled {
            let log = log.new(o!("stream" => "record"));
            let stream = handle_record(
                &log,
                &self.config,
                addr.to_string(),
                self.device.source_name.as_deref(),
                Arc::clone(&self.sock),
                addr.to_owned(),
                move |src, dst| OutgoingMessage::from(OutgoingServerData(src)).serialize(dst),
                opus_enabled,
            )?;
            Some(stream)
        } else {
            None
        };

        let playback = if self.device.sink_enabled && flags.source_enabled {
            let log = log.new(o!("stream" => "playback"));
            let stream = handle_playback(
                &log,
                &self.config,
                addr.to_string(),
                self.device.sink_name.as_deref(),
                opus_enabled,
            )?;
            Some(stream)
        } else {
            None
        };

        let client = ClientRunning {
            inner: Peer::new(record, playback, &time, org_timestamp, rec_timestamp),
            addr: addr.to_owned(),
            last_heartbeat: Instant::now(),
        }
        .into();
        self.clients.write().await.insert(stream_id, client);

        Ok(())
    }
}

enum Client {
    Handshake(ClientHandshake),
    Running(ClientRunning),
}

impl From<ClientHandshake> for Client {
    fn from(value: ClientHandshake) -> Self {
        Self::Handshake(value)
    }
}

impl From<ClientRunning> for Client {
    fn from(value: ClientRunning) -> Self {
        Self::Running(value)
    }
}

struct ClientHandshake {
    flags: StreamFlags,
    org_timestamp: u64,
    last_handshake: Instant,
}

struct ClientRunning {
    inner: Peer,
    addr: SocketAddr,
    last_heartbeat: Instant,
}

impl Deref for ClientRunning {
    type Target = Peer;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for ClientRunning {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

struct HeartbeatWorker {
    log: Logger,
    server: Arc<Server>,
}

impl HeartbeatWorker {
    async fn run(self) {
        let mut sock = wrap_udp(&self.server.sock);
        while RUNNING.load(Ordering::Relaxed) {
            let sock = sock.enter();
            tokio::select! {
                _ = sleep(HEARTBEAT_INTERVAL) => (),
                _ = NOTIFY.notified() => break
            }
            self.check(sock).await;
        }
    }

    async fn check<U, S>(&self, mut sock: S)
    where
        U: UdpWrapper,
        S: AsMut<U>,
    {
        let dead = {
            let mut stream_ids = Vec::new();
            for (stream_id, client) in self.server.clients.read().await.iter() {
                let last_instant = match client {
                    Client::Handshake(c) => c.last_handshake,
                    Client::Running(c) => {
                        self.pulse(&mut sock, c.addr, *stream_id).await;
                        c.last_heartbeat
                    }
                };
                if last_instant.elapsed() > HEARTBEAT_GRACE_PERIOD {
                    stream_ids.push(*stream_id);
                }
            }
            stream_ids
        };
        if dead.len() > 0 {
            let mut clients = self.server.clients.write().await;
            for stream_id in dead {
                clients.remove(&stream_id);
                info!(
                    self.log, "Removed client due to inactivity";
                    "stream_id" => stream_id
                );
            }
        }
    }

    async fn pulse<U, S, A>(&self, mut sock: S, addr: A, stream_id: StreamId)
    where
        U: UdpWrapper,
        S: AsMut<U>,
        A: ToSocketAddrs,
    {
        sock.as_mut()
            .send_message_to(SERVER_HEARTBEAT, addr)
            .await
            .map_err(|e| {
                error!(
                    self.log, "Failed to send heartbeat";
                    "stream_id" => stream_id,
                    "error" => e
                )
            })
            .ok();
    }
}

fn cmd() -> Command {
    let cmd = Command::new("audiowire-udp-server")
        .about("AudioWire server using UDP packets")
        .arg(
            Arg::new("host")
                .long("host")
                .value_parser(value_parser!(IpAddr))
                .default_value("::")
                .help("Host to be used by the server to listen for packets"),
        )
        .arg(
            Arg::new("port")
                .short('p')
                .long("port")
                .value_parser(value_parser!(u16).range(0..65536))
                .default_value("8760")
                .help("Port to be used by the server to listen for packets"),
        );
    add_device_args(cmd)
}

fn main() -> Result<()> {
    let matches = cmd().get_matches();
    let host = matches.get_one("host").map(IpAddr::to_owned).unwrap();
    let port = matches.get_one("port").map(u16::to_owned).unwrap();
    let addr = SocketAddr::new(host, port);

    let device = DeviceConfig::from(&matches);
    let config = Config::default();
    let log = logging::initialize();

    info!(log, "Starting audio check");
    audio_check(&log, &config, &device)?;
    info!(log, "Audio check finished");

    let result = tokio::runtime::Runtime::new()?
        .block_on(async move { run(log, config, device, addr).await });

    audiowire::terminate()?;
    result
}

async fn run(log: Logger, config: Config, device: DeviceConfig, addr: SocketAddr) -> Result<()> {
    let sock = UdpSocket::bind(addr).await?;
    info!(log, "Server listening at {}", addr.to_string());

    let server = Arc::new(Server::new(config, device, sock));
    let mut handles = Vec::new();

    // Signal handler
    handles.push(tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        RUNNING.store(false, Ordering::Relaxed);
        NOTIFY.notify_waiters();
    }));

    // Heartbeat handler
    let worker = HeartbeatWorker {
        log: log.new(o!("worker" => "heartbeat")),
        server: Arc::clone(&server),
    };
    handles.push(tokio::spawn(worker.run()));

    let mut sock = wrap_udp(&server.sock);
    while RUNNING.load(Ordering::Relaxed) {
        let mut sock = sock.enter();
        let (buf, addr) = tokio::select! {
            result = sock.raw_recv_from() => result?,
            _ = NOTIFY.notified() => break
        };
        debug!(log, "Received data {} bytes", buf.remaining(); "addr" => addr);
        let rec_timestamp = get_current_timestamp();

        let log = log.new(o!("addr" => addr.to_string()));
        let context = Context {
            log: &log,
            addr: &addr,
            rec_timestamp,
        };
        if let Err(e) = server.handle_packet(context, buf).await {
            error!(log, "Failed to handle packet"; "error" => e.to_string());
        }
    }

    for client in server.clients.read().await.values() {
        if let Client::Running(ClientRunning { addr, .. }) = client {
            sock.send_message_to(SERVER_CLOSE, addr).await?;
        }
    }
    info!(log, "Server stopped listening");

    for handle in handles {
        handle.abort();
        handle.await.ok();
    }

    _Ok(())
}
