#[derive(Clone, Copy)]
pub struct StreamType(u8);

impl StreamType {
    pub fn new(source: bool, sink: bool) -> Self {
        let mut value = 0;
        if source {
            value |= 1;
        }
        if sink {
            value |= 1 << 1;
        }
        Self(value)
    }

    #[inline]
    pub fn is_source(self) -> bool {
        self.0 & 1 != 0
    }

    #[inline]
    pub fn is_sink(self) -> bool {
        self.0 & (1 << 1) != 0
    }

    #[inline]
    pub fn to_bytes(self) -> [u8; 1] {
        [self.0]
    }
}

impl Into<u8> for StreamType {
    fn into(self) -> u8 {
        self.0
    }
}

impl From<&[u8]> for StreamType {
    #[inline]
    fn from(value: &[u8]) -> Self {
        Self(value[0])
    }
}

impl From<[u8; 1]> for StreamType {
    #[inline]
    fn from(value: [u8; 1]) -> Self {
        Self(value[0])
    }
}

#[derive(Clone, Copy)]
pub struct StreamFlags(u8);

impl StreamFlags {
    pub fn new(stream_type: StreamType, opus_enabled: bool) -> Self {
        if opus_enabled {
            Self(stream_type.0 | (1 << 2))
        } else {
            Self(stream_type.0)
        }
    }

    #[inline]
    pub fn stream_type(self) -> StreamType {
        StreamType(self.0)
    }

    #[inline]
    pub fn opus_enabled(self) -> bool {
        self.0 & (1 << 2) != 0
    }

    #[inline]
    pub fn to_bytes(self) -> [u8; 1] {
        [self.0]
    }
}

impl Default for StreamFlags {
    #[inline]
    fn default() -> Self {
        Self(Default::default())
    }
}

impl Into<u8> for StreamFlags {
    #[inline]
    fn into(self) -> u8 {
        self.0
    }
}

impl Into<[u8; 1]> for StreamFlags {
    #[inline]
    fn into(self) -> [u8; 1] {
        self.to_bytes()
    }
}

impl From<u8> for StreamFlags {
    #[inline]
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl From<&[u8]> for StreamFlags {
    #[inline]
    fn from(value: &[u8]) -> Self {
        Self(value[0])
    }
}

impl From<[u8; 1]> for StreamFlags {
    #[inline]
    fn from(value: [u8; 1]) -> Self {
        Self(value[0])
    }
}
