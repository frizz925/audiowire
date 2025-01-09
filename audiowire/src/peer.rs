use std::{future::Future, io::Result};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{tcp, TcpStream},
};

pub trait PeerReadHalf {
    type Output = ();

    fn read_exact<'a, F: Future<Output = Outpu>>(&'a mut self, buf: &'a mut [u8]) -> F;
}

pub trait PeerWriteHalf {
    fn write_all<'a, F: Future<Output = Result<()>>>(&'a mut self, buf: &'a [u8]) -> F;
}

pub trait Peer<R: PeerReadHalf, W: PeerWriteHalf>: PeerReadHalf + PeerWriteHalf {
    fn into_split(self) -> (R, W);
}

pub struct TcpPeer {
    socket: TcpStream,
}

impl Peer<tcp::OwnedReadHalf, tcp::OwnedWriteHalf> for TcpPeer {
    fn into_split(self) -> (tcp::OwnedReadHalf, tcp::OwnedWriteHalf) {
        self.socket.into_split()
    }
}

impl PeerReadHalf for TcpPeer {
    fn read_exact<'a>(&'a mut self, buf: &'a mut [u8]) -> ReadEx {
        self.socket.read_exact(buf)
    }
}

impl PeerWriteHalf for TcpPeer {
    #[inline]
    async fn write_all<'a>(&'a mut self, buf: &'a [u8]) -> Result<()> {
        AsyncWriteExt::write_all(&mut self.socket, buf).await?;
        Ok(())
    }
}

impl PeerReadHalf for tcp::OwnedReadHalf {
    #[inline]
    async fn read_exact<'a>(&'a mut self, buf: &'a mut [u8]) -> Result<()> {
        AsyncReadExt::read_exact(self, buf).await?;
        Ok(())
    }
}

impl PeerWriteHalf for tcp::OwnedWriteHalf {
    #[inline]
    async fn write_all<'a>(&'a mut self, buf: &'a [u8]) -> Result<()> {
        AsyncWriteExt::write_all(self, buf).await?;
        Ok(())
    }
}
