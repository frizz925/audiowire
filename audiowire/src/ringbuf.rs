use std::{
    cell::UnsafeCell,
    sync::atomic::{AtomicUsize, Ordering},
};

#[derive(Debug)]
pub struct RingBuf<T> {
    buf: UnsafeCell<Vec<T>>,
    ridx: AtomicUsize,
    widx: AtomicUsize,
    mask: usize,
    capacity: usize,
}

impl<T> RingBuf<T> {
    const MAX_CAPACITY: usize = (usize::MAX >> 1) + 1;

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

    pub fn read_chunks<'a>(&'a self) -> Chunks<'a, T> {
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

    pub fn write_chunks<'a>(&'a self) -> ChunksMut<'a, T> {
        let (ridx, widx) = (
            self.ridx.load(Ordering::Relaxed),
            self.widx.load(Ordering::Relaxed),
        );
        let buf = unsafe { &mut *self.buf.get() };
        if widx < ridx {
            ChunksMut::new(self, &mut buf[widx..ridx - 1], &mut [])
        } else if ridx <= 0 {
            ChunksMut::new(self, &mut buf[widx..self.mask], &mut [])
        } else {
            let (tail, head) = buf.split_at_mut(widx);
            ChunksMut::new(self, head, &mut tail[..ridx - 1])
        }
    }

    pub fn advance_write(&self, off: usize) {
        self.widx
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |x| {
                Some((x + off) & self.mask)
            })
            .unwrap();
    }
}

impl<T: Clone + Default> RingBuf<T> {
    pub fn new(requested: usize) -> Self {
        let mut capacity = 2;
        while capacity <= requested && capacity < Self::MAX_CAPACITY {
            capacity <<= 1;
        }

        Self {
            buf: UnsafeCell::new(vec![T::default(); capacity]),
            ridx: AtomicUsize::new(0),
            widx: AtomicUsize::new(0),
            mask: capacity - 1,
            capacity,
        }
    }
}

unsafe impl<T> Sync for RingBuf<T> {}
unsafe impl<T> Send for RingBuf<T> {}

pub struct Chunks<'a, T> {
    rb: &'a RingBuf<T>,
    head: &'a [T],
    tail: &'a [T],
    pos: usize,
    len: usize,
}

impl<'a, T> Chunks<'a, T> {
    pub fn new(rb: &'a RingBuf<T>, head: &'a [T], tail: &'a [T]) -> Self {
        Self {
            rb,
            head,
            tail,
            pos: 0,
            len: head.len() + tail.len(),
        }
    }

    #[inline]
    pub fn head(&'a self) -> &'a [T] {
        self.head
    }

    #[inline]
    pub fn tail(&'a self) -> &'a [T] {
        self.tail
    }

    #[inline]
    pub fn remaining(&self) -> usize {
        self.len - self.pos
    }

    #[inline]
    pub fn free(self) {
        // Consume self and drop
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

    pub fn consume(mut self) -> Vec<T> {
        let mut buf = Vec::with_capacity(self.remaining());
        let pos = if self.pos < self.head.len() {
            buf.extend_from_slice(&self.head[self.pos..]);
            0
        } else {
            self.pos - self.head.len()
        };
        buf.extend_from_slice(&self.tail[pos..]);
        self.pos += buf.len();
        buf
    }
}

impl<'a, T> Drop for Chunks<'a, T> {
    fn drop(&mut self) {
        if self.pos > 0 {
            self.rb.advance_read(self.pos);
        }
    }
}

pub struct ChunksMut<'a, T> {
    rb: &'a RingBuf<T>,
    head: &'a mut [T],
    tail: &'a mut [T],
    pos: usize,
    len: usize,
}

impl<'a, T> ChunksMut<'a, T> {
    pub fn new(rb: &'a RingBuf<T>, head: &'a mut [T], tail: &'a mut [T]) -> Self {
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
    pub fn head(&'a mut self) -> &'a mut [T] {
        self.head
    }

    #[inline]
    pub fn tail(&'a mut self) -> &'a mut [T] {
        self.tail
    }

    #[inline]
    pub fn available(&self) -> usize {
        self.len - self.pos
    }

    #[inline]
    pub fn flush(self) {
        // Consume self and drop
    }
}

impl<'a, T: Copy> ChunksMut<'a, T> {
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

impl<'a, T> Drop for ChunksMut<'a, T> {
    fn drop(&mut self) {
        if self.pos > 0 {
            self.rb.advance_write(self.pos);
        }
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
