use bytes::{Buf, BufMut, TryGetError};

use super::{
    codec::{Decode, Encode},
    stream::StreamFlags,
    time::NetworkTime,
};

pub struct HandshakeInit {
    pub flags: StreamFlags,
    pub timestamp: u64,
}

impl Encode for HandshakeInit {
    fn encode(&self, buf: &mut impl BufMut) {
        buf.put_u8(self.flags.into());
        buf.put_u64(self.timestamp);
    }
}

impl Decode for HandshakeInit {
    fn decode(buf: &mut impl Buf) -> Result<Self, TryGetError> {
        Ok(Self {
            flags: StreamFlags::from(buf.try_get_u8()?),
            timestamp: buf.try_get_u64()?,
        })
    }
}

pub type HandshakeAck = NetworkTime;

pub struct HandshakeReply {
    pub flags: StreamFlags,
    pub ack: HandshakeAck,
}

impl Encode for HandshakeReply {
    fn encode(&self, buf: &mut impl BufMut) {
        buf.put_u8(self.flags.into());
        self.ack.encode(buf);
    }
}

impl Decode for HandshakeReply {
    fn decode(buf: &mut impl Buf) -> Result<Self, TryGetError> {
        Ok(Self {
            flags: StreamFlags::from(buf.try_get_u8()?),
            ack: HandshakeAck::decode(buf)?,
        })
    }
}
