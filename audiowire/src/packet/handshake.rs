use audiowire_derive::{Deserialize, Serialize};

use super::{
    stream::{StreamFlags, StreamId},
    time::NetworkTime,
};

/// Handshake packet initiated by the client to the server.
#[derive(Serialize, Deserialize)]
pub struct HandshakeInit {
    pub flags: StreamFlags,
}

/// Handshake packet sent back from the server after initiated by the client.
#[derive(Serialize, Deserialize)]
pub struct HandshakeReply {
    pub stream_id: StreamId,
    pub flags: StreamFlags,
    pub time: NetworkTime,
}

/// Handshake acknowledgement packet sent by the client after receiving reply from the server.
#[derive(Serialize, Deserialize)]
pub struct HandshakeAck {
    pub stream_id: StreamId,
    pub time: NetworkTime,
}
