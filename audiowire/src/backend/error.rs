use std::{
    ffi::{CStr, c_char, c_int},
    fmt::Display,
};

#[derive(Debug)]
pub struct Error {
    code: i32,
    message: Option<String>,
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(message) = &self.message {
            write!(f, "code: {}, message: {}", self.code, message)
        } else {
            write!(f, "code: {}", self.code)
        }
    }
}

impl std::error::Error for Error {}

impl Error {
    pub(super) fn new(code: c_int, message: *const c_char) -> Self {
        let message = if !message.is_null() {
            unsafe { CStr::from_ptr(message) }
                .to_str()
                .map(str::to_string)
                .ok()
        } else {
            None
        };
        Self { code, message }
    }
}

impl slog::Value for Error {
    fn serialize(
        &self,
        _rec: &slog::Record<'_>,
        key: slog::Key,
        serializer: &mut dyn slog::Serializer,
    ) -> slog::Result {
        serializer.emit_str(key, self.to_string().as_str())
    }
}
