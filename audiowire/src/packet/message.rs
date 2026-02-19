use bytes::{Buf, BufMut};

use super::{
    codec::{Decode, Encode},
    handshake::{HandshakeAck, HandshakeInit, HandshakeReply},
};

pub enum Message {
    Unknown,
    HandshakeInit(HandshakeInit),
    HandshakeAck(HandshakeAck),
    HandshakeReply(HandshakeReply),
}

pub trait IntoMessage {
    fn into_message(self) -> Message;
}

impl Message {
    fn code(&self) -> u8 {
        match self {
            Self::Unknown => 0,
            Self::HandshakeInit(_) => 1,
            Self::HandshakeAck(_) => 2,
            Self::HandshakeReply(_) => 3,
        }
    }
}

impl Encode for Message {
    fn encode(&self, buf: &mut impl BufMut) {
        buf.put_u8(self.code());
        match self {
            Self::Unknown => (),
            Self::HandshakeInit(p) => p.encode(buf),
            Self::HandshakeAck(p) => p.encode(buf),
            Self::HandshakeReply(p) => p.encode(buf),
        }
    }
}

impl Decode for Message {
    fn decode(buf: &mut impl Buf) -> Result<Self, bytes::TryGetError> {
        let message = match buf.get_u8() {
            1 => Self::HandshakeInit(HandshakeInit::decode(buf)?),
            2 => Self::HandshakeAck(HandshakeAck::decode(buf)?),
            3 => Self::HandshakeReply(HandshakeReply::decode(buf)?),
            _ => Self::Unknown,
        };
        Ok(message)
    }
}

impl From<HandshakeInit> for Message {
    fn from(value: HandshakeInit) -> Self {
        Self::HandshakeInit(value)
    }
}

impl From<HandshakeAck> for Message {
    fn from(value: HandshakeAck) -> Self {
        Self::HandshakeAck(value)
    }
}

impl From<HandshakeReply> for Message {
    fn from(value: HandshakeReply) -> Self {
        Self::HandshakeReply(value)
    }
}
