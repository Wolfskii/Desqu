//! OS backends. Native code lives only here.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use self::windows::{icons_visible, item_position, nudge_timing, open, recover_from, selection_count};

#[cfg(not(windows))]
mod unsupported;
#[cfg(not(windows))]
pub(crate) use self::unsupported::{item_position, open, recover_from, selection_count};
