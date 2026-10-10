//! Tauri plugin: owns the engine thread and native event subscription, and can start/stop them
//! at runtime (a stopped module has no threads and no hooks).

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tauri::plugin::{Builder, TauriPlugin};
use tauri::{AppHandle, Emitter, Manager, RunEvent, Runtime};

use crate::core::config::AppConfig;
use crate::engine::{Engine, EngineHandle, EngineMessage, EnginePaths, Notification, Notifier};
use crate::platform::{self, EventSubscription};

/// Must equal the Cargo package name: tauri-plugin derives the permission namespace from it.
pub const PLUGIN_NAME: &str = "screenbound-module";
pub const STOP_TIMEOUT: Duration = Duration::from_secs(3);

pub mod events {
    pub const ACTIVITY: &str = "sb://activity";
    pub const MANAGED_CHANGED: &str = "sb://managed-changed";
    pub const MONITORS_CHANGED: &str = "sb://monitors-changed";
    pub const CONFIG_CHANGED: &str = "sb://config-changed";
}

/// Folders are passed in so a standalone app and Desqu read and write the same files.
#[derive(Debug, Clone)]
pub struct ModuleOptions {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub start_immediately: bool,
}

pub struct ModuleState {
    config_path: PathBuf,
    journal_path: PathBuf,
    handle: Mutex<Option<EngineHandle>>,
    subscription: Mutex<Option<EventSubscription>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl ModuleState {
    fn new(options: &ModuleOptions) -> Self {
        Self {
            config_path: options.config_dir.join("config.json"),
            journal_path: options.data_dir.join("managed-windows.json"),
            handle: Mutex::new(None),
            subscription: Mutex::new(None),
        }
    }

    /// Runs `f` on the engine thread, or fails when the module is off.
    pub fn with_engine<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Engine) -> R + Send + 'static,
    ) -> Result<R, String> {
        // Clone out of the lock so a slow engine call never blocks start/stop.
        let handle = lock(&self.handle).clone();
        match handle {
            Some(h) => h.call(f),
            None => Err("Screenbound is off".into()),
        }
    }

    pub fn status(&self) -> &'static str {
        match self.with_engine(|e| e.config().enabled) {
            Ok(true) => "on",
            Ok(false) => "paused",
            Err(_) => "off",
        }
    }
}

/// Current config, or `None` while the module is off.
pub fn current_config<R: Runtime>(app: &AppHandle<R>) -> Option<AppConfig> {
    app.try_state::<ModuleState>()?.with_engine(|e| e.config()).ok()
}

pub fn start<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let state = app
        .try_state::<ModuleState>()
        .ok_or("Screenbound plugin is not registered")?;
    let mut handle_slot = lock(&state.handle);
    if handle_slot.is_some() {
        return Ok(());
    }
    tracing::info!(
        config = %state.config_path.display(),
        journal = %state.journal_path.display(),
        "starting ScreenBound"
    );

    let emitter = app.clone();
    let notify: Notifier = Arc::new(move |n| {
        let _ = match n {
            Notification::Activity(entry) => emitter.emit(events::ACTIVITY, entry),
            Notification::ManagedChanged => emitter.emit(events::MANAGED_CHANGED, ()),
            Notification::MonitorsChanged(m) => emitter.emit(events::MONITORS_CHANGED, m),
            Notification::ConfigChanged(c) => emitter.emit(events::CONFIG_CHANGED, c),
        };
    });

    let paths = EnginePaths {
        config: state.config_path.clone(),
        journal: state.journal_path.clone(),
    };
    let platform = platform::create();
    let handle = EngineHandle::spawn(Engine::new(platform.clone(), paths, notify))
        .map_err(|e| e.to_string())?;

    let engine_tx = handle.sender();
    let subscription = match platform.subscribe(Box::new(move |ev| {
        let _ = engine_tx.send(EngineMessage::Platform(ev));
    })) {
        Ok(s) => {
            let _ = handle.call(|e| e.set_events_active(true));
            Some(s)
        }
        Err(e) => {
            tracing::error!("native event subscription failed: {e}");
            None
        }
    };

    *lock(&state.subscription) = subscription;
    *handle_slot = Some(handle);
    Ok(())
}

/// Restores every managed window, then removes the native hooks. A no-op when already off.
pub fn stop<R: Runtime>(app: &AppHandle<R>, timeout: Duration) {
    let Some(state) = app.try_state::<ModuleState>() else {
        return;
    };
    let handle = lock(&state.handle).take();
    if let Some(handle) = handle {
        handle.stop(timeout);
    }
    lock(&state.subscription).take();
}

pub fn init<R: Runtime>(options: ModuleOptions) -> TauriPlugin<R> {
    Builder::new(PLUGIN_NAME)
        .invoke_handler(tauri::generate_handler![
            crate::commands::get_monitors,
            crate::commands::get_config,
            crate::commands::set_config,
            crate::commands::set_enabled,
            crate::commands::get_status,
            crate::commands::list_windows,
            crate::commands::get_activity,
            crate::commands::release_window,
            crate::commands::release_all,
            crate::commands::module_start,
            crate::commands::module_stop,
            crate::commands::module_status,
        ])
        .setup(move |app, _api| {
            app.manage(ModuleState::new(&options));
            if options.start_immediately {
                start(app)?;
            }
            Ok(())
        })
        .on_event(|app, event| {
            if let RunEvent::Exit = event {
                // Restore every managed window before the process goes away.
                stop(app, STOP_TIMEOUT);
            }
        })
        .build()
}
