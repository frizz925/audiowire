use std::{future::Future, io, net::SocketAddr, sync::Arc};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{tcp, TcpStream, UdpSocket},
    sync::mpsc,
};

pub trait PeerReadHalf {
    fn read_exact<'a>(
        &'a mut self,
        buf: &'a mut [u8],
    ) -> impl Future<Output = io::Result<()>> + Send;
}

pub trait PeerWriteHalf {
    fn write_all<'a>(&'a mut self, src: &'a [u8]) -> impl Future<Output = io::Result<()>> + Send;
}

pub struct TcpPeer {
    socket: TcpStream,
}

impl TcpPeer {
    pub fn new(socket: TcpStream) -> Self {
        Self { socket }
    }

    pub fn into_split(self) -> (tcp::OwnedReadHalf, tcp::OwnedWriteHalf) {
        self.socket.into_split()
    }
}

impl PeerReadHalf for TcpPeer {
    async fn read_exact<'a>(&'a mut self, buf: &'a mut [u8]) -> io::Result<()> {
        self.socket.read_exact(buf).await?;
        Ok(())
    }
}

impl PeerWriteHalf for TcpPeer {
    async fn write_all<'a>(&'a mut self, src: &'a [u8]) -> io::Result<()> {
        self.socket.write_all(src).await?;
        Ok(())
    }
}

impl PeerReadHalf for tcp::OwnedReadHalf {
    async fn read_exact<'a>(&'a mut self, buf: &'a mut [u8]) -> io::Result<()> {
        AsyncReadExt::read_exact(self, buf).await?;
        Ok(())
    }
}

impl PeerWriteHalf for tcp::OwnedWriteHalf {
    async fn write_all<'a>(&'a mut self, src: &'a [u8]) -> io::Result<()> {
        AsyncWriteExt::write_all(self, src).await?;
        Ok(())
    }
}

pub struct UdpPeerReadHalf {
    backlog: mpsc::Receiver<Vec<u8>>,
    leftover: Option<Vec<u8>>,
}

impl PeerReadHalf for UdpPeerReadHalf {
    async fn read_exact<'a>(&'a mut self, buf: &'a mut [u8]) -> io::Result<()> {
        let buflen = buf.len();
        let mut off = if let Some(src) = self.leftover.as_deref() {
            let srclen = src.len();
            if buflen == srclen {
                buf.clone_from_slice(src);
                self.leftover = None;
                buflen
            } else if buflen < srclen {
                buf.clone_from_slice(&src[..buflen]);
                self.leftover = Some(src[buflen..].to_vec());
                buflen
            } else {
                buf[..srclen].clone_from_slice(src);
                self.leftover = None;
                buflen - srclen
            }
        } else {
            0
        };
        while off < buflen {
            let src = if let Some(buf) = self.backlog.recv().await {
                buf
            } else {
                return Err(io::ErrorKind::UnexpectedEof.into());
            };
            let srclen = src.len();
            let remaining = buflen - off;
            if remaining == srclen {
                buf[off..].clone_from_slice(&src);
                off += srclen;
            } else if remaining > srclen {
                let diff = remaining - srclen;
                let end = off + diff;
                buf[off..end].clone_from_slice(&src);
                off += srclen;
            } else {
                buf[off..].clone_from_slice(&src[..remaining]);
                self.leftover = Some(src[remaining..].to_vec());
                off += remaining;
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct UdpPeerWriteHalf {
    socket: Arc<UdpSocket>,
    addr: SocketAddr,
}

impl PeerWriteHalf for UdpPeerWriteHalf {
    async fn write_all<'a>(&'a mut self, src: &'a [u8]) -> io::Result<()> {
        if self.socket.send_to(src, &self.addr).await? >= src.len() {
            Ok(())
        } else {
            Err(io::ErrorKind::WriteZero.into())
        }
    }
}

pub struct UdpPeerProducer {
    sender: mpsc::Sender<Vec<u8>>,
}

type ProducerSendError = mpsc::error::SendError<Vec<u8>>;

impl UdpPeerProducer {
    pub async fn send(&self, src: &[u8]) -> Result<(), ProducerSendError> {
        self.sender.send(src.to_vec()).await
    }
}

pub struct UdpPeer {
    read: UdpPeerReadHalf,
    write: UdpPeerWriteHalf,
    producer: UdpPeerProducer,
}

impl UdpPeer {
    pub fn new(socket: Arc<UdpSocket>, addr: SocketAddr, backlog: usize) -> Self {
        let (tx, rx) = mpsc::channel(backlog);
        Self {
            read: UdpPeerReadHalf {
                backlog: rx,
                leftover: None,
            },
            write: UdpPeerWriteHalf { socket, addr },
            producer: UdpPeerProducer { sender: tx },
        }
    }

    pub fn into_split(self) -> (UdpPeerReadHalf, UdpPeerWriteHalf, UdpPeerProducer) {
        (self.read, self.write, self.producer)
    }
}

impl PeerReadHalf for UdpPeer {
    async fn read_exact<'a>(&'a mut self, buf: &'a mut [u8]) -> io::Result<()> {
        self.read.read_exact(buf).await
    }
}

impl PeerWriteHalf for UdpPeer {
    async fn write_all<'a>(&'a mut self, src: &'a [u8]) -> io::Result<()> {
        self.write.write_all(src).await
    }
}
