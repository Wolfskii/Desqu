use tauri::{AppHandle, Manager};

use desqu_shell::args;
use desqu_shell::tray::{self, Tray, TrayConfig};
use desqu_shell::{autostart, instance, logging, module, updates, window};
use vials_module::{ModuleOptions, VialsModule};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::init("vials_lib=info,vials_module=info,desqu_shell=info,warn");
    let launch = args::parse(std::env::args());

    let app = tauri::Builder::default()
        .plugin(instance::plugin())
        .plugin(autostart::plugin())
        .plugin(updates::plugin())
        .setup(move |app| {
            // `--quit` with nothing running: leave before any module starts.
            if instance::exit_if_quit_requested(app.handle(), &launch) {
                return Ok(());
            }

            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;

            // Plugins are registered here (not on the Builder) because the folders come from the
            // resolved app identifier.
            app.handle().plugin(desqu_shell::init(config_dir.clone()))?;
            app.handle().plugin(vials_module::init(ModuleOptions {
                config_dir,
                data_dir,
                start_immediately: true,
            }))?;
            module::register(app.handle(), VialsModule);

            let shell = desqu_shell::current_settings(app.handle());
            if let Err(e) = autostart::sync_at_startup(app.handle(), shell.start_with_system) {
                tracing::warn!("autostart sync on startup failed: {e}");
            }

            // Closing the window hides it; the vials stay on the desktop until Exit.
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
            // `--action=...` from a context menu entry that had to start the app.
            instance::deliver_launch_action(app.handle(), &launch);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Vials");

    // The module puts every hidden desktop item back itself on RunEvent::Exit.
    app.run(|_, _| {});
}

fn install_tray(app: &AppHandle) -> tauri::Result<Tray<tauri::Wry>> {
    tray::build(
        app,
        TrayConfig {
            tooltip: "Vials".into(),
            show_label: "Show Vials".into(),
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
