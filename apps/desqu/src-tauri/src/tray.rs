//! Desqu's tray: Show · one check item per module · Screenbound's pause/release · updates · Exit.
//! The check states mirror the UI; [`refresh`] runs after every settings change.

use desqu_shell::module::ModuleStatus;
use desqu_shell::tray::{self, Tray, TrayConfig, TrayItem};
use desqu_shell::updates;
use tauri::{AppHandle, Manager, Wry};

use crate::commands::{self, DesquState};
use crate::registry;

const MODULE_PREFIX: &str = "module:";
const PAUSE_SCREENBOUND: &str = "pause:screenbound";
const RELEASE_SCREENBOUND: &str = "release:screenbound";

pub fn install(app: &AppHandle<Wry>) -> tauri::Result<Tray<Wry>> {
    let mut extra = Vec::new();
    for entry in registry::entries().into_iter().filter(|e| e.descriptor.supported) {
        extra.push(TrayItem::Check {
            id: format!("{MODULE_PREFIX}{}", entry.descriptor.id),
            label: entry.descriptor.name.into(),
            checked: false,
        });
        if entry.descriptor.id == "screenbound" {
            extra.push(TrayItem::Check {
                id: PAUSE_SCREENBOUND.into(),
                label: "Pause Screenbound".into(),
                checked: false,
            });
            extra.push(TrayItem::Normal { id: RELEASE_SCREENBOUND.into(), label: "Release all windows".into() });
        }
    }
    let tray = tray::build(
        app,
        TrayConfig {
            tooltip: "Desqu".into(),
            show_label: "Show Desqu".into(),
            extra_items: extra,
            updates_item: true,
            on_item: Box::new(on_item),
        },
    )?;
    tray::relabel_on_updates(app);
    Ok(tray)
}

/// Brings every check item in line with the current settings and live module status.
pub fn refresh(app: &AppHandle<Wry>) {
    let Some(tray) = app.try_state::<Tray<Wry>>() else {
        return;
    };
    let state = app.state::<DesquState>();
    for entry in registry::entries().into_iter().filter(|e| e.descriptor.supported) {
        let id = entry.descriptor.id;
        tray.set_checked(&format!("{MODULE_PREFIX}{id}"), state.file().get(id).enabled);
        if id == "screenbound" {
            let status = entry.module.status(app);
            let running = matches!(status, ModuleStatus::On | ModuleStatus::Paused);
            tray.set_checked(PAUSE_SCREENBOUND, status == ModuleStatus::Paused);
            tray.set_enabled(PAUSE_SCREENBOUND, running);
            tray.set_enabled(RELEASE_SCREENBOUND, running);
        }
    }
}

fn on_item(app: &AppHandle<Wry>, id: &str) {
    if id == tray::ID_UPDATES {
        updates::on_tray_click(app);
        return;
    }
    // Starting or stopping a module blocks for a moment: never on the tray's event thread.
    let app = app.clone();
    let id = id.to_string();
    std::thread::spawn(move || {
        if let Some(module) = id.strip_prefix(MODULE_PREFIX) {
            let state = app.state::<DesquState>();
            let enable = !state.file().get(module).enabled;
            if let Err(e) = commands::apply_enabled(&app, &state, module, enable) {
                tracing::warn!("tray: could not switch {module}: {e}");
            }
            refresh(&app);
        } else if id == PAUSE_SCREENBOUND {
            let paused = registry::find("screenbound")
                .is_some_and(|e| e.module.status(&app) == ModuleStatus::Paused);
            if let Err(e) = screenbound_module::plugin::set_paused(&app, !paused) {
                tracing::warn!("tray: could not pause Screenbound: {e}");
            }
            refresh(&app);
        } else if id == RELEASE_SCREENBOUND {
            if let Err(e) = screenbound_module::plugin::release_all(&app) {
                tracing::warn!("tray: could not release windows: {e}");
            }
        }
    });
}
