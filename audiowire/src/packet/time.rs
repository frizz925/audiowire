use std::{
    ops::Neg,
    time::{Duration, Instant},
};

use audiowire_derive::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct NetworkTime {
    pub rec_timestamp: Duration,
    pub xmt_timestamp: Duration,
}

impl NetworkTime {
    pub fn calculate_rtt(&self, org_timestamp: Instant, rec_timestamp: Instant) -> Duration {
        let local_dur = rec_timestamp.duration_since(org_timestamp);
        let remote_dur = self.xmt_timestamp.saturating_sub(self.rec_timestamp);
        local_dur.abs_diff(remote_dur)
    }

    pub fn calculate_remote_epoch(&self, local_epoch: Instant, rtt: Duration) -> Instant {
        let delta = (self.rec_timestamp.as_millis() as i64) - ((rtt.as_millis() as i64) / 2);
        if delta >= 0 {
            local_epoch - Duration::from_millis(delta as u64)
        } else {
            local_epoch + Duration::from_millis(delta.neg() as u64)
        }
    }
}
