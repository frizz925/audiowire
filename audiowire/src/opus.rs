use opus::Channels;

pub trait ChannelsParser {
    fn from_u8(value: u8) -> Channels;
}

impl ChannelsParser for Channels {
    fn from_u8(value: u8) -> Channels {
        match value {
            1 => Channels::Mono,
            2 => Channels::Stereo,
            other => panic!("Unsupported number of channels: {}", other),
        }
    }
}
