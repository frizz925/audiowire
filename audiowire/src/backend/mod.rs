pub mod config;
pub mod error;
pub mod result;
pub mod stream;

use audiowire_sys::{aw_initialize, aw_terminate};

use result::{Result, parse_result};

#[inline]
pub fn initialize() -> Result<()> {
    parse_result(unsafe { aw_initialize() })
}

#[inline]
pub fn terminate() -> Result<()> {
    parse_result(unsafe { aw_terminate() })
}
