//! Tauri command surface. Thin: validates/forwards to the engine thread and returns JSON.

use tauri::{AppHandle, Runtime, State};

use crate::core::config::AppConfig;
use crate::core::monitor::MonitorInfo;
use crate::core::window::{WindowId, WindowInfo};
use crate::engine::activity::ActivityEntry;
use crate::engine::EngineStatus;
use crate::plugin::{self, ModuleState};

type CmdResult<T> = Result<T, String>;

#[tauri::command]
pub async fn get_monitors(state: State<'_, ModuleState>) -> CmdResult<Vec<MonitorInfo>> {
    state.with_engine(|e| e.monitors())
}

#[tauri::command]
pub async fn get_config(state: State<'_, ModuleState>) -> CmdResult<AppConfig> {
    state.with_engine(|e| e.config())
}

#[tauri::command]
pub async fn set_config(state: State<'_, ModuleState>, config: AppConfig) -> CmdResult<AppConfig> {
    state.with_engine(move |e| e.set_config(config))?
}

#[tauri::command]
pub async fn set_enabled(state: State<'_, ModuleState>, enabled: bool) -> CmdResult<AppConfig> {
    state.with_engine(move |e| {
        let mut config = e.config();
        config.enabled = enabled;
        e.set_config(config)
    })?
}

#[tauri::command]
pub async fn get_status(state: State<'_, ModuleState>) -> CmdResult<EngineStatus> {
    state.with_engine(|e| e.status())
}

#[tauri::command]
pub async fn list_windows(state: State<'_, ModuleState>) -> CmdResult<Vec<WindowInfo>> {
    state.with_engine(|e| e.list_windows())?
}

#[tauri::command]
pub async fn get_activity(state: State<'_, ModuleState>) -> CmdResult<Vec<ActivityEntry>> {
    state.with_engine(|e| e.activity())
}

#[tauri::command]
pub async fn release_window(state: State<'_, ModuleState>, id: u64) -> CmdResult<()> {
    state.with_engine(move |e| e.release_window(WindowId(id)))
}

#[tauri::command]
pub async fn release_all(state: State<'_, ModuleState>) -> CmdResult<()> {
    state.with_engine(|e| e.release_all())
}

#[tauri::command]
pub async fn module_start<R: Runtime>(app: AppHandle<R>) -> CmdResult<()> {
    plugin::start(&app)
}

#[tauri::command]
pub async fn module_stop<R: Runtime>(app: AppHandle<R>) -> CmdResult<()> {
    plugin::stop(&app, plugin::STOP_TIMEOUT);
    Ok(())
}

/// `"off"` (nothing running), `"paused"` (running, `enabled = false`) or `"on"`.
#[tauri::command]
pub async fn module_status(state: State<'_, ModuleState>) -> CmdResult<&'static str> {
    Ok(state.status())
}
