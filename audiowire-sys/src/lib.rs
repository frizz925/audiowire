#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod tests {
    use std::{
        ffi::{c_char, CStr},
        os::raw::c_void,
        ptr, slice,
        sync::mpsc,
    };

    use crate::*;

    struct UserData {
        ringbuf: *mut ringbuf_t,
        channel: mpsc::SyncSender<Vec<c_char>>,
    }

    unsafe extern "C" fn read_callback(
        buf: *const c_char,
        bufsize: usize,
        userdata: *mut c_void,
    ) -> i32 {
        let ptr = userdata as *mut UserData;
        let rb = (*ptr).ringbuf;
        let chan = (*ptr).channel.clone();
        let result: u32 = if ringbuf_available(rb) >= bufsize {
            let write = ringbuf_write(rb, buf, bufsize);
            let vecbuf = slice::from_raw_parts(buf, write).to_vec();
            chan.send(vecbuf).unwrap();
            aw_stream_callback_result_AW_STREAM_STOP
        } else {
            println!("Buffer overflow!");
            aw_stream_callback_result_AW_STREAM_CONTINUE
        };
        result.try_into().unwrap()
    }

    unsafe extern "C" fn write_callback(
        buf: *mut c_char,
        bufsize: usize,
        userdata: *mut c_void,
    ) -> i32 {
        let ptr = userdata as *mut UserData;
        let rb = (*ptr).ringbuf;
        let chan = (*ptr).channel.clone();
        let result: u32 = if ringbuf_remaining(rb) >= bufsize {
            let read = ringbuf_read(rb, buf, bufsize);
            let vecbuf = slice::from_raw_parts(buf, read).to_vec();
            chan.send(vecbuf).unwrap();
            aw_stream_callback_result_AW_STREAM_STOP
        } else {
            println!("Buffer underflow!");
            buf.write_bytes(0, bufsize);
            aw_stream_callback_result_AW_STREAM_CONTINUE
        };
        result.try_into().unwrap()
    }

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

            let (tx, rx) = mpsc::sync_channel(2);
            let userdata = UserData {
                ringbuf: ringbuf_create(65536),
                channel: tx,
            };
            let userdata_ptr = Box::into_raw(Box::new(userdata));

            assert_aw_result(aw_initialize());
            assert_aw_result(aw_start_record(
                &mut record,
                ptr::null(),
                Some(read_callback),
                userdata_ptr as *mut c_void,
            ));
            assert_aw_result(aw_start_playback(
                &mut playback,
                ptr::null(),
                Some(write_callback),
                userdata_ptr as *mut c_void,
            ));

            let first = rx.recv().unwrap();
            let second = rx.recv().unwrap();
            assert_eq!(first, second);

            assert_aw_result(aw_stop(playback));
            assert_aw_result(aw_stop(record));
            assert_aw_result(aw_terminate());
        }
    }
}
