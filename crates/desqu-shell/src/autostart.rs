//! "Start with system": a Run-key entry that launches the app with `--autostart`, so the app can
//! tell a sign-in launch from a manual one and stay hidden in the tray.

use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Runtime};
use tauri_plugin_autostart::ManagerExt;

pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    let mut builder = tauri_plugin_autostart::Builder::new().args(["--autostart"]);
    let stem = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "app".into());
    if let Some(name) = debug_run_value_name(&stem, cfg!(debug_assertions)) {
        builder = builder.app_name(name);
    }
    builder.build()
}

/// Debug builds get their own Run value (`screenbound-debug`). The release value is named after
/// the product and Windows compares value names case-insensitively, so a dev build would otherwise
/// overwrite or delete the start-up entry of an installed release. `None` = keep the default.
pub fn debug_run_value_name(exe_stem: &str, debug: bool) -> Option<String> {
    debug.then(|| format!("{exe_stem}-debug"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_builds_never_share_the_release_value_name() {
        assert_eq!(debug_run_value_name("screenbound", true).as_deref(), Some("screenbound-debug"));
        assert_ne!(debug_run_value_name("screenbound", true).unwrap().to_lowercase(), "screenbound");
    }

    #[test]
    fn release_builds_keep_the_default_name() {
        assert_eq!(debug_run_value_name("screenbound", false), None);
    }
}
