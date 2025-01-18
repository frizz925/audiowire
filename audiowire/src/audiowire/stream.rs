use std::{
    ffi::{c_char, c_int, CStr, CString},
    os::raw::c_void,
    ptr,
};

use audiowire_sys::*;

use super::result::{parse_result_lazy, Result};
use super::{config::Config, result::parse_result_value};

#[derive(Clone, Copy)]
pub struct StreamType(u8);

impl StreamType {
    pub fn new(source: bool, sink: bool) -> Self {
        let mut value = 0;
        if source {
            value |= 1;
        }
        if sink {
            value |= 1 << 1;
        }
        Self(value)
    }

    #[inline]
    pub fn is_source(self) -> bool {
        self.0 & 1 != 0
    }

    #[inline]
    pub fn is_sink(self) -> bool {
        self.0 & (1 << 1) != 0
    }

    #[inline]
    pub fn to_bytes(self) -> [u8; 1] {
        [self.0]
    }
}

impl From<&[u8]> for StreamType {
    #[inline]
    fn from(value: &[u8]) -> Self {
        Self(value[0])
    }
}

impl From<[u8; 1]> for StreamType {
    #[inline]
    fn from(value: [u8; 1]) -> Self {
        Self(value[0])
    }
}

#[derive(Clone, Copy)]
pub struct StreamFlags(u8);

impl StreamFlags {
    pub fn new(stream_type: StreamType, opus_enabled: bool) -> Self {
        if opus_enabled {
            Self(stream_type.0 | (1 << 2))
        } else {
            Self(stream_type.0)
        }
    }

    #[inline]
    pub fn stream_type(self) -> StreamType {
        StreamType(self.0)
    }

    #[inline]
    pub fn opus_enabled(self) -> bool {
        self.0 & (1 << 2) != 0
    }

    #[inline]
    pub fn to_bytes(self) -> [u8; 1] {
        [self.0]
    }
}

impl Default for StreamFlags {
    #[inline]
    fn default() -> Self {
        Self(Default::default())
    }
}

impl Into<[u8; 1]> for StreamFlags {
    #[inline]
    fn into(self) -> [u8; 1] {
        self.to_bytes()
    }
}

impl From<&[u8]> for StreamFlags {
    #[inline]
    fn from(value: &[u8]) -> Self {
        Self(value[0])
    }
}

impl From<[u8; 1]> for StreamFlags {
    #[inline]
    fn from(value: [u8; 1]) -> Self {
        Self(value[0])
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

pub trait Stream: StreamInternal + Sized {
    fn start(name: &str, device: Option<&str>, config: Config) -> Result<Self>;

    #[inline]
    fn capacity(&self) -> usize {
        unsafe { aw_buffer_capacity(self.base().handle) }
    }

    #[inline]
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
    #[inline]
    pub fn read(&mut self, buf: &mut [u8]) -> usize {
        unsafe { aw_record_read(self.base.handle, buf.as_mut_ptr() as *mut c_char, buf.len()) }
    }
}

impl StreamInternal for RecordStream {
    #[inline]
    fn base(&self) -> &BaseStream {
        &self.base
    }

    #[inline]
    fn base_mut(&mut self) -> &mut BaseStream {
        &mut self.base
    }
}

impl Stream for RecordStream {
    #[inline]
    fn start(name: &str, device: Option<&str>, config: Config) -> Result<Self> {
        StreamBuilder::new(config).start_record(name, device)
    }

    #[inline]
    fn peek(&self) -> usize {
        unsafe { aw_record_peek(self.base.handle) }
    }
}

unsafe impl Sync for RecordStream {}
unsafe impl Send for RecordStream {}

pub struct PlaybackStream {
    base: BaseStream,
}

impl PlaybackStream {
    #[inline]
    pub fn write(&mut self, buf: &[u8]) -> usize {
        unsafe { aw_playback_write(self.base.handle, buf.as_ptr() as *mut c_char, buf.len()) }
    }
}

impl StreamInternal for PlaybackStream {
    #[inline]
    fn base(&self) -> &BaseStream {
        &self.base
    }

    #[inline]
    fn base_mut(&mut self) -> &mut BaseStream {
        &mut self.base
    }
}

impl Stream for PlaybackStream {
    #[inline]
    fn start(name: &str, device: Option<&str>, config: Config) -> Result<Self> {
        StreamBuilder::new(config).start_playback(name, device)
    }

    #[inline]
    fn peek(&self) -> usize {
        unsafe { aw_playback_peek(self.base.handle) }
    }
}

unsafe impl Sync for PlaybackStream {}
unsafe impl Send for PlaybackStream {}

pub type ErrorCallback = fn(err: i32, message: &str, userdata: *mut c_void);

struct ErrorHandle {
    error_cb: ErrorCallback,
    userdata: *mut c_void,
}

unsafe extern "C" fn on_error(err: c_int, message: *const c_char, userdata: *mut c_void) {
    let handle = &ptr::read(userdata as *mut ErrorHandle);
    (handle.error_cb)(
        err as i32,
        CStr::from_ptr(message).to_str().unwrap_or_default(),
        handle.userdata,
    );
}

type StartStreamFn = unsafe extern "C" fn(
    stream: *mut *mut aw_stream,
    devname: *const c_char,
    name: *const c_char,
    cfg: aw_config,
    error_cb: aw_error_callback_t,
    userdata: *mut c_void,
) -> aw_result;

unsafe fn start_stream(
    start_fn: StartStreamFn,
    device: Option<&str>,
    name: &str,
    config: Config,
    error_cb: Option<ErrorCallback>,
    userdata: *mut c_void,
) -> Result<*mut aw_stream> {
    let mut stream: *mut aw_stream = ptr::null_mut();
    let cdev = device.map(|s| CString::new(s).unwrap());
    let dev_ptr = cdev.as_ref().map(|s| s.as_ptr()).unwrap_or(ptr::null());
    let cname = CString::new(name).unwrap();
    let result = if let Some(error_cb) = error_cb {
        let handle = Box::into_raw(Box::new(ErrorHandle { error_cb, userdata }));
        start_fn(
            &mut stream,
            dev_ptr,
            cname.as_ptr(),
            config.into(),
            Some(on_error),
            handle as *mut c_void,
        )
    } else {
        start_fn(
            &mut stream,
            dev_ptr,
            cname.as_ptr(),
            config.into(),
            None,
            ptr::null_mut(),
        )
    };
    parse_result_value(result, stream)
}

pub struct StreamBuilder {
    config: Config,
    error_cb: Option<ErrorCallback>,
    userdata: *mut c_void,
}

impl StreamBuilder {
    #[inline]
    pub fn new(config: Config) -> Self {
        Self {
            config,
            error_cb: None,
            userdata: ptr::null_mut(),
        }
    }

    #[inline]
    pub fn error_cb<T>(&mut self, error_cb: ErrorCallback, userdata: Option<T>) -> &Self {
        self.error_cb = Some(error_cb);
        self.userdata = userdata
            .map(|v| Box::into_raw(Box::new(v)) as *mut c_void)
            .unwrap_or_else(|| ptr::null_mut());
        self
    }

    #[inline]
    pub fn start_record(&self, name: &str, device: Option<&str>) -> Result<RecordStream> {
        self.start_stream(aw_start_record, name, device)
            .map(|base| RecordStream { base })
    }

    #[inline]
    pub fn start_playback(&self, name: &str, device: Option<&str>) -> Result<PlaybackStream> {
        self.start_stream(aw_start_playback, name, device)
            .map(|base| PlaybackStream { base })
    }

    #[inline]
    fn start_stream(
        &self,
        start_fn: StartStreamFn,
        name: &str,
        device: Option<&str>,
    ) -> Result<BaseStream> {
        let result = unsafe {
            start_stream(
                start_fn,
                device,
                name,
                self.config,
                self.error_cb,
                self.userdata,
            )
        };
        result.map(|s| BaseStream::new(s))
    }
}
