mod audiowire;

pub mod handlers;
pub mod logging;
pub mod opus;
pub mod peer;

pub use audiowire::*;

pub const DEFAULT_CONFIG: Config = Config {
    channels: 2,
    sample_rate: 48000,
    sample_format: SampleFormat::S16,
    buffer_frames: 960,
    max_buffer_frames: 14400,
};
