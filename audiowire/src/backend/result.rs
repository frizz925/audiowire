use audiowire_sys::aw_result;

use super::error::Error;

pub type Result<T = (), E = Error> = std::result::Result<T, E>;

#[inline]
pub(super) fn parse_result<T>(res: aw_result, value: T) -> Result<T> {
    if res.code != 0 {
        Err(Error::new(res.code, res.message))
    } else {
        Ok(value)
    }
}

#[inline]
pub(super) fn parse_result_lazy<T, F: FnOnce() -> T>(res: aw_result, f: F) -> Result<T> {
    parse_result(res, ()).map(|_| f())
}
