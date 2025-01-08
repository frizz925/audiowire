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
    pub buffer_frames: usize,
    pub max_buffer_frames: usize,
}

impl Config {
    #[inline]
    pub fn frame_size(&self) -> usize {
        (self.channels as usize) * self.sample_format.size()
    }

    #[inline]
    pub fn buffer_size(&self) -> usize {
        self.buffer_frames * self.frame_size()
    }

    #[inline]
    pub fn buffer_duration(&self) -> Duration {
        self.frame_count_to_duration(self.buffer_frames)
    }

    #[inline]
    pub fn max_buffer_size(&self) -> usize {
        self.max_buffer_frames * self.frame_size()
    }

    #[inline]
    pub fn max_buffer_duration(&self) -> Duration {
        self.frame_count_to_duration(self.max_buffer_frames)
    }

    #[inline]
    fn frame_count_to_duration(&self, count: usize) -> Duration {
        let ms = count * (self.sample_rate / 1000) as usize;
        Duration::from_millis(ms as u64)
    }
}

impl Into<aw_config> for Config {
    fn into(self) -> aw_config {
        aw_config {
            channels: self.channels,
            sample_rate: self.sample_rate,
            sample_format: self.sample_format as u32,
            buffer_frames: self.buffer_frames as u32,
            max_buffer_frames: self.max_buffer_frames as u32,
        }
    }
}
