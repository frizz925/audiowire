use opus::Channels;

pub trait ChannelsParser {
    fn from_u8(value: u8) -> Channels;
}

impl ChannelsParser for Channels {
    fn from_u8(value: u8) -> Channels {
        match value {
            1 => Channels::Mono,
            2 => Channels::Stereo,
            other => panic!("Unsupported number of channels: {}", other),
        }
    }
}

pub fn convert_slice<'a, S, T>(src: &'a [S]) -> &'a [T] {
    let len = src.len() * size_of::<S>() / size_of::<T>();
    unsafe { std::slice::from_raw_parts(src.as_ptr() as *const T, len) }
}

pub fn convert_slice_mut<'a, S, T>(src: &'a mut [S]) -> &'a mut [T] {
    let len = src.len() * size_of::<S>() / size_of::<T>();
    unsafe { std::slice::from_raw_parts_mut(src.as_mut_ptr() as *mut T, len) }
}
