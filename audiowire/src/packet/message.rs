use audiowire_derive::Serialize;
use audiowire_serde::{Deserialize, Serialize};

use super::{command, handshake};

pub const DATA_MESSAGE_CODE: u8 = 128;

#[derive(Serialize)]
pub struct OutgoingMessage<T: Serialize> {
    pub code: u8,
    pub payload: T,
}

macro_rules! message_types {
    (
        $(
            $code:literal => ($name:ident, $type:ty),
        )*
    ) => {
        #[non_exhaustive]
        pub enum IncomingMessage<R> {
            $(
                $name($type),
            )*
            Data(R),
            Unknown(u8),
        }

        impl<R> IncomingMessage<R> {
            pub fn code(&self) -> u8 {
                match self {
                    $(
                        Self::$name(_) => $code,
                    )*
                    Self::Data(_) => DATA_MESSAGE_CODE,
                    Self::Unknown(code) => *code,
                }
            }
        }

        impl<R: std::io::Read> IncomingMessage<R> {
            pub fn deserialize(mut reader: R) -> std::io::Result<Self> {
                let message = match u8::deserialize(&mut reader)? {
                    $(
                        $code => Self::$name(<$type as Deserialize>::deserialize(&mut reader)?),
                    )*
                    DATA_MESSAGE_CODE => Self::Data(reader),
                    code => Self::Unknown(code),
                };
                Ok(message)
            }
        }

        $(
            impl From<$type> for OutgoingMessage<$type> {
                fn from(value: $type) -> Self {
                    Self { code: $code, payload: value }
                }
            }
        )*
    };
}

message_types! {
    1 => (Handshake, handshake::Handshake),
    11 => (ServerCommand, command::server::ServerCommand),
    21 => (ClientCommand, command::client::ClientCommand),
}
