use audiowire_derive::{Deserialize, Serialize};

use crate::message_enum;

pub const SERVER_HEARTBEAT: ServerCommand = ServerCommand::Heartbeat(ServerHeartbeat);
pub const SERVER_CLOSE: ServerCommand = ServerCommand::Close(ServerClose);

#[derive(Serialize, Deserialize)]
pub struct ServerHeartbeat;

#[derive(Serialize, Deserialize)]
pub struct ServerClose;

message_enum! {
    ServerCommand;
    1 => (Heartbeat, ServerHeartbeat),
    255 => (Close, ServerClose),
}
