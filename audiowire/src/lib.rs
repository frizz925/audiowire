mod audiowire;

pub mod logging;
pub mod opus;

use std::time::Duration;

pub use audiowire::*;

pub const DEFAULT_CONFIG: Config = Config {
    channels: 2,
    sample_rate: 48000,
    sample_format: SampleFormat::S16,
    buffer_duration: Duration::from_millis(20),
};

pub fn convert_slice<S: Sized, T: Sized>(buf: &[S], len: usize) -> &[T] {
    let src_size = std::mem::size_of::<S>();
    let dst_size = std::mem::size_of::<T>();
    unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const T, len * src_size / dst_size) }
}
