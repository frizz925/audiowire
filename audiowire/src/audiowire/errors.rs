use std::fmt::Display;

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
    pub fn new(code: i32, message: Option<String>) -> Self {
        Self { code, message }
    }
}
