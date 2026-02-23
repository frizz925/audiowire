use audiowire_derive::{Deserialize, Serialize};

use crate::{message_enum, packet::stream::StreamId};

#[derive(Serialize, Deserialize)]
pub struct CommandClose(pub StreamId);

message_enum! {
    Command;
    (1, Close, CommandClose),
}
