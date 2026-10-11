//! Commands the Desqu UI calls (the modules' own commands are reached through their plugins).

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use desqu_shell::module::ModuleStatus;
use desqu_shell::platform;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State, Wry};

use crate::registry::{self, Entry};
use crate::settings::{self, ModulesFile};
use crate::{takeover, tray};

pub const EVENT_MODULES_CHANGED: &str = "desqu://modules-changed";
pub const EVENT_TAKEOVER_NEEDED: &str = "desqu://takeover-needed";
const STOP_TIMEOUT: Duration = Duration::from_secs(3);

pub struct DesquState {
    pub modules_path: PathBuf,
    /// Per-user Roaming AppData; standalone apps keep their settings in `<roaming>\<identifier>`.
    pub roaming_dir: PathBuf,
    pub modules: Mutex<ModulesFile>,
}

impl DesquState {
    pub fn file(&self) -> MutexGuard<'_, ModulesFile> {
        self.modules.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn save(&self) {
        if let Err(e) = settings::save(&self.modules_path, &self.file()) {
            tracing::warn!("could not save {}: {e}", self.modules_path.display());
        }
    }

    pub fn standalone_dir(&self, entry: &Entry) -> PathBuf {
        self.roaming_dir
            .join(entry.descriptor.standalone.identifier)
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ModuleEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub supported: bool,
    pub enabled: bool,
    pub start_with_system: bool,
    pub status: ModuleStatus,
    /// The module's standalone app is installed on this PC.
    pub standalone_installed: bool,
}

type CmdResult<T> = Result<T, String>;

fn live_status(app: &AppHandle<Wry>, entry: &Entry) -> ModuleStatus {
    if entry.descriptor.supported {
        entry.module.status(app)
    } else {
        ModuleStatus::Unsupported
    }
}

fn describe(app: &AppHandle<Wry>, state: &DesquState, entry: &Entry) -> ModuleEntry {
    let d = &entry.descriptor;
    let saved = state.file().get(d.id);
    ModuleEntry {
        id: d.id.into(),
        name: d.name.into(),
        description: d.description.into(),
        version: d.version.into(),
        supported: d.supported,
        enabled: saved.enabled,
        start_with_system: saved.start_with_system,
        status: live_status(app, entry),
        standalone_installed: platform::read_uninstall_entry(d.standalone.uninstall_key).is_some(),
    }
}

/// Desqu itself starts with the system only while a module wants that.
fn sync_autostart(app: &AppHandle<Wry>, state: &DesquState) {
    let wanted = settings::should_autostart(&state.file());
    if let Err(e) = desqu_shell::autostart::sync(app, wanted) {
        tracing::warn!("could not update the startup entry: {e}");
    }
}

fn changed(app: &AppHandle<Wry>) {
    let _ = app.emit(EVENT_MODULES_CHANGED, ());
    tray::refresh(app);
}

/// Turns a module on or off the same way for the UI and the tray.
pub fn apply_enabled(
    app: &AppHandle<Wry>,
    state: &DesquState,
    id: &str,
    enabled: bool,
) -> CmdResult<()> {
    let entry = registry::find(id).ok_or_else(|| format!("unknown module '{id}'"))?;
    if !entry.descriptor.supported {
        return Err(format!(
            "{} is not available on this operating system",
            entry.descriptor.name
        ));
    }
    settings::set_enabled(&mut state.file(), id, enabled);
    state.save();

    let result = if enabled {
        match entry.module.start(app) {
            Ok(()) => Ok(()),
            Err(_) if entry.module.status(app) == ModuleStatus::OwnedElsewhere => {
                // Not an error: the UI asks whether Desqu should take over from the standalone app.
                let _ = app.emit(EVENT_TAKEOVER_NEEDED, id);
                Ok(())
            }
            Err(msg) => {
                settings::set_enabled(&mut state.file(), id, false);
                state.save();
                Err(msg)
            }
        }
    } else {
        entry.module.stop(app, STOP_TIMEOUT);
        Ok(())
    };
    sync_autostart(app, state);
    changed(app);
    result
}

#[tauri::command]
pub async fn list_modules(
    app: AppHandle<Wry>,
    state: State<'_, DesquState>,
) -> CmdResult<Vec<ModuleEntry>> {
    let mut out = Vec::new();
    let mut dirty = false;
    for entry in registry::entries() {
        let described = describe(&app, &state, &entry);
        // A module turned on from its own page (not via Desqu's switch) is "on" for next launch too.
        if matches!(described.status, ModuleStatus::On | ModuleStatus::Paused) && !described.enabled
        {
            let mut file = state.file();
            let mut saved = file.get(entry.descriptor.id);
            saved.enabled = true;
            file.set(entry.descriptor.id, saved);
            dirty = true;
        }
        out.push(describe(&app, &state, &entry));
    }
    if dirty {
        state.save();
        sync_autostart(&app, &state);
        tray::refresh(&app);
    }
    Ok(out)
}

#[tauri::command]
pub async fn set_module_enabled(
    app: AppHandle<Wry>,
    state: State<'_, DesquState>,
    id: String,
    enabled: bool,
) -> CmdResult<ModuleEntry> {
    apply_enabled(&app, &state, &id, enabled)?;
    let entry = registry::find(&id).ok_or("unknown module")?;
    Ok(describe(&app, &state, &entry))
}

#[tauri::command]
pub async fn set_module_start_with_system(
    app: AppHandle<Wry>,
    state: State<'_, DesquState>,
    id: String,
    value: bool,
) -> CmdResult<ModuleEntry> {
    let entry = registry::find(&id).ok_or("unknown module")?;
    if !settings::set_start_with_system(&mut state.file(), &id, value) {
        return Err(format!("Turn {} on first", entry.descriptor.name));
    }
    state.save();
    sync_autostart(&app, &state);
    changed(&app);
    Ok(describe(&app, &state, &entry))
}

#[tauri::command]
pub async fn set_all_start_with_system(
    app: AppHandle<Wry>,
    state: State<'_, DesquState>,
    value: bool,
) -> CmdResult<Vec<ModuleEntry>> {
    let ids: Vec<&str> = registry::entries()
        .iter()
        .filter(|e| e.descriptor.supported)
        .map(|e| e.descriptor.id)
        .collect();
    settings::set_all_start_with_system(&mut state.file(), &ids, value);
    state.save();
    sync_autostart(&app, &state);
    changed(&app);
    Ok(registry::entries()
        .iter()
        .map(|e| describe(&app, &state, e))
        .collect())
}

#[tauri::command]
pub async fn takeover_module(
    app: AppHandle<Wry>,
    state: State<'_, DesquState>,
    id: String,
) -> CmdResult<ModuleEntry> {
    let entry = registry::find(&id).ok_or("unknown module")?;
    let dir = state.standalone_dir(&entry);
    let (app2, entry2) = (app.clone(), entry.clone());
    tauri::async_runtime::spawn_blocking(move || takeover::take_over(&app2, &entry2, &dir))
        .await
        .map_err(|e| e.to_string())??;
    {
        let mut file = state.file();
        let mut saved = file.get(&id);
        saved.enabled = true;
        file.set(&id, saved);
    }
    state.save();
    sync_autostart(&app, &state);
    changed(&app);
    Ok(describe(&app, &state, &entry))
}

/// Deletes the module's settings files (shared with its standalone app) and restarts it fresh.
#[tauri::command]
pub async fn reset_module_settings(
    app: AppHandle<Wry>,
    state: State<'_, DesquState>,
    id: String,
) -> CmdResult<ModuleEntry> {
    let entry = registry::find(&id).ok_or("unknown module")?;
    let was_running = matches!(
        entry.module.status(&app),
        ModuleStatus::On | ModuleStatus::Paused
    );
    entry.module.stop(&app, STOP_TIMEOUT);
    let dir = state.standalone_dir(&entry);
    for file in entry.descriptor.settings_files {
        match std::fs::remove_file(dir.join(file)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("could not delete {file}: {e}")),
        }
    }
    if was_running {
        entry.module.start(&app)?;
    }
    changed(&app);
    Ok(describe(&app, &state, &entry))
}

/// Runs the standalone app's own uninstaller (the user sees it).
#[tauri::command]
pub async fn remove_standalone(id: String) -> CmdResult<()> {
    let entry = registry::find(&id).ok_or("unknown module")?;
    let uninstall = platform::read_uninstall_entry(entry.descriptor.standalone.uninstall_key)
        .and_then(|u| u.uninstall_string)
        .ok_or("the standalone app is not installed")?;
    platform::launch_uninstaller(&uninstall)
}
