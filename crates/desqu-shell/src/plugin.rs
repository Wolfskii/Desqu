//! `desqu-shell` Tauri plugin: shell settings and app info for the UI.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::Serialize;
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{AppHandle, Manager, Runtime, State};

use crate::autostart;
use crate::settings::{self, ShellSettings};

/// Must equal the Cargo package name: tauri-plugin derives the permission namespace from it.
pub const PLUGIN_NAME: &str = "desqu-shell";
pub const SETTINGS_FILE: &str = "shell.json";
/// File the NSIS installer puts next to the exe; a portable build has none.
const UNINSTALLER: &str = "uninstall.exe";

pub struct ShellState {
    path: PathBuf,
    settings: Mutex<ShellSettings>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub identifier: String,
    /// No installer footprint next to the exe, so the UI must not offer to install updates.
    pub portable: bool,
}

pub fn is_portable(exe: &Path) -> bool {
    exe.parent().map_or(true, |dir| !dir.join(UNINSTALLER).exists())
}

/// Current shell settings (defaults if the plugin is not registered).
pub fn current<R: Runtime>(app: &AppHandle<R>) -> ShellSettings {
    app.try_state::<ShellState>()
        .map(|s| lock(&s.settings).clone())
        .unwrap_or_default()
}

/// A corrupt file is kept as `shell.json.corrupt` and defaults are used, so the app still starts
/// and the user's old values are not lost.
fn load_or_default(path: &Path) -> ShellSettings {
    match settings::load(path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("unreadable {}: {e}; using defaults", path.display());
            let _ = std::fs::copy(path, path.with_extension("json.corrupt"));
            ShellSettings::default()
        }
    }
}

#[tauri::command]
pub async fn get_shell_settings(state: State<'_, ShellState>) -> Result<ShellSettings, String> {
    Ok(lock(&state.settings).clone())
}

#[tauri::command]
pub async fn set_shell_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, ShellState>,
    settings: ShellSettings,
) -> Result<ShellSettings, String> {
    settings::save(&state.path, &settings).map_err(|e| e.to_string())?;
    *lock(&state.settings) = settings.clone();
    autostart::sync(&app, settings.start_with_system)
        .map_err(|e| format!("Saved, but updating the startup entry failed: {e}"))?;
    Ok(settings)
}

#[tauri::command]
pub async fn app_info<R: Runtime>(app: AppHandle<R>) -> Result<AppInfo, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(AppInfo {
        name: app
            .config()
            .product_name
            .clone()
            .unwrap_or_else(|| app.package_info().name.clone()),
        version: app.package_info().version.to_string(),
        identifier: app.config().identifier.clone(),
        portable: is_portable(&exe),
    })
}

pub fn init<R: Runtime>(config_dir: PathBuf) -> TauriPlugin<R> {
    Builder::new(PLUGIN_NAME)
        .invoke_handler(tauri::generate_handler![
            get_shell_settings,
            set_shell_settings,
            app_info
        ])
        .setup(move |app, _api| {
            let path = config_dir.join(SETTINGS_FILE);
            let settings = load_or_default(&path);
            app.manage(ShellState { path, settings: Mutex::new(settings) });
            Ok(())
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("desqu-shell-plugin-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn exe_without_uninstaller_is_portable() {
        let dir = temp_dir("portable");
        assert!(is_portable(&dir.join("app.exe")));
        std::fs::write(dir.join(UNINSTALLER), b"").unwrap();
        assert!(!is_portable(&dir.join("app.exe")));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_settings_fall_back_to_defaults_and_keep_a_copy() {
        let dir = temp_dir("corrupt");
        let path = dir.join(SETTINGS_FILE);
        std::fs::write(&path, "{ nope").unwrap();
        assert_eq!(load_or_default(&path), ShellSettings::default());
        assert_eq!(std::fs::read_to_string(dir.join("shell.json.corrupt")).unwrap(), "{ nope");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn app_info_serializes_camel_case() {
        let info = AppInfo {
            name: "App".into(),
            version: "1.0.0".into(),
            identifier: "x.y".into(),
            portable: true,
        };
        let json = serde_json::to_value(info).unwrap();
        assert_eq!(json["portable"], true);
        assert_eq!(json["identifier"], "x.y");
    }
}
