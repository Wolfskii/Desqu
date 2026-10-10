//! OS backends. Native code lives only here.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use self::windows::{item_position, open, recover_from};

#[cfg(not(windows))]
mod unsupported;
#[cfg(not(windows))]
pub(crate) use self::unsupported::{item_position, open, recover_from};
