//! OS access: the list of virtual desktops and a watcher for switches. Native code lives only here.
//!
//! Only the documented-by-observation registry layout is used (see
//! `docs/virtual-desktop-research.md`); no undocumented COM interface and no window is touched.

use serde::Serialize;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use self::windows as imp;

#[cfg(not(windows))]
mod unsupported;
#[cfg(not(windows))]
use self::unsupported as imp;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopInfo {
    pub id: String,
    /// Empty when the user never named the desktop.
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Desktops {
    pub list: Vec<DesktopInfo>,
    pub current: Option<String>,
}

/// Keeps a registry watcher alive; dropping it stops the thread.
pub struct SwitchWatcher {
    _inner: imp::Watcher,
}

pub fn desktops() -> Result<Desktops, String> {
    imp::desktops()
}

pub fn current_desktop() -> Option<String> {
    desktops().ok().and_then(|d| d.current)
}

/// Calls `on_change` (from a background thread) whenever the virtual desktop registry key changes.
pub fn watch_switches(on_change: Box<dyn Fn() + Send + 'static>) -> Result<SwitchWatcher, String> {
    imp::watch(on_change).map(|w| SwitchWatcher { _inner: w })
}
