use std::time::Duration;

use audiowire_sys::{
    aw_config, aw_sample_format_AW_SAMPLE_FORMAT_F32, aw_sample_format_AW_SAMPLE_FORMAT_S16,
    aw_sample_size,
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
    pub buffer_samples: usize,
    pub max_buffer_samples: usize,
}

impl Config {
    #[inline]
    pub fn buffer_size(&self) -> usize {
        self.sample_count_to_bytes(self.buffer_samples)
    }

    #[inline]
    pub fn buffer_duration(&self) -> Duration {
        self.sample_count_to_duration(self.buffer_samples)
    }

    #[inline]
    pub fn max_buffer_size(&self) -> usize {
        self.sample_count_to_bytes(self.max_buffer_samples)
    }

    #[inline]
    pub fn max_buffer_duration(&self) -> Duration {
        self.sample_count_to_duration(self.max_buffer_samples)
    }

    #[inline]
    fn sample_count_to_bytes(&self, count: usize) -> usize {
        count * (self.channels as usize) * self.sample_format.size()
    }

    #[inline]
    fn sample_count_to_duration(&self, count: usize) -> Duration {
        let ms = count * 1000 / (self.sample_rate as usize);
        Duration::from_millis(ms as u64)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            channels: 2,
            sample_rate: 48000,
            sample_format: SampleFormat::S16,
            buffer_samples: 960,
            max_buffer_samples: 14400,
        }
    }
}

impl Into<aw_config> for Config {
    fn into(self) -> aw_config {
        aw_config {
            channels: self.channels,
            sample_rate: self.sample_rate,
            sample_format: self.sample_format as u32,
            buffer_samples: self.buffer_samples as u32,
            max_buffer_samples: self.max_buffer_samples as u32,
        }
    }
}
