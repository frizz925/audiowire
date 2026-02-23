use std::time::SystemTime;

use audiowire_derive::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct NetworkTime {
    pub rec_timestamp: u64,
    pub xmt_timestamp: u64,
}

pub fn get_current_timestamp() -> u64 {
    SystemTime::UNIX_EPOCH
        .elapsed()
        .unwrap()
        .as_millis()
        .try_into()
        .expect("Timestamp doesn't fit into an 64-bit unsigned integer")
}
