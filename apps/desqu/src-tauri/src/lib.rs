mod commands;
mod registry;
mod settings;
mod takeover;
mod tray;

use std::sync::Mutex;

use desqu_shell::{args, autostart, instance, logging, module, updates, window};
use screenbound_module::platform;
use tauri::Manager;

use commands::DesquState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::init("desqu_lib=info,screenbound_module=info,vials_module=info,desqu_shell=info,warn");
    platform::init_process();
    let launch = args::parse(std::env::args());

    let app = tauri::Builder::default()
        .plugin(instance::plugin())
        .plugin(autostart::plugin())
        .plugin(updates::plugin())
        .invoke_handler(tauri::generate_handler![
            commands::list_modules,
            commands::set_module_enabled,
            commands::set_module_start_with_system,
            commands::set_all_start_with_system,
            commands::takeover_module,
            commands::reset_module_settings,
            commands::remove_standalone,
        ])
        .setup(move |app| {
            // `--quit` with nothing running: leave before any module starts.
            if instance::exit_if_quit_requested(app.handle(), &launch) {
                return Ok(());
            }

            let config_dir = app.path().app_config_dir()?;
            let roaming = app.path().config_dir()?;
            // Desqu's own startup entry follows its modules, so the shell must not manage it.
            app.handle().plugin(desqu_shell::init_with(
                config_dir.clone(),
                desqu_shell::ShellOptions {
                    manage_autostart: false,
                },
            ))?;

            let modules_path = config_dir.join(settings::MODULES_FILE);
            app.manage(DesquState {
                modules: Mutex::new(settings::load(&modules_path)),
                modules_path,
                roaming_dir: roaming.clone(),
            });

            // Modules are plugins of this process: Desqu hosts them, it does not launch them.
            for entry in registry::entries() {
                app.handle().plugin((entry.init)(&roaming))?;
                module::register_arc(app.handle(), entry.module.clone());
            }

            let state = app.state::<DesquState>();
            let shell = desqu_shell::current_settings(app.handle());
            if let Err(e) =
                autostart::sync_at_startup(app.handle(), settings::should_autostart(&state.file()))
            {
                tracing::warn!("autostart sync on startup failed: {e}");
            }

            // From sign-in only "start with system" modules start; a manual launch starts all that are on.
            let to_start = settings::modules_to_start(&state.file(), launch.autostart);
            for id in to_start {
                let Some(entry) = registry::find(&id) else {
                    continue;
                };
                if let Err(e) = entry.module.start(app.handle()) {
                    // e.g. the standalone app owns it: the UI offers to take over.
                    tracing::warn!("{id} did not start: {e}");
                }
            }

            let tray_ok = match tray::install(app.handle()) {
                Ok(t) => {
                    app.manage(t);
                    true
                }
                Err(e) => {
                    tracing::error!("tray icon failed; closing the window will quit: {e}");
                    false
                }
            };
            tray::refresh(app.handle());

            let handle = app.handle().clone();
            window::install_close_handler(app.handle(), tray_ok, move || {
                desqu_shell::current_settings(&handle).close_to_tray
            });
            window::on_startup(app.handle(), &launch, &shell, tray_ok);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Desqu");

    // Each module restores its managed windows itself on RunEvent::Exit.
    app.run(|_, _| {});
}
