#[macro_export]
macro_rules! message_enum {
    (
        $enum:ident;
        $(
            $code:literal => ($name:ident, $struct:ty),
        )+
    ) => {
        pub enum $enum {
            $(
                $name($struct),
            )+
            Unknown(u8),
        }

        impl $enum {
            pub fn code(&self) -> u8 {
                match self {
                    $(
                        Self::$name(_) => $code,
                    )+
                    Self::Unknown(code) => *code,
                }
            }
        }

        impl audiowire_serde::Serialize for $enum {
            fn serialize(&self, buf: &mut impl bytes::BufMut) {
                buf.put_u8(self.code());
                match self {
                    $(
                        Self::$name(v) => v.serialize(buf),
                    )+
                    Self::Unknown(_) => (),
                }
            }
        }

        impl audiowire_serde::Deserialize for $enum {
            fn deserialize(buf: &mut impl bytes::Buf) -> Result<Self, bytes::TryGetError> {
                let payload = match buf.try_get_u8()? {
                    $(
                        $code => {
                            let v = audiowire_serde::Deserialize::deserialize(buf)?;
                            Self::$name(v)
                        }
                    )+
                    code => Self::Unknown(code),
                };
                Ok(payload)
            }
        }

        $(
            impl From<$struct> for $enum {
                fn from(value: $struct) -> Self {
                    Self::$name(value)
                }
            }
        )+
    };
}
