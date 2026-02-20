use std::sync::atomic::AtomicU8;

use audiowire_serde::{Deserialize, Serialize};
use bytes::{Buf, BufMut, TryGetError};

pub type StreamId = u8;

pub type AtomicStreamId = AtomicU8;

pub struct StreamFlags {
    pub source_enabled: bool,
    pub sink_enabled: bool,
    pub opus_enabled: bool,
}

impl StreamFlags {
    const SOURCE: u8 = 0b0001;
    const SINK: u8 = 0b0010;
    const OPUS: u8 = 0b0100;
}

impl Serialize for StreamFlags {
    fn serialize(&self, buf: &mut impl BufMut) {
        buf.put_u8(self.raw());
    }
}

impl Deserialize for StreamFlags {
    fn deserialize(buf: &mut impl Buf) -> Result<Self, TryGetError> {
        let flags = buf.try_get_u8()?;
        Ok(Self {
            source_enabled: flags & Self::SOURCE != 0,
            sink_enabled: flags & Self::SINK != 0,
            opus_enabled: flags & Self::OPUS != 0,
        })
    }
}

impl StreamFlags {
    pub fn raw(&self) -> u8 {
        let mut flags = 0u8;
        if self.source_enabled {
            flags |= Self::SOURCE;
        }
        if self.sink_enabled {
            flags |= Self::SINK;
        }
        if self.opus_enabled {
            flags |= Self::OPUS;
        }
        flags
    }
}
