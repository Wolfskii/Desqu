mod autostart;

use tauri::menu::{Menu, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Listener, Manager, WindowEvent};

use screenbound_module::core::config::AppConfig;
use screenbound_module::plugin::events;
use screenbound_module::{platform, ModuleOptions};

fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new("screenbound_lib=info,screenbound_module=info,warn")
    });
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();
    platform::init_process();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Two instances would fight over the same windows; show the one already running.
            show_main(app);
        }))
        .setup(|app| {
            // The module is registered here (not on the Builder) because the folders come from
            // the resolved app identifier.
            app.handle().plugin(screenbound_module::init(ModuleOptions {
                config_dir: app.path().app_config_dir()?,
                data_dir: app.path().app_data_dir()?,
                start_immediately: true,
            }))?;

            if let Some(config) = screenbound_module::current_config(app.handle()) {
                if let Err(e) = autostart::sync(app.handle(), config.start_at_boot) {
                    tracing::warn!("autostart sync on startup failed: {e}");
                }
            }
            // Keep the Run entry aligned whenever the UI saves settings.
            let handle = app.handle().clone();
            app.listen(events::CONFIG_CHANGED, move |event| {
                match serde_json::from_str::<AppConfig>(event.payload()) {
                    Ok(config) => {
                        if let Err(e) = autostart::sync(&handle, config.start_at_boot) {
                            tracing::warn!("autostart sync failed: {e}");
                        }
                    }
                    Err(e) => tracing::warn!("unreadable config-changed payload: {e}"),
                }
            });

            // Closing the window hides it. The process keeps managing windows until Exit.
            match install_tray(app.handle()) {
                Ok(icon) => {
                    app.manage(icon);
                    if let Some(window) = app.get_webview_window("main") {
                        let hidden = window.clone();
                        window.on_window_event(move |event| {
                            if let WindowEvent::CloseRequested { api, .. } = event {
                                api.prevent_close();
                                let _ = hidden.hide();
                            }
                        });
                    }
                }
                Err(e) => tracing::error!("tray icon failed; closing the window will quit: {e}"),
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building ScreenBound");

    // The module restores managed windows itself on RunEvent::Exit.
    app.run(|_, _| {});
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn install_tray(app: &AppHandle) -> tauri::Result<tauri::tray::TrayIcon> {
    let show = MenuItemBuilder::with_id("show", "Show ScreenBound").build(app)?;
    let exit = MenuItemBuilder::with_id("exit", "Exit").build(app)?;
    let menu = Menu::with_items(app, &[&show, &PredefinedMenuItem::separator(app)?, &exit])?;
    let mut tray = TrayIconBuilder::new()
        .tooltip("ScreenBound")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "exit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)
}
