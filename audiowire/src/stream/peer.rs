use std::io::Read;

use anyhow::Result;

use crate::packet::data::IncomingAudioData;

use super::{playback::PlaybackStream, record::RecordStream};

pub struct Peer {
    pub record: Option<RecordStream>,
    pub playback: Option<PlaybackStream>,
}

impl Peer {
    pub fn write<R: Read>(&mut self, data: IncomingAudioData<R>) -> Result<()> {
        if let Some(playback) = self.playback.as_mut() {
            playback.write(data)?;
        }
        Ok(())
    }
}
