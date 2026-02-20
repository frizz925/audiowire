use audiowire_derive::{Deserialize, Serialize};

use super::{
    stream::{StreamFlags, StreamId},
    time::NetworkTime,
};

/// Handshake packet initiated by the client to the server.
#[derive(Serialize, Deserialize)]
pub struct HandshakeInit {
    pub flags: StreamFlags,
    pub timestamp: u64,
}

/// Handshake packet sent back from the server after initiated by the client.
#[derive(Serialize, Deserialize)]
pub struct HandshakeReply {
    pub id: StreamId,
    pub flags: StreamFlags,
    pub time: NetworkTime,
}

/// Handshake acknowledgement packet sent by the client after receiving reply from the server.
#[derive(Serialize, Deserialize)]
pub struct HandshakeAck {
    pub id: StreamId,
    pub time: NetworkTime,
}
