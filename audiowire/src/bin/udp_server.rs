use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, atomic::Ordering},
};

use anyhow::{Ok as _Ok, Result};
use audiowire::{
    command::{DeviceConfig, add_device_args},
    logging,
    packet::{
        data::ClientData,
        handshake::{HandshakeAck, HandshakeInit, HandshakeReply},
        message::{DecodedMessage, Pack},
        stream::{AtomicStreamId, StreamFlags, StreamId},
        time::{NetworkTime, get_current_timestamp},
    },
    stream::{Peer, handle_playback, handle_record},
};
use audiowire_serde::Deserialize;
use bytes::{Buf, Bytes, BytesMut};
use clap::{Arg, Command, value_parser};
use slog::{Logger, debug, error, info, o, warn};
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
        let message = DecodedMessage::deserialize(&mut buf)?;
        match message {
            DecodedMessage::HandshakeInit(init) => {
                self.handle_handshake_init(context, init).await?;
            }
            DecodedMessage::HandshakeAck(ack) => {
                self.handle_handshake_ack(context, ack).await?;
            }
            DecodedMessage::Data(mut buf) => {
                let data = ClientData::deserialize(&mut buf)?;
                self.handle_data(context, data).await?;
            }
            DecodedMessage::Unknown(code) => {
                warn!(context.log, "Got unknown message code: {}", code);
            }
            _ => (),
        }
        Ok(())
    }

    async fn handle_handshake_init<'a>(
        &self,
        context: Context<'a>,
        init: HandshakeInit,
    ) -> std::io::Result<()> {
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

        let reply = HandshakeReply {
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
        };
        self.sock.send_to(reply.pack().as_ref(), addr).await?;
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

        let client = {
            let mut clients = self.clients.write().await;
            clients.remove(&stream_id)
        };
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
                addr.to_string(),
                self.config.source_name.as_deref(),
                Arc::clone(&self.sock),
                addr.to_owned(),
                move |buf| ClientData(stream_id, buf).pack(),
            )?;
            Some(stream)
        } else {
            None
        };

        let playback = if self.config.sink_enabled && flags.source_enabled {
            let stream = handle_playback(&log, addr.to_string(), self.config.sink_name.as_deref())?;
            Some(stream)
        } else {
            None
        };

        let client = Client::Running(ClientRunning::new(
            record,
            playback,
            &time,
            org_timestamp,
            rec_timestamp,
        ));
        let mut clients = self.clients.write().await;
        clients.insert(stream_id, client);
        debug!(log, "Client inserted");

        Ok(())
    }

    async fn handle_data<'a>(
        &self,
        context: Context<'a>,
        data: ClientData<Bytes>,
    ) -> Result<(), SendError<Bytes>> {
        let Context { log, .. } = context;
        let ClientData(stream_id, buf) = data;
        let log = log.new(o!("stream_id" => stream_id));

        if let Some(client) = self.clients.read().await.get(&stream_id) {
            if let Client::Running(c) = client {
                c.write(buf).await?;
            } else {
                warn!(log, "Received data packet for client that is not running");
            }
        }
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

type ClientRunning = Peer;

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
    let server = Arc::new(Server::new(config, sock));

    let (tx, mut rx) = mpsc::channel(1);
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        tx.send(()).await.unwrap();
    });

    debug!(log, "Bruh");
    let listener = &server.sock;
    loop {
        let mut buf = BytesMut::with_capacity(65536);
        let (_, addr) = tokio::select! {
            result = listener.recv_buf_from(&mut buf) => {
                result?
            }
            _ = rx.recv() => {
                break;
            }
        };
        let rec_timestamp = get_current_timestamp();

        let log = log.new(o!("addr" => addr.to_string()));
        let server = Arc::clone(&server);
        tokio::spawn(async move {
            let context = Context {
                log: &log,
                addr: &addr,
                rec_timestamp,
            };
            if let Err(e) = server.handle_packet(context, buf).await {
                error!(log, "Failed to handle packet"; "error" => e.to_string());
            }
        });
    }
    info!(log, "Server stopped listening");

    _Ok(())
}
