use std::time::SystemTime;

use bytes::{Buf, BufMut, TryGetError};

use crate::packet::codec::{Decode, Encode};

pub struct NetworkTime {
    pub origin_timestamp: u64,
    pub receive_timestamp: u64,
    pub transmit_timestamp: u64,
}

impl Encode for NetworkTime {
    fn encode(&self, buf: &mut impl BufMut) {
        buf.put_u64(self.origin_timestamp);
        buf.put_u64(self.receive_timestamp);
        buf.put_u64(self.transmit_timestamp);
    }
}

impl Decode for NetworkTime {
    fn decode(buf: &mut impl Buf) -> Result<Self, TryGetError> {
        Ok(Self {
            origin_timestamp: buf.try_get_u64()?,
            receive_timestamp: buf.try_get_u64()?,
            transmit_timestamp: buf.try_get_u64()?,
        })
    }
}

pub fn get_current_timestamp() -> u64 {
    SystemTime::UNIX_EPOCH
        .elapsed()
        .unwrap()
        .as_millis()
        .try_into()
        .expect("Timestamp doesn't fit into an 64-bit unsigned integer")
}
