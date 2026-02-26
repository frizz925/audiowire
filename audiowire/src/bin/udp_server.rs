use std::{
    collections::HashMap,
    io::{ErrorKind, Read},
    net::{IpAddr, SocketAddr, ToSocketAddrs, UdpSocket},
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
            server::{SERVER_CLOSE, SERVER_HEARTBEAT},
        },
        data::{IncomingAudioData, IncomingClientData, OutgoingAudioData, OutgoingServerData},
        handshake::{Handshake, HandshakeAck, HandshakeInit, HandshakeReply},
        message::{IncomingMessage, OutgoingMessage},
        socket::{SharedUdpWrapper, UdpWrapper, wrap_udp},
        stream::{AtomicStreamId, StreamFlags, StreamId},
        time::NetworkTime,
    },
    stream::{Peer, handle_playback, handle_record},
};
use audiowire_serde::Serialize;
use clap::{Arg, Command, value_parser};
use slog::{Logger, debug, error, info, o, trace, warn};

static NOTIFY: (Mutex<bool>, Condvar) = (Mutex::new(true), Condvar::new());
static RUNNING: AtomicBool = AtomicBool::new(true);

type SharedClientMap = Arc<RwLock<HashMap<StreamId, Client>>>;

type UdpWrapperShared = SharedUdpWrapper<Arc<UdpSocket>>;

struct Context<'a> {
    log: &'a Logger,
    addr: &'a SocketAddr,
    rec_timestamp: SystemTime,
}

struct Server {
    config: Config,
    device: DeviceConfig,
    sock: UdpWrapperShared,

    next_stream_id: AtomicStreamId,
    clients: SharedClientMap,
}

impl Server {
    fn new(
        config: Config,
        device: DeviceConfig,
        sock: Arc<UdpSocket>,
        clients: SharedClientMap,
    ) -> Self {
        Self {
            config,
            device,
            sock: wrap_udp(sock),

            next_stream_id: AtomicStreamId::new(1),
            clients,
        }
    }

    fn handle_packet<'a, R: Read>(&mut self, context: Context<'a>, reader: R) -> Result<()> {
        let message = IncomingMessage::deserialize(reader)?;
        match message {
            IncomingMessage::Handshake(hs) => {
                self.handle_handshake(context, hs)?;
            }
            IncomingMessage::ClientCommand(cmd) => {
                self.handle_command(context, cmd)?;
            }
            IncomingMessage::Data(reader) => {
                let data = IncomingClientData::deserialize(reader)?;

                self.handle_data(context, data)?;
            }
            _ => (),
        }
        Ok(())
    }

    fn handle_handshake<'a>(&mut self, context: Context<'a>, hs: Handshake) -> Result<()> {
        match hs {
            Handshake::Init(init) => {
                self.handle_handshake_init(context, init)?;
            }
            Handshake::Ack(ack) => {
                self.handle_handshake_ack(context, ack)?;
            }
            _ => (),
        }
        Ok(())
    }

    fn handle_command<'a>(&self, context: Context<'a>, cmd: ClientCommand) -> Result<()> {
        let Context { log, addr, .. } = context;
        match cmd {
            ClientCommand::Heartbeat(ClientHeartbeat(stream_id)) => {
                if let Some(Client::Running(c)) = self.clients.write().unwrap().get_mut(&stream_id)
                {
                    let log = log.new(o!("stream_id" => stream_id));
                    debug!(log, "Received client heartbeat");
                    c.maybe_update_addr(&log, addr);
                    c.last_heartbeat = Instant::now();
                }
            }
            ClientCommand::Close(ClientClose(stream_id)) => {
                if let Some(_) = self.clients.write().unwrap().remove(&stream_id) {
                    info!(log, "Client closed"; "stream_id" => stream_id);
                }
            }
            _ => (),
        }
        Ok(())
    }

    fn handle_data<'a, R: Read>(
        &self,
        context: Context<'a>,
        data: IncomingClientData<R>,
    ) -> std::io::Result<()> {
        let Context { log, .. } = context;
        let IncomingClientData(stream_id, reader) = data;
        let log = log.new(o!("stream_id" => stream_id));
        if let Some(client) = self.clients.write().unwrap().get_mut(&stream_id) {
            if let Client::Running(c) = client {
                c.handle_data(&log, reader)?;
            } else {
                warn!(log, "Received data packet for client that is not running");
            }
        }
        Ok(())
    }

    fn handle_handshake_init<'a>(
        &mut self,
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
            let org_timestamp = SystemTime::now();
            let client = ClientHandshake {
                flags,
                org_timestamp,
                last_handshake: Instant::now(),
            }
            .into();
            self.clients.write().unwrap().insert(stream_id, client);
            org_timestamp
        };

        let reply: Handshake = HandshakeReply {
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
        }
        .into();
        self.sock.send_message_to(reply, addr)?;
        Ok(())
    }

    fn handle_handshake_ack<'a>(&self, context: Context<'a>, ack: HandshakeAck) -> Result<()> {
        let Context {
            log,
            addr,
            rec_timestamp,
        } = context;
        let HandshakeAck { stream_id, time } = ack;
        let log = log.new(o!("stream_id" => stream_id));
        info!(log,
            "Got handshake ack";
            "rec_timestamp" => logging::Timestamp(time.rec_timestamp),
            "xmt_timestamp" => logging::Timestamp(time.xmt_timestamp)
        );

        let client = self.clients.write().unwrap().remove(&stream_id);
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

        let mut sequence = 0;
        let record = if self.device.source_enabled && flags.sink_enabled {
            let log = log.new(o!("stream" => "record"));
            let stream = handle_record(
                &log,
                &self.config,
                addr.to_string(),
                self.device.source_name.as_deref(),
                Arc::clone(self.sock.as_inner()),
                addr.to_owned(),
                move |src, dst| {
                    sequence += 1;
                    OutgoingMessage::from(OutgoingServerData(OutgoingAudioData {
                        sequence,
                        timestamp: SystemTime::now(),
                        data: src,
                    }))
                    .serialize(dst)
                },
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
                &time,
                org_timestamp,
                rec_timestamp,
                opus_enabled,
            )?;
            Some(stream)
        } else {
            None
        };

        let client = ClientRunning {
            inner: Peer { record, playback },
            addr: addr.to_owned(),
            last_heartbeat: Instant::now(),
        }
        .into();
        self.clients.write().unwrap().insert(stream_id, client);

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
    org_timestamp: SystemTime,
    last_handshake: Instant,
}

struct ClientRunning {
    inner: Peer,
    addr: SocketAddr,
    last_heartbeat: Instant,
}

impl ClientRunning {
    fn handle_data<R: Read>(&mut self, log: &Logger, reader: R) -> std::io::Result<()> {
        self.write(IncomingAudioData::deserialize(reader)?)
            .map_err(
                |e| error!(log, "Failed to write incoming audio data"; "error" => e.to_string()),
            )
            .ok();
        Ok(())
    }

    fn maybe_update_addr(&mut self, log: &Logger, addr: &SocketAddr) {
        if self.addr.eq(addr) {
            return;
        }
        let addr = addr.to_owned();
        info!(log, "Client address changed"; "old_addr" => self.addr);
        if let Some(record) = self.record.as_ref() {
            record.update_addr(addr);
            self.addr = addr;
        }
    }
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
    sock: UdpWrapperShared,
    clients: SharedClientMap,
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
            self.check();
        }
    }

    fn check(&mut self) {
        let Self { log, sock, clients } = self;
        let dead = {
            let mut stream_ids = Vec::new();
            for (stream_id, client) in clients.read().unwrap().iter() {
                let last_instant = match client {
                    Client::Handshake(c) => c.last_handshake,
                    Client::Running(c) => {
                        Self::pulse(log, sock, c.addr, *stream_id);
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
            let mut clients = self.clients.write().unwrap();
            for stream_id in dead {
                clients.remove(&stream_id);
                info!(
                    log, "Removed client due to inactivity";
                    "stream_id" => stream_id
                );
            }
        }
    }

    fn pulse<A: ToSocketAddrs>(
        log: &Logger,
        sock: &mut UdpWrapperShared,
        addr: A,
        stream_id: StreamId,
    ) {
        sock.send_message_to(SERVER_HEARTBEAT, addr)
            .map_err(|e| {
                error!(
                    log, "Failed to send heartbeat";
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

fn main() -> Result<ExitCode> {
    let matches = cmd().get_matches();
    let host = matches.get_one("host").map(IpAddr::to_owned).unwrap();
    let port = matches.get_one("port").map(u16::to_owned).unwrap();
    let addr = SocketAddr::new(host, port);

    let device = DeviceConfig::from(&matches);
    let config = Config::default();
    let log = logging::initialize();

    audiowire::initialize()?;
    info!(log, "Starting audio check");
    audio_check(&log, &config, &device)?;
    info!(log, "Audio check finished");

    let result = run(log, config, device, addr);

    audiowire::terminate()?;
    result
}

fn run(log: Logger, config: Config, device: DeviceConfig, addr: SocketAddr) -> Result<ExitCode> {
    let sock = Arc::new(UdpSocket::bind(addr)?);
    info!(log, "Server listening at {}", addr.to_string());
    sock.set_nonblocking(true)?;

    // Signal handler
    ctrlc::set_handler(|| {
        RUNNING.store(false, Ordering::Relaxed);
        let (lock, cvar) = &NOTIFY;
        let mut running = lock.lock().unwrap();
        *running = false;
        cvar.notify_all();
    })
    .unwrap();

    let clients = Arc::new(RwLock::new(HashMap::new()));
    let interval = config.buffer_duration() / 4;
    let mut server = Server::new(config, device, Arc::clone(&sock), Arc::clone(&clients));
    let mut handles = Vec::new();

    // Heartbeat handler
    let worker = HeartbeatWorker {
        log: log.new(o!("worker" => "heartbeat")),
        sock: wrap_udp(Arc::clone(&sock)),
        clients: Arc::clone(&clients),
    };
    handles.push(thread::spawn(|| worker.run()));

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
        trace!(log, "Received data {} bytes", buf.len(); "addr" => addr);
        let rec_timestamp = SystemTime::now();

        let log = log.new(o!("addr" => addr));
        let context = Context {
            log: &log,
            addr: &addr,
            rec_timestamp,
        };
        if let Err(e) = server.handle_packet(context, buf) {
            error!(log, "Failed to handle packet"; "error" => e.to_string());
        }
    }

    for client in server.clients.read().unwrap().values() {
        if let Client::Running(ClientRunning { addr, .. }) = client {
            sock.send_message_to(SERVER_CLOSE, addr)?;
        }
    }
    info!(log, "Server stopped listening");

    for handle in handles {
        handle.join().unwrap();
    }

    _Ok(exit_code)
}

fn is_running() -> bool {
    RUNNING.load(Ordering::Acquire)
}
