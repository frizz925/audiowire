#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod tests {
    use std::{
        ffi::{c_char, CStr},
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

            assert_aw_result(aw_initialize());
            assert_aw_result(aw_start_record(&mut record, ptr::null(), config));
            assert_aw_result(aw_start_playback(&mut playback, ptr::null(), config));

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
