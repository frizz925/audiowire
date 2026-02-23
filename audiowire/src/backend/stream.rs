use std::{
    ffi::{CStr, CString, c_char, c_int},
    os::raw::c_void,
    ptr, slice,
};

use audiowire_sys::*;

use super::{
    config::Config,
    result::{Result, parse_result_lazy},
};

pub trait ReadFn: FnMut(&[u8]) + 'static {}
pub trait WriteFn: FnMut(&mut [u8]) + 'static {}
pub trait ErrorFn: FnMut(i32, &str) + 'static {}

impl<F: FnMut(&[u8]) + 'static> ReadFn for F {}
impl<F: FnMut(&mut [u8]) + 'static> WriteFn for F {}
impl<F: FnMut(i32, &str) + 'static> ErrorFn for F {}

type ReadCallback = Box<dyn ReadFn>;
type WriteCallback = Box<dyn WriteFn>;
type ErrorCallback = Box<dyn ErrorFn>;

pub struct Callbacks {
    read_cb: Option<ReadCallback>,
    write_cb: Option<WriteCallback>,
    error_cb: Option<ErrorCallback>,
}

macro_rules! callback {
    ($ptr:ident, $name:ident $($e:tt)*) => {
        if let Some(callbacks) = unsafe { ($ptr as *mut Callbacks).as_mut() } {
            if let Some($name) = callbacks.$name.as_mut() {
                $name$($e)*
            }
        }
    };
}

unsafe extern "C" fn on_read(buf: *const c_char, len: usize, userdata: *mut c_void) {
    let slice = unsafe { slice::from_raw_parts(buf as *const u8, len) };
    callback!(userdata, read_cb(slice));
}

unsafe extern "C" fn on_write(buf: *mut c_char, len: usize, userdata: *mut c_void) {
    let slice = unsafe { slice::from_raw_parts_mut(buf as *mut u8, len) };
    callback!(userdata, write_cb(slice));
}

unsafe extern "C" fn on_error(err: c_int, message: *const c_char, userdata: *mut c_void) {
    callback!(
        userdata,
        error_cb(
            err as i32,
            unsafe { CStr::from_ptr(message) }
                .to_str()
                .unwrap_or_default(),
        )
    );
}

pub struct Stream {
    handle: *mut aw_stream,
    devname: Option<String>,
    userdata: *mut Callbacks,
}

impl Stream {
    fn new(handle: *mut aw_stream, userdata: *mut Callbacks) -> Self {
        let devname = unsafe {
            let cstr = aw_device_name(handle);
            if !cstr.is_null() {
                CStr::from_ptr(cstr).to_str().map(str::to_string).ok()
            } else {
                None
            }
        };
        Self {
            handle,
            devname,
            userdata,
        }
    }

    pub fn start<N, D>(
        name: N,
        device: Option<D>,
        config: Config,
        callbacks: Callbacks,
    ) -> Result<Self>
    where
        N: Into<Vec<u8>>,
        D: Into<Vec<u8>>,
    {
        let mut stream: *mut aw_stream = ptr::null_mut();
        let cdev = device
            .map(|s| CString::new(s).unwrap().into_raw())
            .unwrap_or(ptr::null_mut());

        let read_cb = callbacks.read_cb.is_some();
        let write_cb = callbacks.write_cb.is_some();
        let error_cb = callbacks.error_cb.is_some();

        let cname = CString::new(name).unwrap().into_raw();
        let userdata = Box::into_raw(Box::new(callbacks));
        let result = unsafe {
            aw_start(
                &mut stream,
                cdev,
                cname,
                config.into(),
                if read_cb { Some(on_read) } else { None },
                if write_cb { Some(on_write) } else { None },
                if error_cb { Some(on_error) } else { None },
                userdata as *mut c_void,
            )
        };
        parse_result_lazy(result, || Stream::new(stream, userdata))
    }

    #[inline]
    pub fn device_name(&self) -> Option<&str> {
        self.devname.as_deref()
    }

    #[inline]
    pub fn sample_rate(&self) -> u32 {
        unsafe { aw_sample_rate(self.handle) }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        unsafe {
            aw_stop(self.handle);
            ptr::drop_in_place(self.userdata);
        }
    }
}

unsafe impl Sync for Stream {}
unsafe impl Send for Stream {}

pub struct StreamBuilder {
    config: Config,
    callbacks: Callbacks,
}

impl StreamBuilder {
    #[inline]
    pub fn new(config: Config) -> Self {
        Self {
            config,
            callbacks: Callbacks {
                read_cb: None,
                write_cb: None,
                error_cb: None,
            },
        }
    }

    #[inline]
    pub fn read_cb(mut self, read_cb: impl ReadFn + 'static) -> Self {
        self.callbacks.read_cb = Some(Box::new(read_cb));
        self
    }

    #[inline]
    pub fn write_cb(mut self, write_cb: impl WriteFn + 'static) -> Self {
        self.callbacks.write_cb = Some(Box::new(write_cb));
        self
    }

    #[inline]
    pub fn error_cb(mut self, error_cb: impl ErrorFn + 'static) -> Self {
        self.callbacks.error_cb = Some(Box::new(error_cb));
        self
    }

    #[inline]
    pub fn start<N, D>(self, name: N, device: Option<D>) -> Result<Stream>
    where
        N: Into<Vec<u8>>,
        D: Into<Vec<u8>>,
    {
        Stream::start(name, device, self.config, self.callbacks)
    }
}

impl Default for StreamBuilder {
    fn default() -> Self {
        Self::new(Config::default())
    }
}
