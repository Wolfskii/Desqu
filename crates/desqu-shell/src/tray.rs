//! Tray builder shared by every app: "Show <app>", app-specific items, optional "Check for
//! updates", "Exit". Left click shows the main window.

use std::collections::HashMap;

use tauri::menu::{CheckMenuItem, CheckMenuItemBuilder, Menu, MenuItem, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Listener, Manager, Runtime};

use crate::window;

/// Ids the shell handles itself; app items must not reuse them.
pub const ID_SHOW: &str = "show";
pub const ID_UPDATES: &str = "updates";
pub const ID_EXIT: &str = "exit";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayItem {
    Normal { id: String, label: String },
    Check { id: String, label: String, checked: bool },
    Separator,
}

pub type ItemHandler<R> = Box<dyn Fn(&AppHandle<R>, &str) + Send + Sync + 'static>;

pub struct TrayConfig<R: Runtime> {
    pub tooltip: String,
    pub show_label: String,
    pub extra_items: Vec<TrayItem>,
    /// Offer "Check for updates" (hidden until the updater exists in Phase 5).
    pub updates_item: bool,
    /// Called with the id of any item the shell does not handle itself (including `updates`).
    pub on_item: ItemHandler<R>,
}

/// Menu order as data, so it can be tested without a running app.
pub fn layout(
    show_label: &str,
    extra_items: &[TrayItem],
    updates_item: bool,
) -> Vec<TrayItem> {
    let mut items = vec![
        TrayItem::Normal { id: ID_SHOW.into(), label: show_label.into() },
        TrayItem::Separator,
    ];
    if !extra_items.is_empty() {
        items.extend(extra_items.iter().cloned());
        items.push(TrayItem::Separator);
    }
    if updates_item {
        items.push(TrayItem::Normal { id: ID_UPDATES.into(), label: "Check for updates".into() });
    }
    items.push(TrayItem::Normal { id: ID_EXIT.into(), label: "Exit".into() });
    items
}

/// The tray icon plus the one item that changes at runtime.
pub struct Tray<R: Runtime> {
    pub icon: TrayIcon<R>,
    updates: Option<MenuItem<R>>,
    checks: HashMap<String, CheckMenuItem<R>>,
}

impl<R: Runtime> Tray<R> {
    /// Updates a `TrayItem::Check` item; unknown ids are ignored.
    pub fn set_checked(&self, id: &str, checked: bool) {
        if let Some(item) = self.checks.get(id) {
            let _ = item.set_checked(checked);
        }
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) {
        if let Some(item) = self.checks.get(id) {
            let _ = item.set_enabled(enabled);
        }
    }

    /// `Some("1.2.0")` relabels the item "Update to v1.2.0"; `None` restores "Check for updates".
    pub fn set_update_available(&self, version: Option<&str>) {
        if let Some(item) = &self.updates {
            let _ = item.set_text(update_label(version));
        }
    }
}

/// Keeps the tray's updates item in step with update checks ("Update to vX" once one is found).
pub fn relabel_on_updates<R: Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    app.listen(crate::updates::EVENT_UPDATE_AVAILABLE, move |event| {
        let version = serde_json::from_str::<serde_json::Value>(event.payload())
            .ok()
            .and_then(|v| v["version"].as_str().map(str::to_owned));
        if let Some(tray) = handle.try_state::<Tray<R>>() {
            tray.set_update_available(version.as_deref());
        }
    });
}

pub fn update_label(version: Option<&str>) -> String {
    match version {
        Some(v) => format!("Update to v{v}"),
        None => "Check for updates".into(),
    }
}

pub fn build<R: Runtime>(app: &AppHandle<R>, config: TrayConfig<R>) -> tauri::Result<Tray<R>> {
    let menu = Menu::new(app)?;
    let mut updates = None;
    let mut checks = HashMap::new();
    for item in layout(&config.show_label, &config.extra_items, config.updates_item) {
        match item {
            TrayItem::Normal { id, label } => {
                let item = MenuItemBuilder::with_id(id.clone(), label).build(app)?;
                menu.append(&item)?;
                if id == ID_UPDATES {
                    updates = Some(item);
                }
            }
            TrayItem::Check { id, label, checked } => {
                let item = CheckMenuItemBuilder::with_id(id.clone(), label).checked(checked).build(app)?;
                menu.append(&item)?;
                checks.insert(id, item);
            }
            TrayItem::Separator => menu.append(&PredefinedMenuItem::separator(app)?)?,
        }
    }

    let on_item = config.on_item;
    let mut tray = TrayIconBuilder::new()
        .tooltip(config.tooltip)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            ID_SHOW => window::show_main(app),
            // Runs RunEvent::Exit, so modules restore managed windows first.
            ID_EXIT => app.exit(0),
            other => on_item(app, other),
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                window::show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    Ok(Tray { icon: tray.build(app)?, updates, checks })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(items: &[TrayItem]) -> Vec<&str> {
        items
            .iter()
            .map(|i| match i {
                TrayItem::Normal { id, .. } | TrayItem::Check { id, .. } => id.as_str(),
                TrayItem::Separator => "-",
            })
            .collect()
    }

    #[test]
    fn minimal_menu_is_show_separator_exit() {
        assert_eq!(ids(&layout("Show App", &[], false)), ["show", "-", "exit"]);
    }

    #[test]
    fn extras_sit_between_two_separators() {
        let extras = [TrayItem::Check { id: "pause".into(), label: "Pause".into(), checked: false }];
        assert_eq!(
            ids(&layout("Show App", &extras, false)),
            ["show", "-", "pause", "-", "exit"]
        );
    }

    #[test]
    fn updates_item_comes_right_before_exit() {
        assert_eq!(ids(&layout("Show App", &[], true)), ["show", "-", "updates", "exit"]);
    }

    #[test]
    fn update_label_switches_between_check_and_update() {
        assert_eq!(update_label(None), "Check for updates");
        assert_eq!(update_label(Some("1.2.0")), "Update to v1.2.0");
    }

    #[test]
    fn show_label_is_used() {
        let items = layout("Show Screenbound", &[], false);
        assert_eq!(items[0], TrayItem::Normal { id: "show".into(), label: "Show Screenbound".into() });
    }
}
