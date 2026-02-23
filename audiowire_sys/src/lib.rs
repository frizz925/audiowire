#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod tests {
    use std::{
        ffi::{CStr, CString, c_char, c_int, c_void},
        ptr,
        sync::atomic::{AtomicUsize, Ordering},
        thread::sleep,
        time::Duration,
    };

    use crate::*;

    fn assert_aw_result(res: aw_result) {
        if res.code != 0 {
            let message = unsafe { CStr::from_ptr(res.message).to_string_lossy() };
            panic!("Result is error: code={}, message={}", res.code, message)
        }
    }

    unsafe extern "C" fn on_read(_: *const c_char, len: usize, userdata: *mut c_void) {
        if let Some(total_bytes) = unsafe { (userdata as *const AtomicUsize).as_ref() } {
            total_bytes.fetch_add(len, Ordering::Relaxed);
        }
    }

    unsafe extern "C" fn on_write(_: *mut c_char, len: usize, userdata: *mut c_void) {
        if let Some(total_bytes) = unsafe { (userdata as *const AtomicUsize).as_ref() } {
            total_bytes.fetch_add(len, Ordering::Relaxed);
        }
    }

    unsafe extern "C" fn on_error(err: c_int, message: *const c_char, _: *mut c_void) {
        panic!("Error {}: {}", err, unsafe {
            CStr::from_ptr(message).to_string_lossy()
        });
    }

    #[test]
    fn start_stop_stream() {
        unsafe {
            let mut record: *mut aw_stream = ptr::null_mut();
            let mut playback: *mut aw_stream = ptr::null_mut();
            let config = aw_config {
                channels: 2,
                sample_rate: 48000,
                sample_format: aw_sample_format_AW_SAMPLE_FORMAT_S16,
                buffer_frames: 960,
                max_buffer_frames: 1920,
            };

            let record_name = CString::new("record-test").unwrap();
            let playback_name = CString::new("playback-test").unwrap();

            let total_read = Box::into_raw(Box::new(AtomicUsize::new(0)));
            let total_write = Box::into_raw(Box::new(AtomicUsize::new(0)));

            assert_aw_result(aw_initialize());
            assert_aw_result(aw_start(
                &mut record,
                ptr::null(),
                record_name.as_ptr(),
                config,
                Some(on_read),
                None,
                Some(on_error),
                total_read as *mut c_void,
            ));
            assert_aw_result(aw_start(
                &mut playback,
                ptr::null(),
                playback_name.as_ptr(),
                config,
                None,
                Some(on_write),
                Some(on_error),
                total_write as *mut c_void,
            ));

            assert!(!aw_device_name(record).is_null());
            assert!(aw_sample_rate(record) > 0);

            assert!(!aw_device_name(playback).is_null());
            assert!(aw_sample_rate(playback) > 0);

            loop {
                sleep(Duration::from_millis(20));
                let read = total_read
                    .as_ref()
                    .map(|v| v.load(Ordering::Relaxed))
                    .unwrap_or_default();
                let write = total_write
                    .as_ref()
                    .map(|v| v.load(Ordering::Relaxed))
                    .unwrap_or_default();
                if read > 0 && write > 0 {
                    break;
                }
            }

            assert_aw_result(aw_stop(playback));
            assert_aw_result(aw_stop(record));

            ptr::drop_in_place(total_read);
            ptr::drop_in_place(total_write);

            assert_aw_result(aw_terminate());
        }
    }
}
