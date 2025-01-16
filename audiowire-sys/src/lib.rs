#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod tests {
    use std::{
        ffi::{c_char, c_int, c_void, CStr, CString},
        ptr,
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

    unsafe extern "C" fn on_error(err: c_int, message: *const c_char, _: *mut c_void) {
        panic!(
            "Error {}: {}",
            err,
            CStr::from_ptr(message).to_string_lossy()
        );
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

            assert_aw_result(aw_initialize());
            assert_aw_result(aw_start_record(
                &mut record,
                ptr::null(),
                record_name.as_ptr(),
                config,
                Some(on_error),
                ptr::null_mut(),
            ));
            assert_aw_result(aw_start_playback(
                &mut playback,
                ptr::null(),
                playback_name.as_ptr(),
                config,
                Some(on_error),
                ptr::null_mut(),
            ));

            assert!(!aw_device_name(record).is_null());
            assert!(!aw_device_name(playback).is_null());

            let mut buf_arr = [0u8; 65536];
            let bufsize = buf_arr.len();
            let buf = buf_arr.as_mut_ptr() as *mut c_char;

            loop {
                sleep(Duration::from_millis(20));
                let read = aw_record_read(record, buf, bufsize);
                if read > 0 {
                    assert_eq!(aw_playback_write(playback, buf, read), read);
                    break;
                }
            }

            assert_aw_result(aw_stop(playback));
            assert_aw_result(aw_stop(record));
            assert_aw_result(aw_terminate());
        }
    }
}
