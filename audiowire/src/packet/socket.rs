use std::{
    io::{Cursor, Error, ErrorKind, Result},
    net::SocketAddr,
};

use audiowire_serde::Serialize;
use std::net::{ToSocketAddrs, UdpSocket};

use crate::packet::message::{IncomingMessage, OutgoingMessage};

const DEFAULT_BUFFER_SIZE: usize = 65536;

pub(self) mod private {
    use std::net::UdpSocket;

    pub trait Sealed {
        fn sock(&self) -> &UdpSocket;

        fn buf(&self) -> &[u8];

        fn buf_mut(&mut self) -> &mut [u8];

        fn buf_mut_and_sock(&mut self) -> (&mut [u8], &UdpSocket);
    }
}

pub trait UdpWrapper<S>: private::Sealed {
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
            let mut cur = Cursor::new(self.buf_mut());
            T::into(value).serialize(&mut cur).unwrap();
            cur.position() as usize
        };
        let buf = self.buf();
        self.raw_send_to(&buf[..len], addr)?;
        Ok(())
    }

    fn raw_recv_from(&mut self) -> Result<(&[u8], SocketAddr)> {
        let (buf, sock) = self.buf_mut_and_sock();
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

    fn as_inner(&self) -> &S;

    fn into_inner(self) -> S;
}

pub struct OwnedUdpWrapper {
    inner: UdpSocket,
    buf: [u8; DEFAULT_BUFFER_SIZE],
}

impl private::Sealed for OwnedUdpWrapper {
    fn sock(&self) -> &UdpSocket {
        &self.inner
    }

    fn buf(&self) -> &[u8] {
        self.buf.as_slice()
    }

    fn buf_mut(&mut self) -> &mut [u8] {
        self.buf.as_mut_slice()
    }

    fn buf_mut_and_sock(&mut self) -> (&mut [u8], &UdpSocket) {
        (self.buf.as_mut_slice(), &self.inner)
    }
}

impl UdpWrapper<UdpSocket> for OwnedUdpWrapper {
    fn as_inner(&self) -> &UdpSocket {
        &self.inner
    }

    fn into_inner(self) -> UdpSocket {
        self.inner
    }
}

pub struct SharedUdpWrapper<S> {
    inner: S,
    buf: [u8; DEFAULT_BUFFER_SIZE],
}

impl<S> private::Sealed for SharedUdpWrapper<S>
where
    S: AsRef<UdpSocket>,
{
    fn sock(&self) -> &UdpSocket {
        self.inner.as_ref()
    }

    fn buf(&self) -> &[u8] {
        &self.buf
    }

    fn buf_mut(&mut self) -> &mut [u8] {
        &mut self.buf
    }

    fn buf_mut_and_sock(&mut self) -> (&mut [u8], &UdpSocket) {
        (&mut self.buf, self.inner.as_ref())
    }
}

impl<S> UdpWrapper<S> for SharedUdpWrapper<S>
where
    S: AsRef<UdpSocket>,
{
    fn as_inner(&self) -> &S {
        &self.inner
    }

    fn into_inner(self) -> S {
        self.inner
    }
}

pub fn wrap_udp<S: AsRef<UdpSocket>>(sock: S) -> SharedUdpWrapper<S> {
    SharedUdpWrapper {
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
