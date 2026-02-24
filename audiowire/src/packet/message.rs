use audiowire_serde::{Deserialize, Serialize};
use bytes::{Buf, BufMut, Bytes, BytesMut, TryGetError};

use super::{Pack, command, handshake};

pub struct EncodedMessage<T> {
    pub code: u8,
    pub message: T,
}

impl<T: Serialize> Serialize for EncodedMessage<T> {
    fn serialize(&self, buf: &mut impl BufMut) {
        buf.put_u8(self.code);
        self.message.serialize(buf);
    }
}

impl<T: Serialize> Pack for EncodedMessage<T> {
    fn pack(self) -> Bytes {
        let mut buf = BytesMut::with_capacity(2048);
        self.serialize(&mut buf);
        buf.freeze()
    }
}

/*
impl Deserialize for DecodedMessage {
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
        )+
    ) => {
        #[non_exhaustive]
        pub enum DecodedMessage {
            $(
                $name($type),
            )+
            Data(Bytes),
            Unknown(u8),
        }

        impl DecodedMessage {
            pub fn code(&self) -> u8 {
                match self {
                    $(
                        Self::$name(_) => $code,
                    )+
                    Self::Data(_) => 10,
                    Self::Unknown(code) => *code,
                }
            }
        }

        impl Deserialize for DecodedMessage {
            fn deserialize(buf: &mut impl Buf) -> Result<Self, TryGetError> {
                let message = match buf.try_get_u8()? {
                    $(
                        $code => Self::$name(<$type as Deserialize>::deserialize(buf)?),
                    )+
                    10 => Self::Data(buf.copy_to_bytes(buf.remaining())),
                    code => Self::Unknown(code),
                };
                Ok(message)
            }
        }

        $(
            impl Pack for $type {
                fn pack(self) -> Bytes {
                    EncodedMessage::from(self).pack()
                }
            }

            impl From<$type> for EncodedMessage<$type> {
                fn from(value: $type) -> Self {
                    Self { code: $code, message: value }
                }
            }
        )+
    };
}

message_types! {
    1 => (Handshake, handshake::Handshake),
    2 => (Command, command::Command),
}
