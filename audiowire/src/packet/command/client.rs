use std::time::Duration;

use audiowire_derive::{Deserialize, Serialize};

use crate::{message_enum, packet::stream::StreamId};

#[derive(Serialize, Deserialize)]
pub struct ClientHeartbeat(pub StreamId);

#[derive(Serialize, Deserialize)]
pub struct ClientTimeSync(pub StreamId, pub Duration);

#[derive(Serialize, Deserialize)]
pub struct ClientClose(pub StreamId);

message_enum! {
    ClientCommand;
    1 => (Heartbeat, ClientHeartbeat),
    2 => (TimeSync, ClientTimeSync),
    255 => (Close, ClientClose),
}
