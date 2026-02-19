use bytes::{Buf, BufMut, TryGetError};

pub trait Encode {
    fn encode(&self, buf: &mut impl BufMut);
}

pub trait Decode: Sized {
    // TODO: Maybe create a separate Error type for this
    fn decode(buf: &mut impl Buf) -> Result<Self, TryGetError>;
}
