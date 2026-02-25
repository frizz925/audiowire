#[macro_export]
macro_rules! message_enum {
    (
        $enum:ident;
        $(
            $code:literal => ($name:ident, $struct:ident),
        )+
    ) => {
        #[non_exhaustive]
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
            fn serialize<W: std::io::Write>(&self, mut writer: W) -> std::io::Result<()>
            {
                self.code().serialize(&mut writer)?;
                match self {
                    $(
                        Self::$name(v) => v.serialize(&mut writer),
                    )+
                    Self::Unknown(_) => Ok(()),
                }
            }
        }

        impl audiowire_serde::Deserialize for $enum {
            fn deserialize<R: std::io::Read>(mut reader: R) -> std::io::Result<Self> {
                let payload = match u8::deserialize(&mut reader)? {
                    $(
                        $code => {
                            Self::$name($struct::deserialize(&mut reader)?)
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
