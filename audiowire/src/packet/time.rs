use std::time::Duration;

use audiowire_derive::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct NetworkTime {
    pub rec_timestamp: Duration,
    pub xmt_timestamp: Duration,
}
