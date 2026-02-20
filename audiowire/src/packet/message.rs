use audiowire_serde::{Deserialize, Serialize};
use bytes::{Buf, BufMut, TryGetError};

use super::handshake;

pub struct EncodedMessage<T> {
    code: u8,
    message: T,
}

impl<T> Serialize for EncodedMessage<T>
where
    T: Serialize,
{
    fn serialize(&self, buf: &mut impl BufMut) {
        buf.put_u8(self.code);
        Serialize::serialize(&self.message, buf);
    }
}

macro_rules! message_types {
    (
        $(
            $(#[$docs:meta])*
            ($code:expr, $name:ident, $type:ty);
        )+
    ) => {
        #[non_exhaustive]
        pub enum DecodedMessage {
            Unknown,
            $(
                $name($type),
            )+
        }

        impl DecodedMessage {
            pub fn code(&self) -> u8 {
                match self {
                    Self::Unknown => 0,
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
                    _ => Self::Unknown,
                };
                Ok(message)
            }
        }

        $(
            impl From<$type> for EncodedMessage<$type> {
                fn from(value: $type) -> Self {
                    Self { code: $code, message: value }
                }
            }
        )+
    };
}

message_types! {
    (1, HandshakeInit, handshake::HandshakeInit);
    (2, HandshakeReply, handshake::HandshakeReply);
    (3, HandshakeAck, handshake::HandshakeAck);
}
