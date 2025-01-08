use std::{
    ffi::{c_char, CStr, CString},
    ptr,
};

use audiowire_sys::*;

use super::config::Config;
use super::result::{parse_result_lazy, Result};

impl Into<aw_config> for Config {
    fn into(self) -> aw_config {
        aw_config {
            channels: self.channels,
            sample_rate: self.sample_rate,
            sample_format: self.sample_format as u32,
            buffer_duration: self.buffer_duration.as_millis() as u32,
            max_buffer_duration: self.max_buffer_duration.as_millis() as u32,
        }
    }
}

pub struct BaseStream {
    handle: *mut aw_stream,
    devname: Option<String>,
    running: bool,
}

impl BaseStream {
    fn new(handle: *mut aw_stream) -> Self {
        let devname = unsafe {
            let cstr = aw_device_name(handle);
            if !cstr.is_null() {
                Some(CStr::from_ptr(cstr).to_string_lossy().to_string())
            } else {
                None
            }
        };
        Self {
            handle,
            devname,
            running: true,
        }
    }
}

pub trait StreamInternal {
    fn base(&self) -> &BaseStream;
    fn base_mut(&mut self) -> &mut BaseStream;
}

pub trait Stream: StreamInternal {
    fn capacity(&self) -> usize {
        unsafe { aw_buffer_capacity(self.base().handle) }
    }

    fn device_name(&self) -> Option<&str> {
        self.base().devname.as_deref()
    }

    fn peek(&self) -> usize;

    // Stop is idempotent
    fn stop(&mut self) -> Result<()> {
        let base = self.base_mut();
        if base.running {
            unsafe { parse_result_lazy(aw_stop(base.handle), || base.running = false) }
        } else {
            Ok(())
        }
    }
}

pub struct RecordStream {
    base: BaseStream,
}

impl RecordStream {
    pub fn read(&mut self, buf: &mut [u8]) -> usize {
        unsafe { aw_record_read(self.base.handle, buf.as_mut_ptr() as *mut c_char, buf.len()) }
    }
}

impl StreamInternal for RecordStream {
    fn base(&self) -> &BaseStream {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseStream {
        &mut self.base
    }
}

impl Stream for RecordStream {
    fn peek(&self) -> usize {
        unsafe { aw_record_peek(self.base.handle) }
    }
}

unsafe impl Sync for RecordStream {}
unsafe impl Send for RecordStream {}

pub fn start_record(name: Option<&str>, cfg: Config) -> Result<RecordStream> {
    let mut handle: *mut aw_stream = ptr::null_mut();
    let result = unsafe {
        let cname = name.map(|s| CString::new(s).unwrap());
        let name_ptr = cname.as_ref().map(|s| s.as_ptr()).unwrap_or(ptr::null());
        aw_start_record(&mut handle, name_ptr, cfg.into())
    };
    parse_result_lazy(result, || RecordStream {
        base: BaseStream::new(handle),
    })
}

pub struct PlaybackStream {
    base: BaseStream,
}

impl PlaybackStream {
    pub fn write(&mut self, buf: &[u8]) -> usize {
        unsafe { aw_playback_write(self.base.handle, buf.as_ptr() as *mut c_char, buf.len()) }
    }
}

impl StreamInternal for PlaybackStream {
    fn base(&self) -> &BaseStream {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseStream {
        &mut self.base
    }
}

impl Stream for PlaybackStream {
    fn peek(&self) -> usize {
        unsafe { aw_playback_peek(self.base.handle) }
    }
}

unsafe impl Sync for PlaybackStream {}
unsafe impl Send for PlaybackStream {}

pub fn start_playback(name: Option<&str>, cfg: Config) -> Result<PlaybackStream> {
    let mut handle: *mut aw_stream = ptr::null_mut();
    let result = unsafe {
        let cname = name.map(|s| CString::new(s).unwrap());
        let name_ptr = cname.as_ref().map(|s| s.as_ptr()).unwrap_or(ptr::null());
        aw_start_playback(&mut handle, name_ptr, cfg.into())
    };
    parse_result_lazy(result, || PlaybackStream {
        base: BaseStream::new(handle),
    })
}
