//! OS-specific helpers. Native code lives only here.

use std::path::PathBuf;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::*;

#[cfg(not(windows))]
mod unsupported;
#[cfg(not(windows))]
pub use self::unsupported::*;

/// A running process found by executable name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: u32,
    /// Full image path; `None` when the process cannot be queried (e.g. elevated).
    pub exe: Option<PathBuf>,
}

/// The values of an `HKCU\...\Uninstall\<key>` entry the shell cares about.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UninstallEntry {
    pub display_version: Option<String>,
    pub install_location: Option<String>,
    pub uninstall_string: Option<String>,
}
