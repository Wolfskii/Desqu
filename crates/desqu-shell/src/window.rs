//! Window lifecycle: start hidden, start minimized, close to tray (rest lands in P3.6).

use tauri::{AppHandle, Manager, Runtime};

pub const MAIN_WINDOW: &str = "main";

pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
