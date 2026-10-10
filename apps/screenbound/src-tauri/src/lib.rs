use tauri::{AppHandle, Manager};

use desqu_shell::args;
use desqu_shell::settings;
use desqu_shell::tray::{self, TrayConfig};
use desqu_shell::{autostart, instance, logging, window};
use screenbound_module::core::config::AppConfig;
use screenbound_module::{platform, ModuleOptions};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::init("screenbound_lib=info,screenbound_module=info,desqu_shell=info,warn");
    platform::init_process();
    let launch = args::parse(std::env::args());

    let app = tauri::Builder::default()
        .plugin(instance::plugin())
        .plugin(autostart::plugin())
        .setup(move |app| {
            // `--quit` with nothing running: leave before any module starts.
            if instance::exit_if_quit_requested(app.handle(), &launch) {
                return Ok(());
            }

            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            migrate_legacy_start_at_boot(&config_dir);

            // Plugins are registered here (not on the Builder) because the folders come from
            // the resolved app identifier.
            app.handle().plugin(desqu_shell::init(config_dir.clone()))?;
            app.handle().plugin(screenbound_module::init(ModuleOptions {
                config_dir,
                data_dir,
                start_immediately: true,
            }))?;

            let shell = desqu_shell::current_settings(app.handle());
            if let Err(e) = autostart::sync_at_startup(app.handle(), shell.start_with_system) {
                tracing::warn!("autostart sync on startup failed: {e}");
            }

            // Closing the window hides it; the process keeps managing windows until Exit.
            let tray_ok = match install_tray(app.handle()) {
                Ok(icon) => {
                    app.manage(icon);
                    true
                }
                Err(e) => {
                    tracing::error!("tray icon failed; closing the window will quit: {e}");
                    false
                }
            };
            let handle = app.handle().clone();
            window::install_close_handler(app.handle(), tray_ok, move || {
                desqu_shell::current_settings(&handle).close_to_tray
            });
            window::on_startup(app.handle(), &launch, &shell, tray_ok);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building ScreenBound");

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

fn install_tray(app: &AppHandle) -> tauri::Result<tauri::tray::TrayIcon> {
    tray::build(
        app,
        TrayConfig {
            tooltip: "ScreenBound".into(),
            show_label: "Show ScreenBound".into(),
            extra_items: Vec::new(),
            updates_item: false,
            on_item: Box::new(|_, _| {}),
        },
    )
}
