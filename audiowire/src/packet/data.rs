use std::io::Read;

use audiowire_derive::{Deserialize, Serialize};
use audiowire_serde::{Deserialize, Serialize};

use super::{
    message::{DATA_MESSAGE_CODE, OutgoingMessage},
    stream::StreamId,
};

#[derive(Serialize)]
pub struct OutgoingClientData<T: Serialize>(pub StreamId, pub T);

#[derive(Serialize)]
pub struct OutgoingServerData<T: Serialize>(pub T);

#[derive(Deserialize)]
pub struct IncomingClientData<T: Deserialize>(pub StreamId, pub T);

#[derive(Deserialize)]
pub struct IncomingServerData<T: Deserialize>(pub T);

macro_rules! outgoing_data {
    (
        $($type:ty),+
    ) => {
        $(
            impl<T: Serialize> From<$type> for OutgoingMessage<$type> {
                fn from(value: $type) -> Self {
                    Self {
                        code: DATA_MESSAGE_CODE,
                        payload: value,
                    }
                }
            }
        )+
    };
}

outgoing_data!(OutgoingClientData<T>, OutgoingServerData<T>);

pub struct IncomingData<R: Read>(pub R);

impl<R: Read> IncomingData<R> {
    pub fn deserialize<T: Deserialize>(self) -> std::io::Result<T> {
        T::deserialize(self.0)
    }
}
