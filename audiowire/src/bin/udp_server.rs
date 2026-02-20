use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use anyhow::{Ok as _Ok, Result};
use audiowire::{
    command::{add_device_args, DeviceConfig},
    logging,
    packet::{
        codec::{Decode, Encode},
        handshake::{HandshakeAck, HandshakeReply},
        message::{DecodedMessage, EncodedMessage},
        stream::{StreamFlags, StreamType},
        time::get_current_timestamp,
    },
};
use bytes::BytesMut;
use clap::{value_parser, Arg, Command};
use log::{error, info, warn};
use tokio::net::UdpSocket;

struct Server {
    config: DeviceConfig,
    listener: UdpSocket,
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
    let listener = UdpSocket::bind(addr).await?;
    info!("Server listening at {}", addr.to_string());
    let server = Arc::new(Server { config, listener });

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
            if let Err(e) = handle_packet(config, buf, sock, &addr, receive_timestamp).await {
                error!(error = e.to_string(), addr = addr.to_string(); "Failed to handle packet");
            }
        });
    }

    _Ok(())
}

async fn handle_packet(
    config: DeviceConfig,
    mut buf: BytesMut,
    sock: Arc<UdpSocket>,
    addr: &SocketAddr,
    receive_timestamp: u64,
) -> Result<()> {
    let DeviceConfig {
        source_name: _1,
        sink_name: _2,
        source_enabled,
        sink_enabled,
        opus_enabled,
    } = config.to_owned();
    let message = DecodedMessage::decode(&mut buf)?;
    buf.clear();

    match message {
        DecodedMessage::HandshakeInit(init) => {
            info!(
                addr = addr.to_string(),
                timestamp = init.timestamp;
                "Got handshake init"
            );

            HandshakeReply {
                id: 0,
                flags: StreamFlags::new(source_enabled, sink_enabled, opus_enabled),
                time: HandshakeAck {
                    origin_timestamp: init.timestamp,
                    receive_timestamp,
                    transmit_timestamp: get_current_timestamp(),
                },
            }
            .into::<EncodedMessage>()
            .encode(&mut buf);
            sock.send_to(&buf, addr).await?;
            buf.clear();
        }
        DecodedMessage::HandshakeAck(ack) => {
            info!(
                addr = addr.to_string(),
                origin_timestamp = ack.origin_timestamp,
                receive_timestamp = ack.receive_timestamp,
                transmit_timestamp = ack.transmit_timestamp;
                "Got handshake ack"
            );
        }
        _ => warn!(addr = addr.to_string(); "Got unknown message"),
    }

    Ok(())
}
