use std::{
    cell::UnsafeCell,
    io::{Error, ErrorKind, Read, Result, Seek, SeekFrom, Write},
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
        self.count_remaining(
            self.ridx.load(Ordering::Relaxed),
            self.widx.load(Ordering::Relaxed),
        )
    }

    /// How many bytes of data is available to write.
    #[inline]
    pub fn available(&self) -> usize {
        self.count_available(
            self.ridx.load(Ordering::Relaxed),
            self.widx.load(Ordering::Relaxed),
        )
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.mask
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

    pub fn reserve(&self, requested: usize) {
        if requested > self.mask {
            panic!(
                "Trying to reserve more than capacity. capacity={}, requested={}",
                self.mask, requested
            );
        }
        let (ridx, widx) = (
            self.ridx.load(Ordering::Acquire),
            self.widx.load(Ordering::Relaxed),
        );
        let available = self.count_available(ridx, widx);
        if requested <= available {
            return;
        }
        let offset = requested - available;
        self.ridx
            .store((ridx + offset) & self.mask, Ordering::Release);
    }

    fn count_remaining(&self, ridx: usize, widx: usize) -> usize {
        if widx >= ridx {
            widx - ridx
        } else {
            self.capacity - ridx + widx
        }
    }

    fn count_available(&self, ridx: usize, widx: usize) -> usize {
        if ridx > widx {
            ridx - widx - 1
        } else {
            self.mask - widx + ridx
        }
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
        if self.len > self.pos {
            self.len - self.pos
        } else {
            0
        }
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

    pub fn advance(&mut self, off: usize) {
        self.pos += off;
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

impl<'a> Read for Chunks<'a, u8> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        Ok(Chunks::read(self, buf))
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
        if self.pos >= self.len {
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

impl<'a> Write for ChunksMut<'a, u8> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(ChunksMut::write(self, buf))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
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

macro_rules! impl_seek {
    ($($struct:ident),+) => {
        $(
            impl<'a, T> Seek for $struct<'a, T> {
                fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
                    let (base_pos, off) = match pos {
                        SeekFrom::Start(n) => {
                            self.pos = n as usize;
                            return Ok(n);
                        }
                        SeekFrom::End(n) => (self.len, n),
                        SeekFrom::Current(n) => (self.pos, n),
                    };
                    match base_pos.checked_add_signed(off as isize) {
                        Some(n) => {
                            self.pos = n;
                            Ok(n as u64)
                        }
                        None => Err(Error::new(
                            ErrorKind::InvalidInput,
                            "Would seek to overflow or negative position",
                        )),
                    }
                }
            }
        )+
    };
}

impl_seek!(Chunks, ChunksMut);

#[cfg(test)]
mod test {
    use crate::ringbuf::RingBuf;

    #[test]
    fn test_ringbuf() {
        let req = 14;
        let rb = RingBuf::new(req);

        assert!(rb.capacity() > req);
        assert_eq!(rb.remaining(), 0);
        assert_eq!(rb.available(), rb.capacity());

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
            assert_eq!(rb.available(), rb.capacity());
            assert_eq!(&buf[..length], &sample[..]);
        }
    }
}
