//! OS backends. Native code lives only here.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use self::windows::{open, recover_from};

#[cfg(not(windows))]
mod unsupported;
#[cfg(not(windows))]
pub(crate) use self::unsupported::{open, recover_from};
