use std::{
    error::Error,
    fmt::{Display, Formatter},
};

use opus::Channels;

#[derive(Debug, Clone)]
pub struct InvalidChannelsError {
    channels: u8,
}

impl InvalidChannelsError {
    fn new(channels: u8) -> Self {
        Self { channels }
    }
}

impl Display for InvalidChannelsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid number of channels: {}", self.channels)
    }
}

impl Error for InvalidChannelsError {}

pub trait ChannelsParser {
    fn from_u8(value: u8) -> Result<Channels, InvalidChannelsError>;
}

impl ChannelsParser for Channels {
    fn from_u8(value: u8) -> Result<Channels, InvalidChannelsError> {
        match value {
            1 => Ok(Channels::Mono),
            2 => Ok(Channels::Stereo),
            other => Err(InvalidChannelsError::new(other)),
        }
    }
}
