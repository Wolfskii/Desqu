//! Tauri command surface. Thin: validates, forwards to the engine state and returns JSON.

use desqu_desktop::ItemKind;
use desqu_shell::module::ModuleStatus;
use serde::Serialize;
use tauri::{AppHandle, Runtime, State};

use crate::core::apps::{missing_apps, RunningApp};
use crate::core::b64;
use crate::core::model::WorkspacesConfig;
use crate::engine::{self, ModuleState};
use crate::platform;

type CmdResult<T> = Result<T, String>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEntry {
    pub id: String,
    /// The desktop's own name, or "Desktop N" when it has none.
    pub name: String,
    pub current: bool,
    pub has_workspace: bool,
    /// The workspace exists but its virtual desktop was removed.
    pub removed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemInfo {
    pub id: String,
    pub name: String,
    pub kind: ItemKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopStatus {
    pub running: bool,
    pub current_desktop: Option<String>,
    /// `false` when auto-arrange is on, so items cannot be hidden.
    pub can_hide: Option<bool>,
    pub problem: Option<String>,
    pub hidden_count: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReport {
    pub launched: Vec<String>,
    pub already_open: Vec<String>,
    pub failed: Vec<String>,
}

#[tauri::command]
pub async fn get_config(state: State<'_, ModuleState>) -> CmdResult<WorkspacesConfig> {
    Ok(state.config())
}

#[tauri::command]
pub async fn set_config(
    state: State<'_, ModuleState>,
    config: WorkspacesConfig,
) -> CmdResult<WorkspacesConfig> {
    state.set_config(config)
}

/// Every virtual desktop (in Windows' order) plus workspaces whose desktop no longer exists.
#[tauri::command]
pub async fn list_desktops(state: State<'_, ModuleState>) -> CmdResult<Vec<DesktopEntry>> {
    let desktops = platform::desktops()?;
    let cfg = state.config();
    let mut out: Vec<DesktopEntry> = desktops
        .list
        .iter()
        .enumerate()
        .map(|(i, d)| DesktopEntry {
            id: d.id.clone(),
            name: if d.name.is_empty() {
                format!("Desktop {}", i + 1)
            } else {
                d.name.clone()
            },
            current: desktops.current.as_deref() == Some(d.id.as_str()),
            has_workspace: cfg.workspace(&d.id).is_some(),
            removed: false,
        })
        .collect();
    for ws in &cfg.workspaces {
        if !desktops.list.iter().any(|d| d.id == ws.desktop_id) {
            out.push(DesktopEntry {
                id: ws.desktop_id.clone(),
                name: if ws.name.is_empty() {
                    "Removed desktop".into()
                } else {
                    ws.name.clone()
                },
                current: false,
                has_workspace: true,
                removed: true,
            });
        }
    }
    Ok(out)
}

#[tauri::command]
pub async fn list_items(state: State<'_, ModuleState>) -> CmdResult<Vec<ItemInfo>> {
    Ok(state
        .layer()?
        .items()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|i| ItemInfo {
            id: i.id,
            name: i.name,
            kind: i.kind,
        })
        .collect())
}

/// The item's icon as a PNG data URL (cached; extraction costs a few ms per icon).
#[tauri::command]
pub async fn item_icon(
    state: State<'_, ModuleState>,
    id: String,
    size: Option<u32>,
) -> CmdResult<String> {
    let size = size.unwrap_or(32).clamp(16, 128);
    let key = format!("{id}|{size}");
    if let Some(hit) = state.cached_icon(&key) {
        return Ok(hit);
    }
    let png = state
        .layer()?
        .icon_png(&id, size)
        .map_err(|e| e.to_string())?;
    let url = b64::png_data_url(&png);
    state.cache_icon(key, url.clone());
    Ok(url)
}

/// Saves the apps and folders open on a desktop into its workspace (created when missing).
#[tauri::command]
pub async fn capture_workspace(
    state: State<'_, ModuleState>,
    desktop_id: String,
) -> CmdResult<WorkspacesConfig> {
    let desktops = platform::desktops()?;
    let index = desktops
        .list
        .iter()
        .position(|d| d.id == desktop_id)
        .ok_or("that virtual desktop does not exist")?;
    let name = &desktops.list[index].name;
    let name = if name.is_empty() {
        format!("Desktop {}", index + 1)
    } else {
        name.clone()
    };

    let (apps, folders) = engine::capture(&desktop_id);
    let mut cfg = state.config();
    let ws = cfg.workspace_mut_or_create(&desktop_id, &name);
    ws.name = name;
    ws.apps = apps;
    ws.folders = folders;
    state.set_config(cfg)
}

/// Starts the workspace's apps and folders that are not open yet. They open on the desktop that is
/// current, which is why only the current desktop's workspace can be restored: Windows does not let
/// an app move another app's windows between desktops.
#[tauri::command]
pub async fn restore_workspace(
    state: State<'_, ModuleState>,
    desktop_id: String,
) -> CmdResult<RestoreReport> {
    let current =
        platform::current_desktop().ok_or("could not tell which virtual desktop is current")?;
    if current != desktop_id {
        return Err(
            "Switch to that desktop first: Windows opens new windows on the desktop you are on."
                .into(),
        );
    }
    let cfg = state.config();
    let ws = cfg
        .workspace(&desktop_id)
        .ok_or("this desktop has no workspace yet")?
        .clone();

    let running: Vec<RunningApp> = platform::running_apps(None);
    let mut report = RestoreReport::default();
    let missing = missing_apps(&ws.apps, &running);
    for app in &ws.apps {
        if !missing.iter().any(|m| std::ptr::eq(*m, app)) {
            report.already_open.push(app.display_name());
        }
    }
    for app in missing {
        match platform::launch_app(&app.exe, app.args.as_deref()) {
            Ok(()) => report.launched.push(app.display_name()),
            Err(e) => report.failed.push(format!("{}: {e}", app.display_name())),
        }
    }
    let open_folders = platform::explorer_folders(None);
    for folder in &ws.folders {
        if open_folders.iter().any(|f| f.eq_ignore_ascii_case(folder)) {
            report.already_open.push(folder.clone());
            continue;
        }
        match platform::open_folder(folder) {
            Ok(()) => report.launched.push(folder.clone()),
            Err(e) => report.failed.push(format!("{folder}: {e}")),
        }
    }
    Ok(report)
}

#[tauri::command]
pub async fn desktop_status(state: State<'_, ModuleState>) -> CmdResult<DesktopStatus> {
    let layer = state.layer().ok();
    Ok(DesktopStatus {
        running: layer.is_some(),
        current_desktop: platform::current_desktop(),
        can_hide: layer.as_ref().and_then(|l| l.can_hide().ok()),
        problem: state.problem(),
        hidden_count: layer
            .as_ref()
            .and_then(|l| l.hidden().ok())
            .map_or(0, |h| h.len()),
    })
}

#[tauri::command]
pub async fn module_start<R: Runtime>(app: AppHandle<R>) -> CmdResult<()> {
    engine::start(&app)
}

#[tauri::command]
pub async fn module_stop<R: Runtime>(app: AppHandle<R>) -> CmdResult<()> {
    engine::stop(&app);
    Ok(())
}

/// `off`, `on` or `ownedElsewhere`.
#[tauri::command]
pub async fn module_status(state: State<'_, ModuleState>) -> CmdResult<ModuleStatus> {
    Ok(status(&state))
}

pub(crate) fn status(state: &ModuleState) -> ModuleStatus {
    if state.is_running() {
        ModuleStatus::On
    } else if state.owned_elsewhere() {
        ModuleStatus::OwnedElsewhere
    } else {
        ModuleStatus::Off
    }
}
