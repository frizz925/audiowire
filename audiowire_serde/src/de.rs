use std::{
    io::{Read, Result},
    time::{Duration, SystemTime},
};

pub trait Deserialize: Sized {
    fn deserialize<R: Read>(reader: R) -> Result<Self>;
}

macro_rules! deserialize_ints {
    ($(($int:ident, $size:literal),)+) => {
        $(
            impl Deserialize for $int {
                fn deserialize<R: Read>(mut reader: R) -> Result<Self> {
                    let mut buf = [0u8; $size];
                    reader.read_exact(&mut buf)?;
                    Ok(Self::from_be_bytes(buf))
                }
            }
        )+
    };
}

deserialize_ints! {
    (u8, 1),
    (u16, 2),
    (u32, 4),
    (u64, 8),
    (u128, 16),

    (i8, 1),
    (i16, 2),
    (i32, 4),
    (i64, 8),
    (i128, 16),

    (f32, 4),
    (f64, 8),
}

impl Deserialize for bool {
    fn deserialize<R: Read>(reader: R) -> Result<Self> {
        u8::deserialize(reader).map(|v| v != 0)
    }
}

impl Deserialize for usize {
    fn deserialize<R: Read>(reader: R) -> Result<Self> {
        u16::deserialize(reader).map(|v| v as usize)
    }
}

impl Deserialize for Vec<u8> {
    fn deserialize<R: Read>(mut reader: R) -> Result<Self> {
        let len = usize::deserialize(&mut reader)?;
        let mut vec = vec![0u8; len];
        reader.read_exact(&mut vec)?;
        Ok(vec)
    }
}

impl Deserialize for SystemTime {
    fn deserialize<R: Read>(reader: R) -> Result<Self> {
        let time = SystemTime::UNIX_EPOCH
            .checked_add(Duration::deserialize(reader)?)
            .unwrap();
        Ok(time)
    }
}

impl Deserialize for Duration {
    fn deserialize<R: Read>(reader: R) -> Result<Self> {
        Ok(Duration::from_millis(u64::deserialize(reader)?))
    }
}
