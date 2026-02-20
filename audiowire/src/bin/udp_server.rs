use std::{
    net::{IpAddr, SocketAddr},
    sync::{Arc, atomic::Ordering},
};

use anyhow::{Ok as _Ok, Result};
use audiowire::{
    command::{DeviceConfig, add_device_args},
    logging,
    packet::{
        handshake::{HandshakeAck, HandshakeInit, HandshakeReply},
        message::{DecodedMessage, Pack},
        stream::{AtomicStreamId, StreamFlags},
        time::{NetworkTime, get_current_timestamp},
    },
};
use audiowire_serde::Deserialize;
use bytes::{Buf, BytesMut};
use clap::{Arg, Command, value_parser};
use log::{error, info, warn};
use tokio::net::UdpSocket;

struct Server {
    config: DeviceConfig,
    sock: UdpSocket,

    next_stream_id: AtomicStreamId,
}

impl Server {
    fn new(config: DeviceConfig, sock: UdpSocket) -> Self {
        Self {
            config,
            sock,
            next_stream_id: AtomicStreamId::new(1),
        }
    }

    async fn handle_packet(
        &self,
        addr: &SocketAddr,
        mut buf: impl Buf,
        receive_timestamp: u64,
    ) -> Result<()> {
        let DeviceConfig {
            source_name: _1,
            sink_name: _2,
            source_enabled,
            sink_enabled,
            opus_enabled,
        } = self.config.to_owned();
        let message = DecodedMessage::deserialize(&mut buf)?;

        match message {
            DecodedMessage::HandshakeInit(init) => {
                let HandshakeInit { flags, timestamp } = init;
                info!(
                    addr = addr.to_string(),
                    stream_flags = flags.raw(),
                    timestamp;
                    "Got handshake init"
                );

                let reply = HandshakeReply {
                    id: self.next_stream_id.fetch_add(1, Ordering::Acquire),
                    flags: StreamFlags {
                        source_enabled,
                        sink_enabled,
                        opus_enabled,
                    },
                    time: NetworkTime {
                        origin_timestamp: init.timestamp,
                        receive_timestamp,
                        transmit_timestamp: get_current_timestamp(),
                    },
                };
                self.sock.send_to(reply.pack().as_ref(), addr).await?;
            }
            DecodedMessage::HandshakeAck(ack) => {
                let HandshakeAck {
                    id: stream_id,
                    time,
                } = ack;
                info!(
                    addr = addr.to_string(),
                    stream_id,
                    origin_timestamp = time.origin_timestamp,
                    receive_timestamp = time.receive_timestamp,
                    transmit_timestamp = time.transmit_timestamp;
                    "Got handshake ack"
                );
            }
            DecodedMessage::Unknown(code) => {
                warn!(addr = addr.to_string(); "Got unknown message code: {}", code)
            }
            _ => (),
        }

        Ok(())
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
    logging::initialize();

    let matches = cmd().get_matches();
    let host = matches.get_one("host").map(IpAddr::to_owned).unwrap();
    let port = matches.get_one("port").map(u16::to_owned).unwrap();
    let addr = SocketAddr::new(host, port);

    let config = DeviceConfig::from(&matches);
    tokio::runtime::Runtime::new()?.block_on(async move { run(config, addr).await })
}

async fn run(config: DeviceConfig, addr: SocketAddr) -> Result<()> {
    let sock = UdpSocket::bind(addr).await?;
    info!("Server listening at {}", addr.to_string());
    let server = Arc::new(Server::new(config, sock));

    let listener = &server.sock;
    loop {
        let mut buf = BytesMut::with_capacity(2048);
        let (_, addr) = tokio::select! {
            result = listener.recv_buf_from(&mut buf) => {
                result?
            }
            _ = tokio::signal::ctrl_c() => {
                break;
            }
        };
        let receive_timestamp = get_current_timestamp();

        let server = Arc::clone(&server);
        tokio::spawn(async move {
            if let Err(e) = server.handle_packet(&addr, buf, receive_timestamp).await {
                error!(error = e.to_string(), addr = addr.to_string(); "Failed to handle packet");
            }
        });
    }

    _Ok(())
}
