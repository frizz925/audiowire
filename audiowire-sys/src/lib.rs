#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod tests {
    use std::{ffi::c_char, os::raw::c_void, ptr, thread, time::Duration};

    use crate::*;

    unsafe extern "C" fn read_callback(
        buf: *const c_char,
        bufsize: usize,
        userdata: *mut c_void,
    ) -> i32 {
        let rb = userdata as *mut ringbuf_t;
        if ringbuf_available(rb) >= bufsize {
            ringbuf_write(rb, buf, bufsize);
        } else {
            println!("Buffer overflow!");
        }
        aw_stream_callback_result_AW_STREAM_CONTINUE
            .try_into()
            .unwrap()
    }

    unsafe extern "C" fn write_callback(
        buf: *mut c_char,
        bufsize: usize,
        userdata: *mut c_void,
    ) -> i32 {
        let rb = userdata as *mut ringbuf_t;
        if ringbuf_remaining(rb) >= bufsize {
            ringbuf_read(rb, buf, bufsize);
        } else {
            println!("Buffer underflow!");
        }
        aw_stream_callback_result_AW_STREAM_CONTINUE
            .try_into()
            .unwrap()
    }

    #[test]
    fn start_stop_stream() {
        unsafe {
            let mut record: *mut aw_stream = ptr::null_mut();
            let mut playback: *mut aw_stream = ptr::null_mut();
            let rb: *mut ringbuf = ringbuf_create(65536);

            assert_eq!(aw_init(), 0);
            assert_eq!(
                aw_start_record(&mut record, Some(read_callback), rb as *mut c_void),
                0
            );
            assert_eq!(
                aw_start_playback(&mut playback, Some(write_callback), rb as *mut c_void),
                0
            );

            thread::sleep(Duration::from_secs(5));

            assert_eq!(aw_stop(playback), 0);
            assert_eq!(aw_stop(record), 0);
            assert_eq!(aw_terminate(), 0);
        }
    }
}
