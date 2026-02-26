use std::time::SystemTime;

use audiowire_derive::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct NetworkTime {
    pub rec_timestamp: SystemTime,
    pub xmt_timestamp: SystemTime,
}
