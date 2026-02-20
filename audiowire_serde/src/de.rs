use bytes::{Buf, Bytes, TryGetError};

pub trait Deserialize: Sized {
    fn deserialize(buf: &mut impl Buf) -> Result<Self, TryGetError>;
}

macro_rules! deserialize_primitives {
    (
        $(
            ($type:ty, $method:tt),
        )+
    ) => {
        $(
            impl Deserialize for $type {
                fn deserialize(buf: &mut impl Buf) -> Result<Self, TryGetError> {
                    buf.$method()
                }
            }
        )+
    };
}

deserialize_primitives! {
    (u8, try_get_u8),
    (u16, try_get_u16),
    (u32, try_get_u32),
    (u64, try_get_u64),

    (i8, try_get_i8),
    (i16, try_get_i16),
    (i32, try_get_i32),
    (i64, try_get_i64),
}

impl Deserialize for usize {
    fn deserialize(buf: &mut impl Buf) -> Result<Self, TryGetError> {
        Ok(buf.try_get_u16()? as usize)
    }
}

impl Deserialize for Bytes {
    fn deserialize(buf: &mut impl Buf) -> Result<Self, TryGetError> {
        let requested = usize::deserialize(buf)?;
        let available = buf.remaining();
        if available >= requested {
            Ok(buf.copy_to_bytes(requested))
        } else {
            Err(TryGetError {
                available,
                requested,
            })
        }
    }
}
