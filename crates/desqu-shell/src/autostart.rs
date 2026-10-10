//! "Start with system": a Run-key entry that launches the app with `--autostart`, so the app can
//! tell a sign-in launch from a manual one and stay hidden in the tray.

use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Runtime};
use tauri_plugin_autostart::ManagerExt;

pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    tauri_plugin_autostart::Builder::new()
        .args(["--autostart"])
        .build()
}

/// Brings the OS entry in line with `enabled`; does nothing when it already is.
pub fn sync<R: Runtime>(app: &AppHandle<R>, enabled: bool) -> Result<(), String> {
    let mgr = app.autolaunch();
    let current = mgr.is_enabled().map_err(|e| e.to_string())?;
    if enabled == current {
        return Ok(());
    }
    if enabled {
        mgr.enable().map_err(|e| e.to_string())?;
    } else {
        mgr.disable().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Like [`sync`], but when enabled it always rewrites the entry once, so a 0.1.0 entry without
/// `--autostart` (or pointing at an old exe path) is replaced.
pub fn sync_at_startup<R: Runtime>(app: &AppHandle<R>, enabled: bool) -> Result<(), String> {
    if enabled {
        let mgr = app.autolaunch();
        // A failed disable only means there was nothing to remove.
        let _ = mgr.disable();
        mgr.enable().map_err(|e| e.to_string())
    } else {
        sync(app, false)
    }
}
