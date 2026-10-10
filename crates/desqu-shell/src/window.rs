//! Window lifecycle: start hidden, start minimized, close to tray.
//!
//! Apps set `"visible": false` on the `main` window in `tauri.conf.json`; this module decides
//! whether to reveal it. Without a working tray the window is always shown, because a hidden
//! window with no tray icon would leave the app unreachable.

use tauri::{AppHandle, Manager, Runtime, WindowEvent};

use crate::args::LaunchArgs;
use crate::settings::ShellSettings;

pub const MAIN_WINDOW: &str = "main";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAction {
    /// Keep the process (and its modules) running with the window hidden.
    HideToTray,
    /// Let the window close; the app exits and modules restore managed windows.
    Close,
}

pub fn should_show_on_startup(launch: &LaunchArgs, settings: &ShellSettings, tray_ok: bool) -> bool {
    let wants_hidden = launch.autostart || settings.start_minimized;
    !(wants_hidden && tray_ok)
}

pub fn close_action(close_to_tray: bool, tray_ok: bool) -> CloseAction {
    if close_to_tray && tray_ok {
        CloseAction::HideToTray
    } else {
        CloseAction::Close
    }
}

pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Call once at the end of `setup`, after the tray was (or failed to be) created.
pub fn on_startup<R: Runtime>(
    app: &AppHandle<R>,
    launch: &LaunchArgs,
    settings: &ShellSettings,
    tray_ok: bool,
) {
    if should_show_on_startup(launch, settings, tray_ok) {
        show_main(app);
    }
}

/// `close_to_tray` is read on every close request, so changing the setting takes effect
/// without a restart.
pub fn install_close_handler<R: Runtime>(
    app: &AppHandle<R>,
    tray_ok: bool,
    close_to_tray: impl Fn() -> bool + Send + 'static,
) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };
    let hidden = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            if close_action(close_to_tray(), tray_ok) == CloseAction::HideToTray {
                api.prevent_close();
                let _ = hidden.hide();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launch(autostart: bool) -> LaunchArgs {
        LaunchArgs { autostart, quit: false }
    }

    fn settings(start_minimized: bool) -> ShellSettings {
        ShellSettings { start_minimized, ..ShellSettings::default() }
    }

    #[test]
    fn manual_launch_shows_the_window() {
        assert!(should_show_on_startup(&launch(false), &settings(false), true));
    }

    #[test]
    fn sign_in_launch_stays_hidden_with_a_tray() {
        assert!(!should_show_on_startup(&launch(true), &settings(false), true));
    }

    #[test]
    fn start_minimized_applies_to_manual_launches_too() {
        assert!(!should_show_on_startup(&launch(false), &settings(true), true));
    }

    #[test]
    fn without_a_tray_the_window_is_always_shown() {
        assert!(should_show_on_startup(&launch(true), &settings(true), false));
    }

    #[test]
    fn close_hides_only_when_enabled_and_tray_works() {
        assert_eq!(close_action(true, true), CloseAction::HideToTray);
        assert_eq!(close_action(false, true), CloseAction::Close);
        assert_eq!(close_action(true, false), CloseAction::Close);
        assert_eq!(close_action(false, false), CloseAction::Close);
    }
}
