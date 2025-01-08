mod audiowire;

pub mod handlers;
pub mod logging;
pub mod opus;

use std::time::Duration;

pub use audiowire::*;

pub const DEFAULT_CONFIG: Config = Config {
    channels: 2,
    sample_rate: 48000,
    sample_format: SampleFormat::S16,
    buffer_duration: Duration::from_millis(20),
    max_buffer_duration: Duration::from_millis(300),
};
