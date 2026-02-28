use std::{net::SocketAddr, time::SystemTime};

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

pub struct HandshakeResult {
    pub org_timestamp: SystemTime,
    pub rec_timestamp: SystemTime,
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

    let org_timestamp = SystemTime::now();
    let init: Handshake = HandshakeInit {
        flags: StreamFlags {
            source_enabled,
            sink_enabled,
            opus_enabled,
        },
    }
    .into();
    sock.send_message_to(init, &daddr)?;

    let HandshakeReply {
        stream_id,
        flags,
        time,
    } = loop {
        let (message, saddr) = sock.recv_message_from()?;
        if saddr.ne(daddr) {
            continue;
        }
        if let IncomingMessage::Handshake(Handshake::Reply(reply)) = message {
            break reply;
        }
    };
    let rec_timestamp = SystemTime::now();

    info!(
        log,
        "Got handshake reply";
        "stream_id" => stream_id,
        "stream_flags" => flags,
        "rec_timestamp" => Timestamp(time.rec_timestamp),
        "xmt_timestamp" => Timestamp(time.xmt_timestamp)
    );

    let ack: Handshake = HandshakeAck {
        stream_id,
        time: NetworkTime {
            rec_timestamp,
            xmt_timestamp: SystemTime::now(),
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
