use std::time::{Duration, SystemTime, SystemTimeError};

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
        org_timestamp: SystemTime,
        rec_timestamp: SystemTime,
    ) -> Result<Self, SystemTimeError> {
        let local_dur = rec_timestamp.duration_since(org_timestamp)?;
        let remote_dur = time.xmt_timestamp.duration_since(time.rec_timestamp)?;
        Ok(Self {
            rtt: local_dur.abs_diff(remote_dur),
            record,
            playback,
        })
    }

    pub fn write(&mut self, buf: &[u8]) -> opus::Result<()> {
        if let Some(playback) = self.playback.as_mut() {
            playback.write(buf)?;
        }
        Ok(())
    }
}
