use std::ffi::CStr;

use audiowire_sys::aw_result;

use super::errors::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[inline]
pub(super) fn parse_result(res: aw_result) -> Result<()> {
    parse_result_value(res, ())
}

#[inline]
pub(super) fn parse_result_value<T>(res: aw_result, value: T) -> Result<T> {
    if res.code != 0 {
        Err(Error::new(res.code, res.get_message()))
    } else {
        Ok(value)
    }
}

#[inline]
pub(super) fn parse_result_lazy<T, F: FnOnce() -> T>(res: aw_result, f: F) -> Result<T> {
    parse_result(res).map(|_| f())
}

#[allow(dead_code)]
pub(super) trait CResult {
    fn is_ok(&self) -> bool;
    fn is_err(&self) -> bool;
    fn get_message(&self) -> Option<String>;
}

impl CResult for aw_result {
    #[inline]
    fn is_ok(&self) -> bool {
        self.code == 0
    }

    #[inline]
    fn is_err(&self) -> bool {
        self.code != 0
    }

    #[inline]
    fn get_message(&self) -> Option<String> {
        if !self.message.is_null() {
            unsafe { Some(CStr::from_ptr(self.message).to_string_lossy().to_string()) }
        } else {
            None
        }
    }
}
