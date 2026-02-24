pub mod backend;
pub mod command;
pub mod logging;
pub mod opus;
pub mod packet;
pub mod ringbuf;
pub mod stream;

mod macros;

use std::time::Duration;

pub use backend::{initialize, terminate};

pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
pub const HEARTBEAT_GRACE_PERIOD: Duration = Duration::from_secs(45);
