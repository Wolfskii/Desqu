//! Tauri command surface. Thin: validates, forwards to the engine state and returns JSON.

use desqu_desktop::ItemKind;
use desqu_shell::module::ModuleStatus;
use serde::Serialize;
use tauri::{AppHandle, Runtime, State};

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
    pub item_count: usize,
    /// The workspace exists but its virtual desktop was removed.
    pub removed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemInfo {
    pub id: String,
    pub name: String,
    pub kind: ItemKind,
    /// The virtual desktop that owns it; `None` = shown on every desktop.
    pub owner: Option<String>,
    /// Currently hidden on this desktop.
    pub hidden: bool,
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
    /// What the last pass over the desktop did.
    pub last: Option<crate::engine::ReconcileInfo>,
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
    let count = |id: &str| cfg.workspace(id).map_or(0, |w| w.items.len());
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
            item_count: count(&d.id),
            removed: false,
        })
        .collect();
    for ws in cfg
        .workspaces
        .iter()
        .filter(|w| !desktops.list.iter().any(|d| d.id == w.desktop_id))
    {
        out.push(DesktopEntry {
            id: ws.desktop_id.clone(),
            name: if ws.name.is_empty() {
                "Removed desktop".into()
            } else {
                ws.name.clone()
            },
            current: false,
            item_count: ws.items.len(),
            removed: true,
        });
    }
    Ok(out)
}

#[tauri::command]
pub async fn list_items(state: State<'_, ModuleState>) -> CmdResult<Vec<ItemInfo>> {
    let layer = state.layer()?;
    let cfg = state.config();
    Ok(layer
        .items()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|i| ItemInfo {
            owner: cfg.owner_of(&i.id).map(str::to_string),
            id: i.id,
            name: i.name,
            kind: i.kind,
            hidden: i.parked,
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

/// Ties an item to a virtual desktop, or (with `desktopId: null`) shows it on every desktop again.
#[tauri::command]
pub async fn assign_item(
    state: State<'_, ModuleState>,
    item_id: String,
    desktop_id: Option<String>,
) -> CmdResult<WorkspacesConfig> {
    let mut cfg = state.config();
    match desktop_id {
        None => {
            cfg.assign(&item_id, None);
        }
        Some(id) => {
            let desktops = platform::desktops()?;
            let index = desktops
                .list
                .iter()
                .position(|d| d.id == id)
                .ok_or("that virtual desktop does not exist")?;
            let name = &desktops.list[index].name;
            let name = if name.is_empty() {
                format!("Desktop {}", index + 1)
            } else {
                name.clone()
            };
            cfg.assign(&item_id, Some((&id, &name)));
        }
    }
    state.set_config(cfg)
}

/// First run: gives every desktop item nobody owns yet to one virtual desktop (or, with `None`,
/// keeps them on every desktop) and remembers that the question was answered.
#[tauri::command]
pub async fn assign_all_items(
    state: State<'_, ModuleState>,
    desktop_id: Option<String>,
) -> CmdResult<WorkspacesConfig> {
    let mut cfg = state.config();
    if let Some(id) = desktop_id {
        let desktops = platform::desktops()?;
        let index = desktops
            .list
            .iter()
            .position(|d| d.id == id)
            .ok_or("that virtual desktop does not exist")?;
        let name = &desktops.list[index].name;
        let name = if name.is_empty() {
            format!("Desktop {}", index + 1)
        } else {
            name.clone()
        };
        for item in state.layer()?.items().map_err(|e| e.to_string())? {
            if cfg.owner_of(&item.id).is_none() {
                cfg.assign(&item.id, Some((&id, &name)));
            }
        }
    }
    cfg.initialized = true;
    state.set_config(cfg)
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
        last: state.last_reconcile(),
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
