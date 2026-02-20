use std::{
    net::{SocketAddr, ToSocketAddrs},
    process::ExitCode,
};

use anyhow::{Ok as _Ok, Result};
use audiowire::{
    command::{DeviceConfig, add_device_args},
    logging,
    packet::{
        handshake::{HandshakeAck, HandshakeInit, HandshakeReply},
        message::{DecodedMessage, Pack},
        stream::StreamFlags,
        time::{NetworkTime, get_current_timestamp},
    },
};
use audiowire_serde::Deserialize;
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

    let init = HandshakeInit {
        flags: StreamFlags {
            source_enabled,
            sink_enabled,
            opus_enabled,
        },
        timestamp: get_current_timestamp(),
    };
    sock.send_to(init.pack().as_ref(), &addr).await?;

    // TODO: Check if packet coming from server address
    let (_, addr) = sock.recv_buf_from(&mut buf).await?;
    let receive_timestamp = get_current_timestamp();
    let message = DecodedMessage::deserialize(&mut buf)?;
    buf.clear();

    let HandshakeReply {
        id: stream_id,
        flags,
        time,
    } = if let DecodedMessage::HandshakeReply(reply) = message {
        reply
    } else {
        error!("We should get handshake reply here");
        return _Ok(ExitCode::FAILURE);
    };

    info!(
        stream_id,
        stream_flags = flags.raw(),
        origin_timestamp = time.origin_timestamp,
        receive_timestamp = time.receive_timestamp,
        transmit_timestamp = time.transmit_timestamp;
        "Got handshake reply"
    );

    let ack = HandshakeAck {
        id: stream_id,
        time: NetworkTime {
            origin_timestamp: time.transmit_timestamp,
            receive_timestamp,
            transmit_timestamp: get_current_timestamp(),
        },
    };
    sock.send_to(ack.pack().as_ref(), addr).await?;

    _Ok(ExitCode::SUCCESS)
}
