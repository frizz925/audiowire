use std::{
    io::{Cursor, Error, ErrorKind, Result},
    net::SocketAddr,
    ops::Deref,
};

use audiowire_serde::Serialize;
use std::net::{ToSocketAddrs, UdpSocket};

use crate::packet::message::{IncomingMessage, OutgoingMessage};

const DEFAULT_BUFFER_SIZE: usize = 65536;

trait UdpWrapperInner {
    fn buf(&self) -> &[u8];

    fn cursor(&mut self) -> Cursor<&mut [u8]>;

    fn sock(&self) -> &UdpSocket;

    fn sock_and_buf_mut(&mut self) -> (&UdpSocket, &mut [u8]);
}

pub trait UdpWrapper: Sized {
    fn recv_message_from(&mut self) -> Result<(IncomingMessage<&[u8]>, SocketAddr)>;

    fn send_message_to<T, A>(&mut self, value: T, addr: A) -> Result<()>
    where
        T: Into<OutgoingMessage<T>> + Serialize,
        A: ToSocketAddrs;

    fn raw_recv_from(&mut self) -> Result<(&[u8], SocketAddr)>;

    fn raw_send_to<B, A>(&self, buf: B, addr: A) -> Result<()>
    where
        B: AsRef<[u8]>,
        A: ToSocketAddrs;
}

impl<U: UdpWrapperInner + AsRef<UdpSocket>> UdpWrapper for U {
    fn recv_message_from(&mut self) -> Result<(IncomingMessage<&[u8]>, SocketAddr)> {
        let (buf, addr) = self.raw_recv_from()?;
        let message =
            IncomingMessage::deserialize(buf).map_err(|_| Error::from(ErrorKind::UnexpectedEof))?;
        Ok((message, addr))
    }

    fn send_message_to<T, A>(&mut self, value: T, addr: A) -> Result<()>
    where
        T: Into<OutgoingMessage<T>> + Serialize,
        A: ToSocketAddrs,
    {
        let len = {
            let mut cur = self.cursor();
            let message = T::into(value);
            message.serialize(&mut cur).unwrap();
            cur.position() as usize
        };
        let buf = self.buf();
        self.raw_send_to(&buf[..len], addr)?;
        Ok(())
    }

    fn raw_recv_from(&mut self) -> Result<(&[u8], SocketAddr)> {
        let (sock, buf) = self.sock_and_buf_mut();
        let (len, addr) = sock.recv_from(buf)?;
        Ok((&buf[..len], addr))
    }

    fn raw_send_to<B, A>(&self, buf: B, addr: A) -> Result<()>
    where
        B: AsRef<[u8]>,
        A: ToSocketAddrs,
    {
        self.sock().send_to(buf.as_ref(), addr)?;
        Ok(())
    }
}

pub struct BorrowedUdpWrapper<S: AsRef<UdpSocket>> {
    inner: S,
    buf: [u8; DEFAULT_BUFFER_SIZE],
}

impl<S: AsRef<UdpSocket>> BorrowedUdpWrapper<S> {
    pub fn as_inner(&self) -> &S {
        &self.inner
    }
}

impl<S: AsRef<UdpSocket>> UdpWrapperInner for BorrowedUdpWrapper<S> {
    fn buf(&self) -> &[u8] {
        &self.buf
    }

    fn cursor(&mut self) -> Cursor<&mut [u8]> {
        Cursor::new(&mut self.buf)
    }

    fn sock(&self) -> &UdpSocket {
        self.inner.as_ref()
    }

    fn sock_and_buf_mut(&mut self) -> (&UdpSocket, &mut [u8]) {
        (self.inner.as_ref(), &mut self.buf)
    }
}

impl<S: AsRef<UdpSocket>> AsRef<UdpSocket> for BorrowedUdpWrapper<S> {
    fn as_ref(&self) -> &UdpSocket {
        self.as_inner().as_ref()
    }
}

impl<S: AsRef<UdpSocket>> Deref for BorrowedUdpWrapper<S> {
    type Target = UdpSocket;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

pub struct OwnedUdpWrapper {
    inner: UdpSocket,
    buf: [u8; DEFAULT_BUFFER_SIZE],
}

impl OwnedUdpWrapper {
    pub fn into_inner(self) -> UdpSocket {
        self.inner
    }
}

impl UdpWrapperInner for OwnedUdpWrapper {
    fn buf(&self) -> &[u8] {
        &self.buf
    }

    fn cursor(&mut self) -> Cursor<&mut [u8]> {
        Cursor::new(&mut self.buf)
    }

    fn sock(&self) -> &UdpSocket {
        &self.inner
    }

    fn sock_and_buf_mut(&mut self) -> (&UdpSocket, &mut [u8]) {
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

impl Deref for OwnedUdpWrapper {
    type Target = UdpSocket;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

pub fn wrap_udp<S>(sock: S) -> BorrowedUdpWrapper<S>
where
    S: AsRef<UdpSocket>,
{
    BorrowedUdpWrapper {
        inner: sock,
        buf: [0u8; DEFAULT_BUFFER_SIZE],
    }
}

pub fn wrap_udp_owned(sock: UdpSocket) -> OwnedUdpWrapper {
    OwnedUdpWrapper {
        inner: sock,
        buf: [0u8; DEFAULT_BUFFER_SIZE],
    }
}
