use std::{
    net::{SocketAddr, ToSocketAddrs},
    process::ExitCode,
    sync::Arc,
};

use anyhow::{Ok as _Ok, Result};
use audiowire::{
    backend::config::Config,
    command::{DeviceConfig, add_device_args},
    logging,
    packet::{
        command::{Command as CommandPacket, CommandClose},
        data::{IncomingServerData, OutgoingClientData},
        handshake::{Handshake, HandshakeAck, HandshakeInit, HandshakeReply},
        message::{IncomingMessage, OutgoingMessage},
        socket::{UdpWrapper, wrap_udp, wrap_udp_owned},
        stream::StreamFlags,
        time::{NetworkTime, get_current_timestamp},
    },
    stream::{Peer, handle_playback, handle_record},
};
use audiowire_serde::{Deserialize, Serialize};
use bytes::Bytes;
use clap::{Arg, Command};
use slog::{Logger, debug, error, info, o};
use tokio::{net::UdpSocket, sync::mpsc};

struct Client {
    inner: Peer,
    running: bool,
}

impl Client {
    async fn handle_packet(&mut self, mut buf: Bytes) -> Result<()> {
        match IncomingMessage::deserialize(&mut buf)? {
            IncomingMessage::Data(mut buf) => {
                let data = IncomingServerData::deserialize(&mut buf)?;
                self.handle_data(data);
            }
            IncomingMessage::Command(CommandPacket::Close(_)) => {
                self.running = false;
            }
            _ => (),
        }
        Ok(())
    }

    fn handle_data(&self, data: IncomingServerData<Bytes>) {
        self.as_ref().write(data.0);
    }
}

impl AsRef<Peer> for Client {
    fn as_ref(&self) -> &Peer {
        &self.inner
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
    let mut sock = wrap_udp_owned(UdpSocket::bind(":::0").await?);

    let org_timestamp = get_current_timestamp();
    let init: Handshake = HandshakeInit {
        flags: StreamFlags {
            source_enabled,
            sink_enabled,
            opus_enabled,
        },
    }
    .into();
    sock.send_message_to(init, &saddr).await?;

    let (message, addr) = sock.recv_message_from().await?;
    let rec_timestamp = get_current_timestamp();

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

    info!(
        log,
        "Got handshake reply";
        "stream_id" => stream_id,
        "stream_flags" => flags.raw(),
        "rec_timestamp" => time.rec_timestamp,
        "xmt_timestamp" => time.xmt_timestamp
    );

    let ack: Handshake = HandshakeAck {
        stream_id,
        time: NetworkTime {
            rec_timestamp,
            xmt_timestamp: get_current_timestamp(),
        },
    }
    .into();
    sock.send_message_to(ack, &saddr).await?;

    let sock = Arc::new(sock.into_inner());
    let record = if config.source_enabled && flags.sink_enabled {
        let log = log.new(o!("stream" => "record"));
        let stream = handle_record(
            &log,
            Config::default(),
            name.as_str(),
            source_name,
            Arc::clone(&sock),
            addr,
            move |src, dst| {
                OutgoingMessage::from(OutgoingClientData(stream_id, src)).serialize(dst)
            },
        )?;
        debug!(log, "Record running");
        Some(stream)
    } else {
        None
    };

    let playback = if config.sink_enabled && flags.source_enabled {
        let log = log.new(o!("stream" => "playback"));
        let stream = handle_playback(&log, Config::default(), name.as_str(), sink_name)?;
        debug!(log, "Playback running");
        Some(stream)
    } else {
        None
    };

    let mut client = Client {
        inner: Peer::new(record, playback, &time, org_timestamp, rec_timestamp),
        running: true,
    };

    let (tx, mut cancel_rx) = mpsc::channel(1);
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        tx.send(()).await.unwrap();
    });

    let mut sock = wrap_udp(sock);
    while client.running {
        let mut sock = sock.enter();
        tokio::select! {
            result = sock.raw_recv_from() => {
                let (buf, addr) = result?;
                if let Err(e) = client.handle_packet(buf).await {
                    error!(log, "Failed to handle packet"; "addr" => addr, "error" => e);
                }
            }
            _ = cancel_rx.recv() => {
                break;
            }
        }
    }
    let cmd: CommandPacket = CommandClose(stream_id).into();
    sock.send_message_to(cmd, &addr).await?;

    Ok(ExitCode::SUCCESS)
}
