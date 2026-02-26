use std::time::Duration;

use audiowire_sys::{
    aw_config, aw_sample_format_AW_SAMPLE_FORMAT_F32, aw_sample_format_AW_SAMPLE_FORMAT_S16,
    aw_sample_size,
};

use crate::opus::ChannelsParser;

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
    pub fn sample_size(&self) -> usize {
        self.sample_format.size()
    }

    #[inline]
    pub fn frame_size(&self) -> usize {
        self.channels as usize * self.sample_size()
    }

    #[inline]
    pub fn buffer_size(&self) -> usize {
        self.frames_to_bytes(self.buffer_frames)
    }

    #[inline]
    pub fn buffer_duration(&self) -> Duration {
        self.frames_to_duration(self.buffer_frames)
    }

    #[inline]
    pub fn max_buffer_size(&self) -> usize {
        self.frames_to_bytes(self.max_buffer_frames)
    }

    #[inline]
    pub fn max_buffer_duration(&self) -> Duration {
        self.frames_to_duration(self.max_buffer_frames)
    }

    #[inline]
    pub fn opus_channels(&self) -> opus::Channels {
        opus::Channels::from_u8(self.channels)
    }

    #[inline]
    pub fn frames_to_bytes(&self, count: usize) -> usize {
        count * self.frame_size()
    }

    #[inline]
    pub fn frames_to_duration(&self, count: usize) -> Duration {
        let ms = count * 1000 / (self.sample_rate as usize);
        Duration::from_millis(ms as u64)
    }

    #[inline]
    pub fn duration_to_frames(&self, dur: Duration) -> usize {
        self.sample_rate as usize / 1000 * dur.as_millis() as usize
    }

    #[inline]
    pub fn duration_to_bytes(&self, dur: Duration) -> usize {
        self.frame_size() * self.duration_to_frames(dur)
    }

    #[inline]
    pub fn bytes_to_frames(&self, bytes: usize) -> usize {
        bytes / self.frame_size()
    }

    #[inline]
    pub fn bytes_to_duration(&self, bytes: usize) -> Duration {
        self.frames_to_duration(self.bytes_to_frames(bytes))
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            channels: 2,
            sample_rate: 48000,
            sample_format: SampleFormat::S16,
            buffer_frames: 960,
            max_buffer_frames: 14400,
        }
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

#[cfg(test)]
mod test {
    use std::time::Duration;

    use crate::backend::config::Config;

    #[test]
    fn test_config() {
        let config = Config::default();

        assert_eq!(config.sample_size(), 2);
        assert_eq!(config.frame_size(), 4);

        assert_eq!(config.buffer_size(), 3840);
        assert_eq!(config.max_buffer_size(), 57600);
        assert_eq!(config.buffer_duration().as_millis(), 20);
        assert_eq!(config.max_buffer_duration().as_millis(), 300);
        assert_eq!(config.max_buffer_size(), 57600);

        let duration = Duration::from_millis(20);
        assert_eq!(config.duration_to_frames(duration), 960);
        assert_eq!(config.duration_to_bytes(duration), 3840);

        assert_eq!(config.bytes_to_duration(3840).as_millis(), 20);
        assert_eq!(config.bytes_to_frames(3840), 960);
    }
}
