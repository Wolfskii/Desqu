//! OS access: virtual desktops, other apps' windows and launching. Native code lives only here.
//!
//! Only documented surfaces are used (see `docs/virtual-desktop-research.md`): the virtual desktop
//! registry layout, the public `IVirtualDesktopManager`, `IShellWindows`, process queries and
//! `ShellExecuteW`. Foreign windows are never moved.

use serde::Serialize;

use crate::core::apps::RunningApp;

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

/// Apps with a normal window on the given desktop (`None` = the current one).
pub fn running_apps(desktop_id: Option<&str>) -> Vec<RunningApp> {
    imp::running_apps(desktop_id)
}

/// Local folders open in Explorer on the given desktop (`None` = the current one).
pub fn explorer_folders(desktop_id: Option<&str>) -> Vec<String> {
    imp::explorer_folders(desktop_id)
}

pub fn launch_app(exe: &str, args: Option<&str>) -> Result<(), String> {
    imp::launch_app(exe, args)
}

pub fn open_folder(path: &str) -> Result<(), String> {
    imp::open_folder(path)
}
