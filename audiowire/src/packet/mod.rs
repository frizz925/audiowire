use bytes::Bytes;

pub mod command;
pub mod data;
pub mod handshake;
pub mod message;
pub mod stream;
pub mod time;

pub const MAX_PACKET_SIZE: usize = 1280;

pub trait Pack {
    fn pack(self) -> Bytes;
}
