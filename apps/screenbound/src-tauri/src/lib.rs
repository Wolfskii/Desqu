use tauri::{AppHandle, Manager};

use desqu_shell::args;
use desqu_shell::settings;
use desqu_shell::tray::{self, Tray, TrayConfig};
use desqu_shell::{autostart, instance, logging, module, updates, window};
use screenbound_module::core::config::AppConfig;
use screenbound_module::{migrate, platform, ModuleOptions};

/// Starts Desqu (found through its uninstall entry). Without one, the window just hides:
/// the user still has the tray icon and Desqu is controlling their windows anyway.
#[tauri::command]
fn open_desqu(app: AppHandle) -> Result<(), String> {
    let exe = desqu_shell::platform::read_uninstall_entry("Desqu")
        .and_then(|u| u.install_location)
        .map(|dir| std::path::PathBuf::from(dir.trim().trim_matches('"')).join("desqu.exe"))
        .filter(|path| path.exists());
    match exe {
        // A second launch of a running Desqu just shows its window (single instance).
        Some(path) => std::process::Command::new(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("could not start Desqu: {e}")),
        None => {
            if let Some(window) = app.get_webview_window(window::MAIN_WINDOW) {
                let _ = window.hide();
            }
            Ok(())
        }
    }
}

#[tauri::command]
fn exit_app(app: AppHandle) {
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::init("screenbound_lib=info,screenbound_module=info,desqu_shell=info,warn");
    platform::init_process();
    let launch = args::parse(std::env::args());

    let app = tauri::Builder::default()
        .plugin(instance::plugin())
        .plugin(autostart::plugin())
        .plugin(updates::plugin())
        .invoke_handler(tauri::generate_handler![open_desqu, exit_app])
        .setup(move |app| {
            // `--quit` with nothing running: leave before any module starts.
            if instance::exit_if_quit_requested(app.handle(), &launch) {
                return Ok(());
            }

            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            let legacy_dir = migrate::legacy_dir(&app.path().config_dir()?);
            // Copy the old settings first so the startAtBoot hand-over below can see them.
            if let Err(e) = migrate::migrate_config(&legacy_dir, &config_dir.join("config.json")) {
                tracing::warn!("settings migration failed: {e}");
            }
            migrate_legacy_start_at_boot(&config_dir);

            // Plugins are registered here (not on the Builder) because the folders come from
            // the resolved app identifier.
            app.handle().plugin(desqu_shell::init(config_dir.clone()))?;
            app.handle().plugin(screenbound_module::init(ModuleOptions {
                config_dir,
                data_dir,
                legacy_dir: Some(legacy_dir),
                start_immediately: true,
            }))?;
            module::register(app.handle(), screenbound_module::ScreenboundModule);

            let shell = desqu_shell::current_settings(app.handle());
            if let Err(e) = autostart::sync_at_startup(app.handle(), shell.start_with_system) {
                tracing::warn!("autostart sync on startup failed: {e}");
            }

            // Closing the window hides it; the process keeps managing windows until Exit.
            let tray_ok = match install_tray(app.handle()) {
                Ok(tray) => {
                    app.manage(tray);
                    true
                }
                Err(e) => {
                    tracing::error!("tray icon failed; closing the window will quit: {e}");
                    false
                }
            };
            tray::relabel_on_updates(app.handle());
            let handle = app.handle().clone();
            window::install_close_handler(app.handle(), tray_ok, move || {
                desqu_shell::current_settings(&handle).close_to_tray
            });
            window::on_startup(app.handle(), &launch, &shell, tray_ok);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Screenbound");

    // The module restores managed windows itself on RunEvent::Exit.
    app.run(|_, _| {});
}

/// Config v1 kept "start at boot" inside config.json; it now lives in the shell settings.
/// Runs before the engine and the shell load their files. Failures only cost the old value.
fn migrate_legacy_start_at_boot(config_dir: &std::path::Path) {
    let legacy = match AppConfig::migrate_legacy_start_at_boot(&config_dir.join("config.json")) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("config migration skipped: {e}");
            return;
        }
    };
    let shell_path = config_dir.join(desqu_shell::plugin::SETTINGS_FILE);
    if let Err(e) = settings::adopt_legacy_start_with_system(&shell_path, legacy) {
        tracing::warn!("could not carry startAtBoot over to the shell settings: {e}");
    }
}

fn install_tray(app: &AppHandle) -> tauri::Result<Tray<tauri::Wry>> {
    tray::build(
        app,
        TrayConfig {
            tooltip: "Screenbound".into(),
            show_label: "Show Screenbound".into(),
            extra_items: Vec::new(),
            updates_item: true,
            on_item: Box::new(|app, id| {
                if id == tray::ID_UPDATES {
                    updates::on_tray_click(app);
                }
            }),
        },
    )
}
