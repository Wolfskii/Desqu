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

/// Keeps the switch watchers alive; dropping it stops their threads.
pub struct SwitchWatcher {
    _registry: Option<imp::Watcher>,
    _foreground: Option<imp::ForegroundWatcher>,
}

/// Keeps the Ctrl+V hook alive; dropping it removes the hook.
pub struct CutPasteWatcher {
    _inner: imp::PasteWatcher,
}

/// Calls `on_paste` with the paths that were cut when Ctrl+V is pressed on the desktop.
pub fn watch_cut_paste(on_paste: Box<dyn Fn(Vec<String>) + Send + 'static>) -> Result<CutPasteWatcher, String> {
    imp::PasteWatcher::start(on_paste).map(|w| CutPasteWatcher { _inner: w })
}

pub fn desktops() -> Result<Desktops, String> {
    imp::desktops()
}

pub fn current_desktop() -> Option<String> {
    desktops().ok().and_then(|d| d.current)
}

/// Calls `on_change` (from a background thread) when the virtual desktop probably changed: the
/// registry key changed, or the focused window moved to another desktop. Both are tried; it fails
/// only when neither can be watched.
pub fn watch_switches(
    on_change: std::sync::Arc<dyn Fn() + Send + Sync + 'static>,
) -> Result<SwitchWatcher, String> {
    let a = on_change.clone();
    let registry = imp::watch(Box::new(move || a()))
        .map_err(|e| tracing::debug!("registry watcher: {e}"))
        .ok();
    let b = on_change;
    let foreground = imp::ForegroundWatcher::start(Box::new(move || b()))
        .map_err(|e| tracing::debug!("foreground watcher: {e}"))
        .ok();
    if registry.is_none() && foreground.is_none() {
        return Err(
            "neither the virtual desktop registry key nor the foreground window can be watched"
                .into(),
        );
    }
    Ok(SwitchWatcher {
        _registry: registry,
        _foreground: foreground,
    })
}
