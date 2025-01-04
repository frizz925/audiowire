#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod tests {
    use std::{
        borrow::Borrow,
        ffi::{c_char, CStr},
        os::raw::c_void,
        ptr, slice,
        sync::mpsc,
    };

    use crate::*;

    struct Reader {
        sender: mpsc::SyncSender<Vec<c_char>>,
        notify: mpsc::SyncSender<Vec<c_char>>,
    }

    struct Writer {
        receiver: mpsc::Receiver<Vec<c_char>>,
        notify: mpsc::SyncSender<Vec<c_char>>,
    }

    unsafe extern "C" fn read_callback(
        buf: *const c_char,
        bufsize: usize,
        userdata: *mut c_void,
    ) -> i32 {
        let uptr = userdata as *mut Reader;
        let chan = (*uptr).sender.clone();
        let notify = (*uptr).notify.clone();
        let vecbuf = slice::from_raw_parts(buf, bufsize).to_vec();
        chan.send(vecbuf.clone()).unwrap();
        notify.send(vecbuf).unwrap();
        aw_stream_callback_result_AW_STREAM_STOP.try_into().unwrap()
    }

    unsafe extern "C" fn write_callback(
        buf: *mut c_char,
        bufsize: usize,
        userdata: *mut c_void,
    ) -> i32 {
        let uptr = userdata as *mut Writer;
        let chan = (*uptr).receiver.borrow();
        let notify = (*uptr).notify.clone();
        let vecbuf = chan.recv().unwrap();
        if vecbuf.len() > bufsize {
            ptr::copy_nonoverlapping(vecbuf.as_ptr(), buf, bufsize);
        } else {
            let len = vecbuf.len();
            ptr::copy_nonoverlapping(vecbuf.as_ptr(), buf, len);
            buf.offset(len as isize).write_bytes(0, bufsize - len);
        }
        notify.send(vecbuf).unwrap();
        aw_stream_callback_result_AW_STREAM_STOP.try_into().unwrap()
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

            let (tx, rx) = mpsc::sync_channel(1);
            let (notify_tx, notify_rx) = mpsc::sync_channel(2);

            let reader = Reader {
                sender: tx,
                notify: notify_tx.clone(),
            };
            let writer = Writer {
                receiver: rx,
                notify: notify_tx.clone(),
            };

            let reader_ptr = Box::into_raw(Box::new(reader));
            let writer_ptr = Box::into_raw(Box::new(writer));

            assert_aw_result(aw_initialize());
            assert_aw_result(aw_start_record(
                &mut record,
                ptr::null(),
                Some(read_callback),
                reader_ptr as *mut c_void,
            ));
            assert_aw_result(aw_start_playback(
                &mut playback,
                ptr::null(),
                Some(write_callback),
                writer_ptr as *mut c_void,
            ));

            assert!(!aw_device_name(record).is_null());
            assert!(!aw_device_name(playback).is_null());

            let first = notify_rx.recv().unwrap();
            let second = notify_rx.recv().unwrap();
            assert_eq!(first, second);

            assert_aw_result(aw_stop(playback));
            assert_aw_result(aw_stop(record));
            assert_aw_result(aw_terminate());

            ptr::drop_in_place(writer_ptr);
            ptr::drop_in_place(reader_ptr);
        }
    }
}
