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
use crate::legacy;
use crate::migrate;
use crate::bridge::protocol::Product;
use crate::bridge::server::BridgeServer;
use crate::engine::{
    Engine, EngineHandle, EngineMessage, EnginePaths, EngineSink, HostIdentity, Notification, Notifier,
};
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
    pub const BROWSER_CHANGED: &str = "sb://browser-changed";
}

/// Folders are passed in so a standalone app and Desqu read and write the same files.
#[derive(Debug, Clone)]
pub struct ModuleOptions {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    /// Settings folder of the pre-rebrand install (`migrate::legacy_dir`), if any.
    pub legacy_dir: Option<PathBuf>,
    pub start_immediately: bool,
    /// Shown to the browser extension in the handshake.
    pub product: Product,
    pub app_version: String,
}

pub struct ModuleState {
    config_path: PathBuf,
    journal_path: PathBuf,
    legacy_dir: Option<PathBuf>,
    handle: Mutex<Option<EngineHandle>>,
    subscription: Mutex<Option<EventSubscription>>,
    module_lock: Mutex<Option<ModuleLock>>,
    owned_elsewhere: AtomicBool,
    /// Browser bridge: runs only while this process owns the module.
    bridge: Mutex<Option<BridgeServer>>,
    identity: HostIdentity,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl ModuleState {
    fn new(options: &ModuleOptions) -> Self {
        Self {
            config_path: options.config_dir.join("config.json"),
            journal_path: options.data_dir.join("managed-windows.json"),
            legacy_dir: options.legacy_dir.clone(),
            handle: Mutex::new(None),
            subscription: Mutex::new(None),
            module_lock: Mutex::new(None),
            owned_elsewhere: AtomicBool::new(false),
            bridge: Mutex::new(None),
            identity: HostIdentity {
                product: options.product,
                app_version: options.app_version.clone(),
            },
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

    pub fn legacy_dir(&self) -> Option<&std::path::Path> {
        self.legacy_dir.as_deref()
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
    // The old app does not take the lock, so check for it after we own the lock ourselves: a *new*
    // standalone copy would have made us return above. Never fight an older instance.
    if legacy::older_instance_running() {
        return Err(legacy::OLDER_RUNNING_MESSAGE.into());
    }
    tracing::info!(
        config = %state.config_path.display(),
        journal = %state.journal_path.display(),
        "starting Screenbound"
    );

    let emitter = app.clone();
    let notify: Notifier = Arc::new(move |n| {
        let _ = match n {
            Notification::BrowserChanged => emitter.emit(events::BROWSER_CHANGED, ()),
            Notification::Activity(entry) => emitter.emit(events::ACTIVITY, entry),
            Notification::ManagedChanged => emitter.emit(events::MANAGED_CHANGED, ()),
            Notification::MonitorsChanged(m) => emitter.emit(events::MONITORS_CHANGED, m),
            Notification::ConfigChanged(c) => emitter.emit(events::CONFIG_CHANGED, c),
        };
    });

    let paths = EnginePaths {
        config: state.config_path.clone(),
        journal: state.journal_path.clone(),
        legacy_journal: state
            .legacy_dir
            .as_ref()
            .map(|d| d.join(migrate::LEGACY_JOURNAL_FILE)),
        identity: state.identity.clone(),
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

    let bridge_handle = handle.clone();
    *lock(&state.subscription) = subscription;
    *lock(&state.module_lock) = Some(module_lock);
    *handle_slot = Some(handle);
    drop(handle_slot);
    start_bridge(&state, bridge_handle);
    Ok(())
}

/// The native host that ships next to this app.
pub fn host_exe_path() -> Result<std::path::PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let host = exe.with_file_name("desqu-browser-host.exe");
    if host.is_file() {
        Ok(host)
    } else {
        Err("desqu-browser-host.exe is not next to this app".into())
    }
}

/// Writes the browser manifests and registry entries so browsers can start the host.
/// Idempotent: safe on every start.
pub fn register_host() -> Result<std::path::PathBuf, String> {
    use crate::bridge::manifest::{chromium_manifest, chromium_origins, firefox_manifest};
    let host = host_exe_path()?;
    let host_text = host.to_string_lossy().into_owned();
    let chromium = chromium_manifest(&host_text, &chromium_origins());
    let firefox = firefox_manifest(&host_text);
    crate::platform::native_host::install(&chromium, &firefox).map_err(|e| e.to_string())?;
    Ok(host)
}

/// Starts the browser bridge when integration is on. A failure is logged, not fatal: the fullscreen
/// limiter works without browsers.
fn start_bridge(state: &ModuleState, handle: EngineHandle) {
    let enabled = handle
        .call(|e| e.config().browser.integration_enabled)
        .unwrap_or(false);
    if !enabled {
        return;
    }
    let mut slot = lock(&state.bridge);
    if slot.is_some() {
        return;
    }
    if let Err(e) = register_host() {
        tracing::warn!("browser host is not registered: {e}");
    }
    match BridgeServer::start(Arc::new(EngineSink(handle))) {
        Ok(server) => *slot = Some(server),
        Err(e) => tracing::warn!("browser bridge did not start: {e}"),
    }
}

/// Brings the bridge in line with the saved setting: starts it when on, stops it when off.
pub fn sync_bridge(state: &ModuleState) {
    let handle = lock(&state.handle).clone();
    let enabled = handle
        .as_ref()
        .and_then(|h| h.call(|e| e.config().browser.integration_enabled).ok())
        .unwrap_or(false);
    match handle {
        Some(handle) if enabled => start_bridge(state, handle),
        _ => {
            // Off: stop serving, and remove the browser entries so no browser can find us.
            lock(&state.bridge).take();
            if let Err(e) = crate::platform::native_host::uninstall() {
                tracing::warn!("could not remove the browser host entries: {e}");
            }
        }
    }
}

/// Pauses or resumes Screenbound (`enabled = false` in its config) without stopping the engine.
pub fn set_paused<R: Runtime>(app: &AppHandle<R>, paused: bool) -> Result<(), String> {
    let state = app
        .try_state::<ModuleState>()
        .ok_or("Screenbound plugin is not registered")?;
    state.with_engine(move |e| {
        let mut config = e.config();
        config.enabled = !paused;
        e.set_config(config)
    })??;
    Ok(())
}

/// Gives every constrained window back its original size and position.
pub fn release_all<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let state = app
        .try_state::<ModuleState>()
        .ok_or("Screenbound plugin is not registered")?;
    state.with_engine(|e| e.release_all())
}

/// Restores every managed window, then removes the native hooks. A no-op when already off.
pub fn stop<R: Runtime>(app: &AppHandle<R>, timeout: Duration) {
    let Some(state) = app.try_state::<ModuleState>() else {
        return;
    };
    // The bridge goes first: its connections post to the engine.
    lock(&state.bridge).take();
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
            crate::commands::legacy_status,
            crate::commands::legacy_uninstall_old,
            crate::commands::legacy_delete_settings_backup,
            crate::commands::get_browser_status,
            crate::commands::set_browser_integration,
        ])
        .setup(move |app, _api| {
            // Before the engine reads its config, so the first run after the rebrand sees the old settings.
            if let Some(legacy) = &options.legacy_dir {
                let new_config = options.config_dir.join("config.json");
                match migrate::migrate_config(legacy, &new_config) {
                    Ok(true) => tracing::info!("copied settings from {}", legacy.display()),
                    Ok(false) => {}
                    Err(e) => tracing::warn!("settings migration failed: {e}"),
                }
            }
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
