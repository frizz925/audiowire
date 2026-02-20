use std::{
    net::{SocketAddr, ToSocketAddrs},
    process::ExitCode,
};

use anyhow::{Ok as _Ok, Result};
use audiowire::{
    command::{add_device_args, DeviceConfig},
    logging,
    packet::{
        codec::{Decode, Encode},
        handshake::{HandshakeAck, HandshakeInit},
        message::DecodedMessage,
        stream::{StreamFlags, StreamType},
        time::get_current_timestamp,
    },
};
use bytes::BytesMut;
use clap::{Arg, Command};
use log::{error, info};
use tokio::net::UdpSocket;

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
    logging::initialize();

    let matches = cmd().get_matches();
    let addr = matches
        .get_one::<String>("addr")
        .unwrap()
        .to_socket_addrs()?
        .next()
        .expect("Unable to resolve remote address");

    let config = DeviceConfig::from(&matches);
    tokio::runtime::Runtime::new()?.block_on(async move { run(config, addr).await })
}

async fn run(config: DeviceConfig, addr: SocketAddr) -> Result<ExitCode> {
    let DeviceConfig {
        source_name: _1,
        sink_name: _2,
        source_enabled,
        sink_enabled,
        opus_enabled,
    } = config;

    let mut buf = BytesMut::with_capacity(2048);
    let sock = UdpSocket::bind(":::0").await?;

    DecodedMessage::from(HandshakeInit {
        flags: StreamFlags::new(StreamType::new(source_enabled, sink_enabled), opus_enabled),
        timestamp: get_current_timestamp(),
    })
    .encode(&mut buf);
    sock.send_to(&buf, &addr).await?;
    buf.clear();

    // TODO: Check if packet coming from server address
    let (_, addr) = sock.recv_buf_from(&mut buf).await?;
    let receive_timestamp = get_current_timestamp();
    let message = DecodedMessage::decode(&mut buf)?;
    buf.clear();

    let reply = if let DecodedMessage::HandshakeReply(reply) = message {
        reply
    } else {
        error!("We should get handshake reply here");
        return _Ok(ExitCode::FAILURE);
    };
    info!(
        origin_timestamp = reply.time.origin_timestamp,
        receive_timestamp = reply.time.receive_timestamp,
        transmit_timestamp = reply.time.transmit_timestamp;
        "Got handshake reply"
    );

    DecodedMessage::from(HandshakeAck {
        origin_timestamp: reply.time.transmit_timestamp,
        receive_timestamp,
        transmit_timestamp: get_current_timestamp(),
    })
    .encode(&mut buf);
    sock.send_to(&buf, addr).await?;
    buf.clear();

    _Ok(ExitCode::SUCCESS)
}
