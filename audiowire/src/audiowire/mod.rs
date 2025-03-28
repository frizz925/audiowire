mod config;
mod errors;
mod result;
mod stream;

use audiowire_sys::{aw_initialize, aw_terminate};

use result::parse_result;

pub use config::*;
pub use errors::Error;
pub use result::Result;
pub use stream::*;

#[inline]
pub fn initialize() -> Result<()> {
    parse_result(unsafe { aw_initialize() })
}

#[inline]
pub fn terminate() -> Result<()> {
    parse_result(unsafe { aw_terminate() })
}
