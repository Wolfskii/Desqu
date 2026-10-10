//! Tray builder shared by every app: "Show <app>", app-specific items, optional "Check for
//! updates", "Exit". Left click shows the main window.

use tauri::menu::{CheckMenuItemBuilder, Menu, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Runtime};

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

pub fn build<R: Runtime>(app: &AppHandle<R>, config: TrayConfig<R>) -> tauri::Result<TrayIcon<R>> {
    let menu = Menu::new(app)?;
    for item in layout(&config.show_label, &config.extra_items, config.updates_item) {
        match item {
            TrayItem::Normal { id, label } => {
                menu.append(&MenuItemBuilder::with_id(id, label).build(app)?)?
            }
            TrayItem::Check { id, label, checked } => {
                menu.append(&CheckMenuItemBuilder::with_id(id, label).checked(checked).build(app)?)?
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
    tray.build(app)
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
    fn show_label_is_used() {
        let items = layout("Show Screenbound", &[], false);
        assert_eq!(items[0], TrayItem::Normal { id: "show".into(), label: "Show Screenbound".into() });
    }
}
