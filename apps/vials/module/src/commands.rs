//! Tauri command surface. Thin: validates, forwards to the engine state and returns JSON.

use std::collections::HashSet;

use desqu_desktop::ItemKind;
use desqu_shell::module::ModuleStatus;
use serde::Serialize;
use tauri::{AppHandle, Runtime, State};

use crate::core::b64;
use crate::core::model::VialsConfig;
use crate::engine::{self, ModuleState};

type CmdResult<T> = Result<T, String>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemInfo {
    pub id: String,
    pub name: String,
    pub kind: ItemKind,
    /// The vial holding it, if any.
    pub vial: Option<String>,
    /// Currently hidden on the desktop (parked).
    pub parked: bool,
    /// In a vial but no longer on the desktop (deleted or renamed).
    pub missing: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    pub name: String,
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopStatus {
    pub running: bool,
    /// `false` when auto-arrange is on, so items cannot be hidden.
    pub can_hide: Option<bool>,
    pub problem: Option<String>,
    pub hidden_count: usize,
}

#[tauri::command]
pub async fn get_config(state: State<'_, ModuleState>) -> CmdResult<VialsConfig> {
    Ok(state.config())
}

#[tauri::command]
pub async fn set_config(state: State<'_, ModuleState>, config: VialsConfig) -> CmdResult<VialsConfig> {
    state.set_config(config)
}

/// Desktop items plus the vial references whose item is gone.
fn collect_items(state: &ModuleState) -> CmdResult<Vec<ItemInfo>> {
    let layer = state.layer()?;
    let cfg = state.config();
    let mut seen = HashSet::new();
    let mut out: Vec<ItemInfo> = layer
        .items()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|item| {
            seen.insert(item.id.clone());
            ItemInfo {
                vial: cfg.vial_of(&item.id).map(|v| v.id.clone()),
                id: item.id,
                name: item.name,
                kind: item.kind,
                parked: item.parked,
                missing: false,
            }
        })
        .collect();
    for id in cfg.assigned_ids().into_iter().filter(|id| !seen.contains(id)) {
        let name = std::path::Path::new(&id)
            .file_stem()
            .map_or_else(|| id.clone(), |s| s.to_string_lossy().into_owned());
        out.push(ItemInfo {
            vial: cfg.vial_of(&id).map(|v| v.id.clone()),
            kind: desqu_desktop::classify(&id, false),
            id,
            name,
            parked: false,
            missing: true,
        });
    }
    Ok(out)
}

#[tauri::command]
pub async fn list_items(state: State<'_, ModuleState>) -> CmdResult<Vec<ItemInfo>> {
    collect_items(&state)
}

/// The item's icon as a PNG data URL (cached; extraction costs a few ms per icon).
#[tauri::command]
pub async fn item_icon(state: State<'_, ModuleState>, id: String, size: Option<u32>) -> CmdResult<String> {
    let size = size.unwrap_or(32).clamp(16, 128);
    let key = format!("{id}|{size}");
    if let Some(hit) = state.cached_icon(&key) {
        return Ok(hit);
    }
    let png = state.layer()?.icon_png(&id, size).map_err(|e| e.to_string())?;
    let url = b64::png_data_url(&png);
    state.cache_icon(key, url.clone());
    Ok(url)
}

/// Puts an item into a vial, or (with `vialId: null`) takes it out again.
#[tauri::command]
pub async fn assign_item(state: State<'_, ModuleState>, item_id: String, vial_id: Option<String>) -> CmdResult<VialsConfig> {
    let (config, _) = state.update_config(|cfg| cfg.assign(&item_id, vial_id.as_deref()))?;
    Ok(config)
}

/// Opens an item with its default handler. Only ids Vials knows about are accepted, so the UI
/// cannot be used to launch arbitrary paths.
#[tauri::command]
pub async fn open_item(state: State<'_, ModuleState>, id: String) -> CmdResult<()> {
    let known = state.config().assigned_ids().contains(&id)
        || state.layer()?.items().map_err(|e| e.to_string())?.iter().any(|i| i.id == id);
    if !known {
        return Err(format!("{id} is not an item on the desktop"));
    }
    desqu_shell::platform::open_url(&id)
}

#[tauri::command]
pub async fn get_monitors<R: Runtime>(app: AppHandle<R>) -> CmdResult<Vec<MonitorInfo>> {
    Ok(engine::monitors(&app)
        .into_iter()
        .map(|m| MonitorInfo {
            name: m.name,
            left: m.bounds.left,
            top: m.bounds.top,
            right: m.bounds.right,
            bottom: m.bounds.bottom,
            primary: m.primary,
        })
        .collect())
}

#[tauri::command]
pub async fn desktop_status(state: State<'_, ModuleState>) -> CmdResult<DesktopStatus> {
    let layer = state.layer().ok();
    Ok(DesktopStatus {
        running: layer.is_some(),
        can_hide: layer.as_ref().and_then(|l| l.can_hide().ok()),
        problem: state.problem(),
        hidden_count: layer.as_ref().and_then(|l| l.hidden().ok()).map_or(0, |h| h.len()),
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
