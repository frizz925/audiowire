use std::{
    io::{Read, Result},
    time::SystemTime,
};

use audiowire_derive::Serialize;
use audiowire_serde::{Deserialize, Serialize};

use super::{
    message::{DATA_MESSAGE_CODE, OutgoingMessage},
    stream::StreamId,
};

#[derive(Serialize)]
pub struct OutgoingClientData<T: Serialize>(pub StreamId, pub T);

#[derive(Serialize)]
pub struct OutgoingServerData<T: Serialize>(pub T);

#[derive(Serialize)]
pub struct OutgoingAudioData<T: Serialize> {
    pub sequence: u64,
    pub timestamp: SystemTime,
    pub data: T,
}

pub struct IncomingClientData<R: Read>(pub StreamId, pub R);

impl<R: Read> IncomingClientData<R> {
    pub fn deserialize(mut reader: R) -> Result<Self> {
        Ok(Self(StreamId::deserialize(&mut reader)?, reader))
    }
}

pub struct IncomingServerData;

impl IncomingServerData {
    pub fn deserialize<R: Read>(reader: R) -> Result<R> {
        Ok(reader)
    }
}

pub struct IncomingAudioData<R: Read> {
    pub sequence: u64,
    pub timestamp: SystemTime,
    pub reader: R,
}

impl<R: Read> IncomingAudioData<R> {
    pub fn deserialize(mut reader: R) -> Result<Self> {
        Ok(Self {
            sequence: u64::deserialize(&mut reader)?,
            timestamp: SystemTime::deserialize(&mut reader)?,
            reader,
        })
    }
}

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
