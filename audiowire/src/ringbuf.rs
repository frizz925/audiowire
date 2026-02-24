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

    pub fn read_chunks<'a>(&'a self) -> Chunks<'a, u8> {
        let (ridx, widx) = (
            self.ridx.load(Ordering::Relaxed),
            self.widx.load(Ordering::Relaxed),
        );
        let buf = unsafe { &*self.buf.get() };
        if ridx <= widx {
            Chunks::new(self, &buf[ridx..widx], &[])
        } else {
            Chunks::new(self, &buf[ridx..], &buf[..widx])
        }
    }

    pub fn advance_read(&self, off: usize) {
        self.ridx
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |x| {
                Some((x + off) & self.mask)
            })
            .unwrap();
    }

    pub fn write_chunks<'a>(&'a self) -> MutChunks<'a, u8> {
        let (ridx, widx) = (
            self.ridx.load(Ordering::Relaxed),
            self.widx.load(Ordering::Relaxed),
        );
        let buf = unsafe { &mut *self.buf.get() };
        if widx < ridx {
            MutChunks::new(self, &mut buf[widx..ridx - 1], &mut [])
        } else if ridx <= 0 {
            MutChunks::new(self, &mut buf[widx..self.mask], &mut [])
        } else {
            let (tail, head) = buf.split_at_mut(widx);
            MutChunks::new(self, head, &mut tail[..ridx - 1])
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

pub struct Chunks<'a, T> {
    rb: &'a RingBuf,
    head: &'a [T],
    tail: &'a [T],
    pos: usize,
    len: usize,
}

impl<'a, T> Chunks<'a, T> {
    pub fn new(rb: &'a RingBuf, head: &'a [T], tail: &'a [T]) -> Self {
        Self {
            rb,
            head,
            tail,
            pos: 0,
            len: head.len() + tail.len(),
        }
    }

    #[inline]
    pub fn remaining(&self) -> usize {
        self.len - self.pos
    }

    #[inline]
    pub fn free(self) {
        self.rb.advance_read(self.pos);
    }
}

impl<'a, T: Copy> Chunks<'a, T> {
    pub fn read(&mut self, dst: &mut [T]) -> usize {
        if self.remaining() <= 0 {
            return 0;
        }
        // Read from head if the cursor hasn't advanced past the head
        let (mut read, pos) = if self.pos < self.head.len() {
            (safe_copy_from_slice(&self.head[self.pos..], dst), 0)
        } else {
            (0, self.pos - self.head.len())
        };
        // Read from tail if there's still buffer to fill
        if read < dst.len() {
            read += safe_copy_from_slice(&self.tail[pos..], &mut dst[read..]);
        }
        self.pos += read;
        read
    }

    #[inline]
    pub fn consume(self) -> Vec<T> {
        let mut buf = Vec::with_capacity(self.remaining());
        let pos = if self.pos < self.head.len() {
            buf.extend_from_slice(&self.head[self.pos..]);
            0
        } else {
            self.pos - self.head.len()
        };
        buf.extend_from_slice(&self.tail[pos..]);
        self.rb.advance_read(buf.len());
        buf
    }
}

pub struct MutChunks<'a, T> {
    rb: &'a RingBuf,
    head: &'a mut [T],
    tail: &'a mut [T],
    pos: usize,
    len: usize,
}

impl<'a, T> MutChunks<'a, T> {
    pub fn new(rb: &'a RingBuf, head: &'a mut [T], tail: &'a mut [T]) -> Self {
        let len = head.len() + tail.len();
        Self {
            rb,
            head,
            tail,
            pos: 0,
            len,
        }
    }

    #[inline]
    pub fn available(&self) -> usize {
        self.len - self.pos
    }

    #[inline]
    pub fn flush(self) {
        self.rb.advance_write(self.pos);
    }
}

impl<'a, T: Copy> MutChunks<'a, T> {
    pub fn write(&mut self, src: &[T]) -> usize {
        if self.available() <= 0 {
            return 0;
        }
        // Read from head if the cursor hasn't advanced past the head
        let (mut write, pos) = if self.pos < self.head.len() {
            (safe_copy_from_slice(src, &mut self.head[self.pos..]), 0)
        } else {
            (0, self.pos - self.head.len())
        };
        // Read from tail if there's still buffer to fill
        if write < src.len() {
            write += safe_copy_from_slice(&src[write..], &mut self.tail[pos..]);
        }
        self.pos += write;
        write
    }
}

#[inline]
fn safe_copy_from_slice<T>(src: &[T], dst: &mut [T]) -> usize
where
    T: Copy,
{
    let len = usize::min(src.len(), dst.len());
    dst[..len].copy_from_slice(&src[..len]);
    len
}

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

        let chunks = rb.read_chunks();
        assert_eq!(chunks.remaining(), rb.remaining());

        let chunks = rb.write_chunks();
        assert_eq!(chunks.available(), rb.available());

        let sample = b"Hello world!";
        let length = sample.len();
        let mut buf = [0u8; 24];
        for _ in 0..10 {
            // Test write
            let mut dst = rb.write_chunks();
            assert_eq!(dst.available(), rb.available());

            let write = dst.write(sample);
            assert_eq!(write, length);
            dst.flush();
            assert_eq!(rb.remaining(), write);

            // Test read
            let mut src = rb.read_chunks();
            assert_eq!(src.remaining(), rb.remaining());

            let read = src.read(&mut buf);
            assert_eq!(read, length);
            src.free();

            assert_eq!(rb.remaining(), 0);
            assert_eq!(rb.available(), rb.capacity() - 1);
            assert_eq!(&buf[..length], &sample[..]);
        }
    }
}
