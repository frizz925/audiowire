use bytes::Bytes;
use tokio::sync::mpsc::error::SendError;

use crate::packet::time::NetworkTime;

use super::{playback::PlaybackStream, record::RecordStream};

pub struct Peer {
    pub rtt: u64,
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
            rtt: rec_timestamp
                .abs_diff(org_timestamp)
                .abs_diff(time.xmt_timestamp.abs_diff(time.rec_timestamp)),
            record,
            playback,
        }
    }

    pub async fn write(&self, buf: Bytes) -> Result<(), SendError<Bytes>> {
        if let Some(playback) = self.playback.as_ref() {
            playback.write(buf).await?;
        }
        Ok(())
    }
}
