#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod tests {
    use std::{ffi::c_char, os::raw::c_void, ptr, slice, sync::mpsc};

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

            assert_eq!(aw_initialize(), 0);
            assert_eq!(
                aw_start_record(
                    &mut record,
                    ptr::null(),
                    Some(read_callback),
                    userdata_ptr as *mut c_void
                ),
                0
            );
            assert_eq!(
                aw_start_playback(
                    &mut playback,
                    ptr::null(),
                    Some(write_callback),
                    userdata_ptr as *mut c_void
                ),
                0
            );

            let first = rx.recv().unwrap();
            let second = rx.recv().unwrap();
            assert_eq!(first, second);

            assert_eq!(aw_stop(playback), 0);
            assert_eq!(aw_stop(record), 0);
            assert_eq!(aw_terminate(), 0);
        }
    }
}
