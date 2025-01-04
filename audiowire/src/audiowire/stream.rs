use std::{
    ffi::{c_char, c_int, c_void, CStr, CString},
    ptr, slice,
};

use audiowire_sys::*;
use slog::{error, o, Logger};

use super::result::{parse_result, parse_result_lazy, Result};

pub enum CallbackResult {
    Continue,
    Stop,
    Abort,
}

impl CallbackResult {
    fn to_cint(&self) -> c_int {
        (match self {
            Self::Continue => aw_stream_callback_result_AW_STREAM_CONTINUE,
            Self::Stop => aw_stream_callback_result_AW_STREAM_STOP,
            Self::Abort => aw_stream_callback_result_AW_STREAM_ABORT,
        }) as c_int
    }
}

pub type ReadCallback = fn(&[u8], *mut c_void) -> CallbackResult;
pub type WriteCallback = fn(&mut [u8], *mut c_void) -> CallbackResult;

struct Context<F, T> {
    callback: F,
    userdata: *mut T,
}

impl<F, T> Context<F, T> {
    fn new(callback: F, userdata: T) -> Self {
        Self {
            callback,
            userdata: Box::into_raw(Box::new(userdata)),
        }
    }

    fn new_ptr(callback: F, userdata: T) -> *mut Self {
        Box::into_raw(Box::new(Self::new(callback, userdata)))
    }
}

impl<F, T> Drop for Context<F, T> {
    fn drop(&mut self) {
        unsafe {
            if !self.userdata.is_null() {
                ptr::drop_in_place(self.userdata);
            }
        }
    }
}

pub trait Stream: Sync + Send {
    fn device_name(&self) -> Option<&str>;
    fn stop(&mut self) -> Result<()>;
}

enum StreamType {
    Record,
    Playback,
}

struct StreamImpl<F, T> {
    logger: Logger,
    handle: *mut aw_stream,
    userdata: *mut Context<F, T>,
    devname: Option<String>,
    running: bool,
}

impl<F, T> StreamImpl<F, T> {
    fn new(
        stype: StreamType,
        logger: &Logger,
        handle: *mut aw_stream,
        context: *mut Context<F, T>,
    ) -> Self {
        let stream_type = match stype {
            StreamType::Record => "record",
            StreamType::Playback => "playback",
        };
        let devname = unsafe {
            let cstr = aw_device_name(handle);
            if !cstr.is_null() {
                Some(CStr::from_ptr(cstr).to_string_lossy().to_string())
            } else {
                None
            }
        };
        Self {
            logger: logger.new(o!("stream" => stream_type)),
            handle,
            userdata: context,
            devname,
            running: true,
        }
    }
}

impl<F, T> Stream for StreamImpl<F, T> {
    fn device_name(&self) -> Option<&str> {
        self.devname.as_ref().map(|s| s.as_str())
    }

    // Stop is idempotent
    fn stop(&mut self) -> Result<()> {
        if self.running {
            unsafe {
                parse_result(aw_stop(self.handle)).map(|v| {
                    self.running = false;
                    v
                })
            }
        } else {
            Ok(())
        }
    }
}

impl<F, T> Drop for StreamImpl<F, T> {
    fn drop(&mut self) {
        match self.stop() {
            Ok(_) => unsafe { ptr::drop_in_place(self.userdata) },
            Err(err) => error!(self.logger, "Failed to stop stream: {}", err),
        }
    }
}

unsafe impl<F, T> Sync for StreamImpl<F, T> {}
unsafe impl<F, T> Send for StreamImpl<F, T> {}

unsafe extern "C" fn read_callback(
    buf: *const c_char,
    bufsize: usize,
    userdata: *mut c_void,
) -> c_int {
    let context = userdata as *mut Context<ReadCallback, c_void>;
    let bufslice = slice::from_raw_parts(buf as *const u8, bufsize);
    ((*context).callback)(bufslice, (*context).userdata).to_cint()
}

unsafe extern "C" fn write_callback(
    buf: *mut c_char,
    bufsize: usize,
    userdata: *mut c_void,
) -> c_int {
    let context = userdata as *mut Context<WriteCallback, c_void>;
    let bufslice = slice::from_raw_parts_mut(buf as *mut u8, bufsize);
    ((*context).callback)(bufslice, (*context).userdata).to_cint()
}

pub fn start_record<T>(
    logger: &Logger,
    name: Option<&str>,
    callback: ReadCallback,
    userdata: T,
) -> Result<impl Stream> {
    let context = Context::new_ptr(callback, userdata);
    let mut handle: *mut aw_stream = ptr::null_mut();
    let result = unsafe {
        let cname = name.map(|s| CString::new(s).unwrap());
        let name_ptr = cname.map(|s| s.as_ptr()).unwrap_or(ptr::null());
        aw_start_record(
            &mut handle,
            name_ptr,
            Some(read_callback),
            context as *mut c_void,
        )
    };
    parse_result_lazy(result, || {
        StreamImpl::new(StreamType::Record, logger, handle, context)
    })
}

pub fn start_playback<T>(
    logger: &Logger,
    name: Option<&str>,
    callback: WriteCallback,
    userdata: T,
) -> Result<impl Stream> {
    let context = Context::new_ptr(callback, userdata);
    let mut handle: *mut aw_stream = ptr::null_mut();
    let result = unsafe {
        let cname = name.map(|s| CString::new(s).unwrap());
        let name_ptr = cname.map(|s| s.as_ptr()).unwrap_or(ptr::null());
        aw_start_playback(
            &mut handle,
            name_ptr,
            Some(write_callback),
            context as *mut c_void,
        )
    };
    parse_result_lazy(result, || {
        StreamImpl::new(StreamType::Playback, logger, handle, context)
    })
}
