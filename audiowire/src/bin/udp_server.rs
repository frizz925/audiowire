use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, atomic::Ordering},
};

use anyhow::{Ok as _Ok, Result};
use audiowire::{
    backend::config::Config,
    command::{DeviceConfig, add_device_args},
    logging,
    packet::{
        command::{Command as CommandPacket, CommandClose},
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
use slog::{Logger, error, info, o, warn};
use tokio::{
    net::UdpSocket,
    sync::{
        RwLock,
        mpsc::{self, error::SendError},
    },
};

struct Context<'a> {
    log: &'a Logger,
    addr: &'a SocketAddr,
    rec_timestamp: u64,
}

struct Server {
    config: DeviceConfig,
    sock: Arc<UdpSocket>,

    next_stream_id: AtomicStreamId,
    clients: RwLock<HashMap<StreamId, Client>>,
}

impl Server {
    fn new(config: DeviceConfig, sock: UdpSocket) -> Self {
        Self {
            config,
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
            IncomingMessage::Command(cmd) => {
                self.handle_command(context, cmd).await?;
            }
            IncomingMessage::Data(mut buf) => {
                let data = IncomingClientData::deserialize(&mut buf)?;
                self.handle_data(context, data).await?;
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

    async fn handle_command<'a>(&self, context: Context<'a>, cmd: CommandPacket) -> Result<()> {
        let Context { log, .. } = context;
        match cmd {
            CommandPacket::Close(CommandClose(stream_id)) => {
                self.clients.write().await.remove(&stream_id);
                info!(log, "Client closed"; "stream_id" => stream_id);
            }
            _ => (),
        }
        Ok(())
    }

    async fn handle_data<'a>(
        &self,
        context: Context<'a>,
        data: IncomingClientData<Bytes>,
    ) -> Result<(), SendError<Bytes>> {
        let Context { log, .. } = context;
        let IncomingClientData(stream_id, buf) = data;
        let log = log.new(o!("stream_id" => stream_id));

        if let Some(client) = self.clients.read().await.get(&stream_id) {
            if let Client::Running(c) = client {
                c.inner.write(buf);
            } else {
                warn!(log, "Received data packet for client that is not running");
            }
        }
        Ok(())
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
        } = self.config.to_owned();
        let Context {
            log,
            addr,
            rec_timestamp,
        } = context;
        let HandshakeInit { flags } = init;
        info!(log, "Got handshake init"; "stream_flags" => flags.raw());

        let stream_id = self.next_stream_id.fetch_add(1, Ordering::Acquire);
        let xmt_timestamp = {
            let org_timestamp = get_current_timestamp();
            let client = Client::Handshake(ClientHandshake {
                flags,
                org_timestamp,
            });
            self.clients.write().await.insert(stream_id, client);
            org_timestamp
        };

        let msg = OutgoingMessage::from(Handshake::from(HandshakeReply {
            stream_id,
            flags: StreamFlags {
                source_enabled,
                sink_enabled,
                opus_enabled,
            },
            time: NetworkTime {
                rec_timestamp,
                xmt_timestamp,
            },
        }));
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
        } = hs;

        let record = if self.config.source_enabled && flags.sink_enabled {
            let log = log.new(o!("stream" => "record"));
            let stream = handle_record(
                &log,
                Config::default(),
                addr.to_string(),
                self.config.source_name.as_deref(),
                Arc::clone(&self.sock),
                addr.to_owned(),
                move |src, dst| OutgoingMessage::from(OutgoingServerData(src)).serialize(dst),
            )?;
            Some(stream)
        } else {
            None
        };

        let playback = if self.config.sink_enabled && flags.source_enabled {
            let log = log.new(o!("stream" => "playback"));
            let stream = handle_playback(
                &log,
                Config::default(),
                addr.to_string(),
                self.config.sink_name.as_deref(),
            )?;
            Some(stream)
        } else {
            None
        };

        let client = Client::Running(ClientRunning {
            inner: Peer::new(record, playback, &time, org_timestamp, rec_timestamp),
            addr: addr.to_owned(),
        });
        self.clients.write().await.insert(stream_id, client);

        Ok(())
    }
}

enum Client {
    Handshake(ClientHandshake),
    Running(ClientRunning),
}

struct ClientHandshake {
    flags: StreamFlags,
    org_timestamp: u64,
}

struct ClientRunning {
    inner: Peer,
    addr: SocketAddr,
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
    let log = logging::initialize();
    audiowire::initialize()?;

    let matches = cmd().get_matches();
    let host = matches.get_one("host").map(IpAddr::to_owned).unwrap();
    let port = matches.get_one("port").map(u16::to_owned).unwrap();
    let addr = SocketAddr::new(host, port);

    let config = DeviceConfig::from(&matches);
    let result =
        tokio::runtime::Runtime::new()?.block_on(async move { run(log, config, addr).await });

    audiowire::terminate()?;
    result
}

async fn run(log: Logger, config: DeviceConfig, addr: SocketAddr) -> Result<()> {
    let sock = UdpSocket::bind(addr).await?;
    info!(log, "Server listening at {}", addr.to_string());
    let server = Server::new(config, sock);

    let (tx, mut rx) = mpsc::channel(1);
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        tx.send(()).await.unwrap();
    });

    let mut sock = wrap_udp(&server.sock);
    loop {
        let mut sock = sock.enter();
        let (buf, addr) = tokio::select! {
            result = sock.raw_recv_from() => {
                result?
            }
            _ = rx.recv() => {
                break;
            }
        };
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

    for (stream_id, client) in server.clients.read().await.iter() {
        if let Client::Running(ClientRunning { addr, .. }) = client {
            let cmd: CommandPacket = CommandClose(*stream_id).into();
            sock.send_message_to(cmd, addr).await?;
        }
    }
    info!(log, "Server stopped listening");

    _Ok(())
}
