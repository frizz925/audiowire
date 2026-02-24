use audiowire_serde::{Deserialize, Serialize};
use bytes::{BufMut, Bytes, BytesMut};

use super::{command, handshake};

pub struct OutgoingMessage<T> {
    pub code: u8,
    pub message: T,
}

impl<T: Serialize> OutgoingMessage<T> {
    pub fn into_bytes(self) -> Bytes {
        let mut buf = BytesMut::new();
        self.serialize(&mut buf);
        buf.freeze()
    }
}

impl<T: Serialize> Serialize for OutgoingMessage<T> {
    fn serialize(&self, buf: &mut impl BufMut) {
        buf.put_u8(self.code);
        self.message.serialize(buf);
    }
}

/*
impl Deserialize for IncomingMessage {
    fn deserialize(buf: &mut impl Buf) -> Result<Self, TryGetError> {
        let message = match buf.try_get_u8()? {
            10 => Self::Data(buf.copy_to_bytes(buf.remaining())),
            code => Self::Unknown(code),
        };
        Ok(message)
    }
}
*/

macro_rules! message_types {
    (
        $(
            $code:literal => ($name:ident, $type:ty),
        )*
    ) => {
        #[non_exhaustive]
        pub enum IncomingMessage {
            $(
                $name($type),
            )*
            Data(bytes::Bytes),
            Unknown(u8),
        }

        impl IncomingMessage {
            pub fn code(&self) -> u8 {
                match self {
                    $(
                        Self::$name(_) => $code,
                    )*
                    Self::Data(_) => 10,
                    Self::Unknown(code) => *code,
                }
            }
        }

        impl Deserialize for IncomingMessage {
            fn deserialize(buf: &mut impl bytes::Buf) -> Result<Self, bytes::TryGetError> {
                let message = match buf.try_get_u8()? {
                    $(
                        $code => Self::$name(<$type as Deserialize>::deserialize(buf)?),
                    )*
                    10 => Self::Data(buf.copy_to_bytes(buf.remaining())),
                    code => Self::Unknown(code),
                };
                Ok(message)
            }
        }

        $(
            impl From<$type> for OutgoingMessage<$type> {
                fn from(value: $type) -> Self {
                    Self { code: $code, message: value }
                }
            }
        )*
    };
}

message_types! {
    1 => (Handshake, handshake::Handshake),
    2 => (Command, command::Command),
}
