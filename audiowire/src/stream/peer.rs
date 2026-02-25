use std::time::Duration;

use crate::packet::time::NetworkTime;

use super::{playback::PlaybackStream, record::RecordStream};

pub struct Peer {
    pub rtt: Duration,
    pub record: Option<RecordStream>,
    pub playback: Option<PlaybackStream>,
}

impl Peer {
    pub fn new(
        record: Option<RecordStream>,
        playback: Option<PlaybackStream>,
        time: &NetworkTime,
        org_timestamp: u64,
        rec_timestamp: u64,
    ) -> Self {
        Self {
            rtt: Duration::from_millis(
                rec_timestamp
                    .abs_diff(org_timestamp)
                    .abs_diff(time.xmt_timestamp.abs_diff(time.rec_timestamp)),
            ),
            record,
            playback,
        }
    }

    pub fn write(&mut self, buf: &[u8]) -> opus::Result<()> {
        if let Some(playback) = self.playback.as_mut() {
            playback.write(buf)?;
        }
        Ok(())
    }
}
