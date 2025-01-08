use std::time::Duration;

use audiowire_sys::{
    aw_sample_format_AW_SAMPLE_FORMAT_F32, aw_sample_format_AW_SAMPLE_FORMAT_S16, aw_sample_size,
};

#[derive(Clone, Copy)]
pub enum SampleFormat {
    S16 = aw_sample_format_AW_SAMPLE_FORMAT_S16 as isize,
    F32 = aw_sample_format_AW_SAMPLE_FORMAT_F32 as isize,
}

impl SampleFormat {
    pub fn size(self) -> usize {
        unsafe { aw_sample_size(self as u32) }
    }
}

#[derive(Clone, Copy)]
pub struct Config {
    pub channels: u8,
    pub sample_rate: u32,
    pub sample_format: SampleFormat,
    pub buffer_duration: Duration,
    pub max_buffer_duration: Duration,
}

impl Config {
    #[inline]
    pub fn frame_size(&self) -> usize {
        (self.channels as usize) * self.sample_format.size()
    }

    #[inline]
    pub fn frame_buffer_count(&self, duration: Duration) -> usize {
        (self.sample_rate / 1000 * (duration.as_millis() as u32)) as usize
    }

    #[inline]
    pub fn frame_buffer_size(&self, duration: Duration) -> usize {
        self.frame_buffer_count(duration) * self.frame_size()
    }
}
