use std::{net::SocketAddr, sync::LazyLock, time::Instant};

use slog::{Logger, info};

use crate::{
    command::DeviceConfig,
    logging::Timestamp,
    packet::{
        handshake::{Handshake, HandshakeAck, HandshakeInit, HandshakeReply},
        message::IncomingMessage,
        socket::{OwnedUdpWrapper, UdpWrapper},
        stream::{StreamFlags, StreamId},
        time::NetworkTime,
    },
};

pub static TIMESTAMP_EPOCH: LazyLock<Instant> = LazyLock::new(|| Instant::now());

pub struct HandshakeResult {
    pub org_timestamp: Instant,
    pub rec_timestamp: Instant,
    pub stream_id: StreamId,
    pub flags: StreamFlags,
    pub time: NetworkTime,
}

pub fn start_handshake(
    log: &Logger,
    device: &DeviceConfig,
    sock: &mut OwnedUdpWrapper,
    daddr: &SocketAddr,
) -> std::io::Result<HandshakeResult> {
    let DeviceConfig {
        source_enabled,
        sink_enabled,
        opus_enabled,
        ..
    } = *device;

    let org_timestamp = Instant::now();
    let init: Handshake = HandshakeInit {
        flags: StreamFlags {
            source_enabled,
            sink_enabled,
            opus_enabled,
        },
    }
    .into();
    sock.send_message_to(init, &daddr)?;

    let (reply, rec_timestamp) = loop {
        let (message, saddr) = sock.recv_message_from()?;
        let rec_timestamp = Instant::now();
        if saddr.ne(daddr) {
            continue;
        }
        if let IncomingMessage::Handshake(Handshake::Reply(reply)) = message {
            break (reply, rec_timestamp);
        }
    };
    let HandshakeReply {
        stream_id,
        flags,
        time,
    } = reply;

    info!(
        log,
        "Got handshake reply";
        "stream_id" => stream_id,
        "stream_flags" => flags,
        "rec_timestamp" => Timestamp(time.rec_timestamp),
        "xmt_timestamp" => Timestamp(time.xmt_timestamp)
    );

    let xmt_timestamp = Instant::now();
    let ack: Handshake = HandshakeAck {
        stream_id,
        time: NetworkTime {
            rec_timestamp: rec_timestamp.duration_since(org_timestamp),
            xmt_timestamp: xmt_timestamp.duration_since(org_timestamp),
        },
    }
    .into();
    sock.send_message_to(ack, daddr)?;

    Ok(HandshakeResult {
        org_timestamp,
        rec_timestamp,
        stream_id,
        flags,
        time,
    })
}
