use bytes::BufMut;

pub trait Serialize {
    fn serialize(&self, buf: &mut impl BufMut);
}

macro_rules! serialize_primitives {
    (
        $(
            ($type:ty, $method:tt),
        )+
    ) => {
        $(
            impl Serialize for $type {
                fn serialize(&self, buf: &mut impl BufMut) {
                    buf.$method(*self);
                }
            }
        )+
    };
}

serialize_primitives! {
    (u8, put_u8),
    (u16, put_u16),
    (u32, put_u32),
    (u64, put_u64),

    (i8, put_i8),
    (i16, put_i16),
    (i32, put_i32),
    (i64, put_i64),
}

impl Serialize for usize {
    fn serialize(&self, buf: &mut impl BufMut) {
        buf.put_u16(*self as u16);
    }
}

impl Serialize for &[u8] {
    fn serialize(&self, buf: &mut impl BufMut) {
        let slice = *self;
        slice.len().serialize(buf);
        buf.put_slice(slice);
    }
}
