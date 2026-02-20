use audiowire_serde::{Deserialize, Serialize};
use bytes::{Buf, BufMut, Bytes, BytesMut, TryGetError};

use super::handshake;

pub trait IntoMessage: Sized {
    fn into_message(self) -> EncodedMessage<Self>;
}

pub trait Pack {
    fn pack(self) -> Bytes;
}

pub struct EncodedMessage<T> {
    code: u8,
    message: T,
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

impl<T: Serialize + IntoMessage> Pack for T {
    fn pack(self) -> Bytes {
        self.into_message().pack()
    }
}

macro_rules! message_types {
    (
        $(
            ($code:expr, $name:ident, $type:ty),
        )+
    ) => {
        #[non_exhaustive]
        pub enum DecodedMessage {
            Unknown(u8),
            $(
                $name($type),
            )+
        }

        impl DecodedMessage {
            pub fn code(&self) -> u8 {
                match self {
                    Self::Unknown(code) => *code,
                    $(
                        Self::$name(_) => $code,
                    )+
                }
            }
        }

        impl Deserialize for DecodedMessage {
            fn deserialize(buf: &mut impl Buf) -> Result<Self, TryGetError> {
                let message = match buf.try_get_u8()? {
                    $(
                        $code => Self::$name(<$type as Deserialize>::deserialize(buf)?),
                    )+
                    code => Self::Unknown(code),
                };
                Ok(message)
            }
        }

        $(
            impl IntoMessage for $type {
                fn into_message(self) -> EncodedMessage<Self> {
                    EncodedMessage::from(self)
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
    (1, HandshakeInit, handshake::HandshakeInit),
    (2, HandshakeReply, handshake::HandshakeReply),
    (3, HandshakeAck, handshake::HandshakeAck),
}
