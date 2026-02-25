use std::{
    fmt::Display,
    io::{Read, Result, Write},
    sync::atomic::AtomicU8,
};

use audiowire_serde::{Deserialize, Serialize};

pub type StreamId = u8;

pub type AtomicStreamId = AtomicU8;

#[derive(Clone, Copy, Debug)]
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

impl slog::Value for StreamFlags {
    fn serialize(
        &self,
        _rec: &slog::Record<'_>,
        key: slog::Key,
        serializer: &mut dyn slog::Serializer,
    ) -> slog::Result {
        serializer.emit_str(key, self.to_string().as_str())
    }
}

impl Display for StreamFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "StreamFlags(source={} sink={} opus={})",
            self.source_enabled, self.sink_enabled, self.opus_enabled
        )
    }
}

impl Serialize for StreamFlags {
    fn serialize<W: Write>(&self, writer: W) -> Result<()> {
        self.raw().serialize(writer)
    }
}

impl Deserialize for StreamFlags {
    fn deserialize<R: Read>(reader: R) -> Result<Self> {
        let flags = u8::deserialize(reader)?;
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
