//! OS backends. Native code lives only here.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use self::windows::{desktop_folders, folder_at, icon_png_standalone, icons_visible, item_at, item_position, nudge_timing, open, recover_from, selected_items, selection_count, surface_probe, thumbnail_png_standalone};

#[cfg(not(windows))]
mod unsupported;
#[cfg(not(windows))]
pub(crate) use self::unsupported::{desktop_folders, folder_at, icon_png_standalone, item_at, item_position, open, recover_from, selected_items, selection_count, thumbnail_png_standalone};
