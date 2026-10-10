//! OS-specific helpers. Native code lives only here.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
#[allow(unused_imports)] // empty until P3.5/P3.8 add helpers
pub use self::windows::*;

#[cfg(not(windows))]
mod unsupported;
#[cfg(not(windows))]
#[allow(unused_imports)] // empty until P3.5/P3.8 add helpers
pub use self::unsupported::*;
