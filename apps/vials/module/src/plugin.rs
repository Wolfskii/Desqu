//! Tauri plugin and the shell's `Module` implementation for Vials.

use std::path::PathBuf;
use std::time::Duration;

use desqu_shell::module::{Module, ModuleStatus};
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{AppHandle, Listener, Manager, RunEvent, Runtime};

use crate::commands;
use crate::engine::{self, ModuleState};

/// Must equal the Cargo package name: tauri-plugin derives the permission namespace from it.
pub const PLUGIN_NAME: &str = "vials-module";

/// Folders are passed in so a standalone app and Desqu read and write the same files.
#[derive(Debug, Clone)]
pub struct ModuleOptions {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub start_immediately: bool,
}

pub fn init<R: Runtime>(options: ModuleOptions) -> TauriPlugin<R> {
    Builder::new(PLUGIN_NAME)
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::set_config,
            commands::list_items,
            commands::item_icon,
            commands::assign_item,
            commands::open_item,
            commands::create_vial_from_menu,
            commands::dismiss_menu,
            commands::menu_ready,
            commands::begin_rename,
            commands::end_rename,
            commands::drop_items,
            commands::drag_item_out,
            commands::show_item_menu,
            commands::copy_items,
            commands::paste_into_vial,
            commands::focus_vial,
            commands::get_monitors,
            commands::desktop_status,
            commands::module_start,
            commands::module_stop,
            commands::module_status,
        ])
        .setup(move |app, _api| {
            app.manage(ModuleState::new(
                options.config_dir.join("config.json"),
                options.data_dir.join("desktop-journal.json"),
            ));
            // `<exe> --action=create-vial` (the desktop right-click entry) arrives here.
            let handle = app.clone();
            app.listen(desqu_shell::instance::ACTION_EVENT, move |event| {
                if event.payload().trim_matches('"') == "create-vial" {
                    let handle = handle.clone();
                    std::thread::spawn(move || {
                        if let Err(e) = engine::create_vial_at_context(&handle) {
                            tracing::debug!("create-vial action ignored: {e}");
                        }
                    });
                }
            });
            if options.start_immediately {
                // Not fatal: another process may own the module, and the UI shows that state.
                if let Err(e) = engine::start(app) {
                    tracing::warn!("Vials did not start: {e}");
                }
            }
            Ok(())
        })
        .on_event(|app, event| {
            if let RunEvent::Exit = event {
                // Restore every hidden desktop item before the process goes away.
                engine::stop(app);
            }
        })
        .build()
}

/// Handle the shell uses to drive Vials; all state lives in the plugin's `ModuleState`.
pub struct VialsModule;

impl<R: Runtime> Module<R> for VialsModule {
    fn id(&self) -> &'static str {
        engine::MODULE_ID
    }

    fn start(&self, app: &AppHandle<R>) -> Result<(), String> {
        engine::start(app)
    }

    fn stop(&self, app: &AppHandle<R>, _timeout: Duration) {
        engine::stop(app)
    }

    fn status(&self, app: &AppHandle<R>) -> ModuleStatus {
        app.try_state::<ModuleState>()
            .map_or(ModuleStatus::Off, |state| commands::status(&state))
    }
}
