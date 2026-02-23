use std::{
    net::{SocketAddr, ToSocketAddrs},
    process::ExitCode,
    sync::Arc,
};

use anyhow::{Ok as _Ok, Result};
use audiowire::{
    command::{DeviceConfig, add_device_args},
    logging,
    packet::{
        data::{ClientData, ServerData},
        handshake::{HandshakeAck, HandshakeInit, HandshakeReply},
        message::{DecodedMessage, Pack},
        stream::StreamFlags,
        time::{NetworkTime, get_current_timestamp},
    },
    stream::{Peer, handle_playback, handle_record},
};
use audiowire_serde::Deserialize;
use bytes::{Buf, Bytes, BytesMut};
use clap::{Arg, Command};
use slog::{Logger, debug, error, info, o, warn};
use tokio::{net::UdpSocket, sync::mpsc};

struct Client {
    inner: Peer,
    log: Logger,
}

impl Client {
    async fn handle_packet(&self, mut buf: Bytes) -> Result<()> {
        match DecodedMessage::deserialize(&mut buf)? {
            DecodedMessage::Data(mut buf) => {
                let data = ServerData::deserialize(&mut buf)?;
                self.handle_data(data).await?;
            }
            DecodedMessage::Unknown(code) => {
                warn!(self.log, "Got unknown message code: {}", code);
            }
            _ => (),
        }
        Ok(())
    }

    async fn handle_data(&self, data: ServerData<Bytes>) -> Result<()> {
        self.inner.write(data.0).await?;
        Ok(())
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

    audiowire::initialize()?;

    let log = logging::initialize();
    let config = DeviceConfig::from(&matches);
    let result = tokio::runtime::Runtime::new()?
        .block_on(async move { run(log, config, addr, saddr).await });

    audiowire::terminate()?;

    result
}

async fn run(
    log: Logger,
    config: DeviceConfig,
    name: String,
    saddr: SocketAddr,
) -> Result<ExitCode> {
    let DeviceConfig {
        source_name,
        sink_name,
        source_enabled,
        sink_enabled,
        opus_enabled,
    } = config;

    let mut buf = BytesMut::with_capacity(2048);
    let sock = Arc::new(UdpSocket::bind(":::0").await?);

    let org_timestamp = get_current_timestamp();
    let init = HandshakeInit {
        flags: StreamFlags {
            source_enabled,
            sink_enabled,
            opus_enabled,
        },
    };
    sock.send_to(init.pack().as_ref(), &saddr).await?;

    let (_, addr) = sock.recv_buf_from(&mut buf).await?;
    let rec_timestamp = get_current_timestamp();
    let message = DecodedMessage::deserialize(&mut buf)?;
    buf.clear();

    let HandshakeReply {
        stream_id,
        flags,
        time,
    } = if let DecodedMessage::HandshakeReply(reply) = message {
        reply
    } else {
        error!(log, "We should get handshake reply here");
        return _Ok(ExitCode::FAILURE);
    };

    info!(
        log,
        "Got handshake reply";
        "stream_id" => stream_id,
        "stream_flags" => flags.raw(),
        "rec_timestamp" => time.rec_timestamp,
        "xmt_timestamp" => time.xmt_timestamp
    );

    let ack = HandshakeAck {
        stream_id,
        time: NetworkTime {
            rec_timestamp,
            xmt_timestamp: get_current_timestamp(),
        },
    };
    sock.send_to(ack.pack().as_ref(), &saddr).await?;

    let record = if config.source_enabled && flags.sink_enabled {
        let log = log.new(o!("stream" => "record"));
        let stream = handle_record(
            &log,
            name.as_str(),
            source_name,
            Arc::clone(&sock),
            addr,
            move |buf| ClientData(stream_id, buf).pack(),
        )?;
        debug!(log, "Record running");
        Some(stream)
    } else {
        None
    };

    let playback = if config.sink_enabled && flags.source_enabled {
        let log = log.new(o!("stream" => "playback"));
        let stream = handle_playback(&log, name.as_str(), sink_name)?;
        debug!(log, "Playback running");
        Some(stream)
    } else {
        None
    };
    debug!(log, "Bruh");

    let client = Client {
        inner: Peer::new(record, playback, &time, org_timestamp, rec_timestamp),
        log,
    };

    let (tx, mut cancel_rx) = mpsc::channel(1);
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        tx.send(()).await.unwrap();
    });

    loop {
        let mut buf = BytesMut::with_capacity(65536);
        tokio::select! {
            result = sock.recv_buf(&mut buf) => {
                let recv = result?;
                client.handle_packet(buf.copy_to_bytes(recv)).await?;
            }
            _ = cancel_rx.recv() => {
                break;
            }
        }
    }

    Ok(ExitCode::SUCCESS)
}
