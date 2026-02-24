use std::{
    io::{Error, ErrorKind, Result},
    net::SocketAddr,
    ops::{Deref, DerefMut},
};

use audiowire_serde::{Deserialize, Serialize};
use bytes::{Buf, Bytes, BytesMut};
use tokio::net::{ToSocketAddrs, UdpSocket};

use crate::packet::message::{IncomingMessage, OutgoingMessage};

const DEFAULT_BUFFER_SIZE: usize = 65536;

trait UdpWrapperInner {
    fn buf_mut<'a>(&'a mut self) -> &'a mut BytesMut;

    fn borrow_mut<'a>(&'a mut self) -> (&'a UdpSocket, &'a mut BytesMut);
}

pub trait UdpWrapper: Sized {
    fn recv_from(&mut self) -> impl Future<Output = Result<(IncomingMessage, SocketAddr)>>;

    fn send_to<T, A>(&mut self, value: T, addr: A) -> impl Future<Output = Result<()>>
    where
        T: Into<OutgoingMessage<T>> + Serialize,
        A: ToSocketAddrs;

    fn raw_recv_from(&mut self) -> impl Future<Output = Result<(Bytes, SocketAddr)>>;

    fn raw_send_to<B, A>(&self, buf: B, addr: A) -> impl Future<Output = Result<usize>>
    where
        B: AsRef<[u8]>,
        A: ToSocketAddrs;

    fn enter<'a>(&'a mut self) -> WrapperGuard<'a, Self> {
        WrapperGuard { inner: self }
    }

    fn reset(&mut self);
}

impl<U: UdpWrapperInner + AsRef<UdpSocket>> UdpWrapper for U {
    async fn recv_from(&mut self) -> Result<(IncomingMessage, SocketAddr)> {
        let (mut buf, addr) = self.raw_recv_from().await?;
        let message = IncomingMessage::deserialize(&mut buf)
            .map_err(|e| Error::new(ErrorKind::UnexpectedEof, e))?;
        Ok((message, addr))
    }

    async fn send_to<T, A>(&mut self, value: T, addr: A) -> Result<()>
    where
        T: Into<OutgoingMessage<T>> + Serialize,
        A: ToSocketAddrs,
    {
        let buf = {
            let buf = self.buf_mut();
            buf.clear();
            T::into(value).serialize(buf);
            buf.copy_to_bytes(buf.remaining())
        };
        self.raw_send_to(buf, addr).await?;
        Ok(())
    }

    async fn raw_recv_from(&mut self) -> Result<(Bytes, SocketAddr)> {
        let (sock, buf) = self.borrow_mut();
        buf.clear();
        let (recv, addr) = sock.recv_buf_from(buf).await?;
        Ok((buf.copy_to_bytes(recv), addr))
    }

    async fn raw_send_to<B, A>(&self, buf: B, addr: A) -> Result<usize>
    where
        B: AsRef<[u8]>,
        A: ToSocketAddrs,
    {
        self.as_ref().send_to(buf.as_ref(), addr).await
    }

    fn reset(&mut self) {
        let buf = self.buf_mut();
        buf.clear();
        buf.reserve(DEFAULT_BUFFER_SIZE);
    }
}

pub struct WrapperGuard<'a, T: UdpWrapper> {
    inner: &'a mut T,
}

impl<'a, T: UdpWrapper> AsRef<T> for WrapperGuard<'a, T> {
    fn as_ref(&self) -> &T {
        self.inner
    }
}

impl<'a, T: UdpWrapper> AsMut<T> for WrapperGuard<'a, T> {
    fn as_mut(&mut self) -> &mut T {
        self.inner
    }
}

impl<'a, T: UdpWrapper> Deref for WrapperGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

impl<'a, T: UdpWrapper> DerefMut for WrapperGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
    }
}

impl<'a, T: UdpWrapper> Drop for WrapperGuard<'a, T> {
    fn drop(&mut self) {
        self.reset();
    }
}

pub struct BorrowedUdpWrapper<S: AsRef<UdpSocket>> {
    inner: S,
    buf: BytesMut,
}

impl<S: AsRef<UdpSocket>> BorrowedUdpWrapper<S> {
    pub fn as_inner(&self) -> &S {
        &self.inner
    }
}

impl<S: AsRef<UdpSocket>> UdpWrapperInner for BorrowedUdpWrapper<S> {
    fn buf_mut<'a>(&'a mut self) -> &'a mut BytesMut {
        &mut self.buf
    }

    fn borrow_mut<'a>(&'a mut self) -> (&'a UdpSocket, &'a mut BytesMut) {
        (self.inner.as_ref(), &mut self.buf)
    }
}

impl<S: AsRef<UdpSocket>> AsRef<UdpSocket> for BorrowedUdpWrapper<S> {
    fn as_ref(&self) -> &UdpSocket {
        self.as_inner().as_ref()
    }
}

pub struct OwnedUdpWrapper {
    inner: UdpSocket,
    buf: BytesMut,
}

impl OwnedUdpWrapper {
    pub fn into_inner(self) -> UdpSocket {
        self.inner
    }
}

impl UdpWrapperInner for OwnedUdpWrapper {
    fn buf_mut<'a>(&'a mut self) -> &'a mut BytesMut {
        &mut self.buf
    }

    fn borrow_mut<'a>(&'a mut self) -> (&'a UdpSocket, &'a mut BytesMut) {
        (&self.inner, &mut self.buf)
    }
}

impl Into<UdpSocket> for OwnedUdpWrapper {
    fn into(self) -> UdpSocket {
        self.into_inner()
    }
}

impl AsRef<UdpSocket> for OwnedUdpWrapper {
    fn as_ref(&self) -> &UdpSocket {
        &self.inner
    }
}

pub fn wrap_udp<S>(sock: S) -> BorrowedUdpWrapper<S>
where
    S: AsRef<UdpSocket>,
{
    BorrowedUdpWrapper {
        inner: sock,
        buf: BytesMut::with_capacity(DEFAULT_BUFFER_SIZE),
    }
}

pub fn wrap_udp_owned(sock: UdpSocket) -> OwnedUdpWrapper {
    OwnedUdpWrapper {
        inner: sock,
        buf: BytesMut::with_capacity(DEFAULT_BUFFER_SIZE),
    }
}
