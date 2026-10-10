//! Tauri plugin: owns the engine thread and native event subscription, and can start/stop them
//! at runtime (a stopped module has no threads and no hooks).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use desqu_shell::module::{Module, ModuleStatus};
use desqu_shell::mutex::{self, ModuleLock};
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{AppHandle, Emitter, Manager, RunEvent, Runtime};

use crate::core::config::AppConfig;
use crate::engine::{Engine, EngineHandle, EngineMessage, EnginePaths, Notification, Notifier};
use crate::platform::{self, EventSubscription};

/// Must equal the Cargo package name: tauri-plugin derives the permission namespace from it.
pub const PLUGIN_NAME: &str = "screenbound-module";
/// Id of the cross-process lock: only one process may run the Screenbound engine.
pub const MODULE_ID: &str = "screenbound";
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
    module_lock: Mutex<Option<ModuleLock>>,
    owned_elsewhere: AtomicBool,
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
            module_lock: Mutex::new(None),
            owned_elsewhere: AtomicBool::new(false),
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

    pub fn status(&self) -> ModuleStatus {
        match self.with_engine(|e| e.config().enabled) {
            Ok(true) => ModuleStatus::On,
            Ok(false) => ModuleStatus::Paused,
            Err(_) if self.owned_elsewhere.load(Ordering::Relaxed) => ModuleStatus::OwnedElsewhere,
            Err(_) => ModuleStatus::Off,
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
    // Take the cross-process lock first so a standalone app and Desqu never fight over windows.
    let Some(module_lock) = mutex::try_acquire(MODULE_ID)? else {
        state.owned_elsewhere.store(true, Ordering::Relaxed);
        return Err("Screenbound is already running in another process".into());
    };
    state.owned_elsewhere.store(false, Ordering::Relaxed);
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
    *lock(&state.module_lock) = Some(module_lock);
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
    // Dropping the lock lets another process take the module over.
    lock(&state.module_lock).take();
    state.owned_elsewhere.store(false, Ordering::Relaxed);
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
                // Not fatal: another process may own the module, and the UI shows that state.
                if let Err(e) = start(app) {
                    tracing::warn!("Screenbound did not start: {e}");
                }
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

/// Handle the shell uses to drive Screenbound; all state lives in the plugin's `ModuleState`.
pub struct ScreenboundModule;

impl<R: Runtime> Module<R> for ScreenboundModule {
    fn id(&self) -> &'static str {
        MODULE_ID
    }

    fn start(&self, app: &AppHandle<R>) -> Result<(), String> {
        start(app)
    }

    fn stop(&self, app: &AppHandle<R>, timeout: Duration) {
        stop(app, timeout)
    }

    fn status(&self, app: &AppHandle<R>) -> ModuleStatus {
        app.try_state::<ModuleState>()
            .map_or(ModuleStatus::Off, |state| state.status())
    }
}
