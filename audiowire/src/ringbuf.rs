use std::{
    cell::UnsafeCell,
    sync::atomic::{AtomicUsize, Ordering},
};

#[derive(Debug)]
pub struct RingBuf {
    buf: UnsafeCell<Vec<u8>>,
    ridx: AtomicUsize,
    widx: AtomicUsize,
    mask: usize,
    capacity: usize,
}

impl RingBuf {
    const MAX_CAPACITY: usize = (usize::MAX >> 1) + 1;

    pub fn new(requested: usize) -> Self {
        let mut capacity = 2;
        while capacity <= requested && capacity < Self::MAX_CAPACITY {
            capacity <<= 1;
        }

        Self {
            buf: UnsafeCell::new(vec![0u8; capacity]),
            ridx: AtomicUsize::new(0),
            widx: AtomicUsize::new(0),
            mask: capacity - 1,
            capacity,
        }
    }

    /// How many bytes of data is remaining to read.
    #[inline]
    pub fn remaining(&self) -> usize {
        let (ridx, widx) = (
            self.ridx.load(Ordering::Relaxed),
            self.widx.load(Ordering::Relaxed),
        );
        if widx >= ridx {
            widx - ridx
        } else {
            self.capacity - ridx + widx
        }
    }

    /// How many bytes of data is available to write.
    #[inline]
    pub fn available(&self) -> usize {
        let (ridx, widx) = (
            self.ridx.load(Ordering::Relaxed),
            self.widx.load(Ordering::Relaxed),
        );
        if ridx > widx {
            ridx - widx - 1
        } else {
            self.mask - widx + ridx
        }
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn read_chunk<'a>(&'a self) -> &'a [u8] {
        let (ridx, widx) = (
            self.ridx.load(Ordering::Relaxed),
            self.widx.load(Ordering::Relaxed),
        );
        let buf = unsafe { &*self.buf.get() };
        if ridx <= widx {
            &buf[ridx..widx]
        } else {
            &buf[ridx..]
        }
    }

    pub fn advance_read(&self, off: usize) {
        self.ridx
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |x| {
                Some((x + off) & self.mask)
            })
            .unwrap();
    }

    pub fn write_chunk<'a>(&'a self) -> &'a mut [u8] {
        let (ridx, widx) = (
            self.ridx.load(Ordering::Relaxed),
            self.widx.load(Ordering::Relaxed),
        );
        let buf = unsafe { &mut *self.buf.get() };
        if widx < ridx {
            &mut buf[widx..ridx - 1]
        } else if ridx <= 0 {
            &mut buf[widx..self.mask]
        } else {
            &mut buf[widx..]
        }
    }

    pub fn advance_write(&self, off: usize) {
        self.widx
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |x| {
                Some((x + off) & self.mask)
            })
            .unwrap();
    }

    /*
    pub fn make_contiguous(&self) -> &mut Self {
        let (ridx, widx) = (self.ridx, self.widx);
        if ridx == widx {
            // Just move the cursors to the beginning of the buffer if they match
            self.ridx = 0;
            self.widx = 0;
            return self;
        } else if ridx < widx {
            // Move written bytes to the beginning of the buffer
            self.buf.copy_within(ridx..widx, 0);
            self.ridx -= ridx;
            self.widx -= ridx;
            return self;
        }

        let length = self.buf.len();
        let offset = length - ridx;
        self.reverse(0, length);
        self.reverse(0, offset);
        self.reverse(offset, length);

        self.ridx = (self.ridx + offset) & self.mask;
        self.widx = (self.widx + offset) & self.mask;
        self
    }

    fn reverse(&mut self, start: usize, end: usize) {
        self.buf[start..end].reverse();
    }
    */
}

unsafe impl Sync for RingBuf {}
unsafe impl Send for RingBuf {}

#[cfg(test)]
mod test {
    use crate::ringbuf::RingBuf;

    #[test]
    fn test_ringbuf() {
        let req = 14;
        let rb = RingBuf::new(req);

        assert!(rb.capacity() > req);
        assert_eq!(rb.remaining(), 0);
        assert_eq!(rb.available(), rb.capacity() - 1);

        let chunk = rb.read_chunk();
        assert_eq!(chunk.len(), rb.remaining());

        let chunk = rb.write_chunk();
        assert_eq!(chunk.len(), rb.available());

        let sample = b"Hello world!";
        let length = sample.len();
        for _ in 0..10 {
            // Test write
            let chunk = rb.write_chunk();
            let write = usize::min(chunk.len(), length);
            chunk[..write].copy_from_slice(&sample[..write]);

            rb.advance_write(write);
            assert_eq!(rb.remaining(), write);
            assert_eq!(rb.available(), rb.capacity() - write - 1);

            // Test read
            let chunk = rb.read_chunk();
            let read = usize::min(chunk.len(), length);
            assert_eq!(read, write);
            assert_eq!(chunk, &sample[..read]);

            rb.advance_read(chunk.len());
            assert_eq!(rb.remaining(), 0);
            assert_eq!(rb.available(), rb.capacity() - 1);
        }
    }
}
