//! OS backends. Native code lives only here.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use self::windows::{desktop_folders, folder_at, icons_visible, item_position, nudge_timing, open, recover_from, selection_count, surface_probe};

#[cfg(not(windows))]
mod unsupported;
#[cfg(not(windows))]
pub(crate) use self::unsupported::{desktop_folders, folder_at, item_position, open, recover_from, selection_count};
