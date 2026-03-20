use std::{
    io::{Result, Write},
    time::{Duration, SystemTime},
};

pub trait Serialize {
    fn serialize<W: Write>(&self, writer: W) -> Result<()>;
}

macro_rules! serialize_nums {
    ($($int:ty),+) => {
        $(
            impl Serialize for $int {
                fn serialize<W: Write>(&self, mut writer: W) -> Result<()> {
                    let buf = self.to_be_bytes();
                    writer.write_all(&buf)
                }
            }
        )+
    };
}

serialize_nums!(u8, u16, u32, u64, u128, i8, i16, i32, i64, i128, f32, f64);

impl Serialize for bool {
    fn serialize<W: Write>(&self, writer: W) -> Result<()> {
        let value = if *self { 1 } else { 0 };
        value.serialize(writer)
    }
}

impl Serialize for usize {
    fn serialize<W: Write>(&self, writer: W) -> Result<()> {
        (*self as u16).serialize(writer)
    }
}

impl Serialize for [u8] {
    fn serialize<W: Write>(&self, mut writer: W) -> Result<()> {
        self.len().serialize(&mut writer)?;
        writer.write_all(self)?;
        Ok(())
    }
}

impl Serialize for &[u8] {
    fn serialize<W: Write>(&self, mut writer: W) -> Result<()> {
        self.len().serialize(&mut writer)?;
        writer.write_all(*self)?;
        Ok(())
    }
}

impl Serialize for Vec<u8> {
    fn serialize<W: Write>(&self, writer: W) -> Result<()> {
        self.as_slice().serialize(writer)
    }
}

impl Serialize for String {
    fn serialize<W: Write>(&self, writer: W) -> Result<()> {
        self.as_bytes().serialize(writer)
    }
}

impl Serialize for str {
    fn serialize<W: Write>(&self, writer: W) -> Result<()> {
        self.as_bytes().serialize(writer)
    }
}

impl Serialize for SystemTime {
    fn serialize<W: Write>(&self, writer: W) -> Result<()> {
            self.duration_since(SystemTime::UNIX_EPOCH)
            .unwrap().serialize(writer)
    }
}

impl Serialize for Duration {
    fn serialize<W: Write>(&self, writer: W) -> Result<()> {
        (self.as_millis() as u64).serialize(writer)
    }
}