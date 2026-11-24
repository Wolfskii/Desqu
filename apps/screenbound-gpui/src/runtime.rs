//! Owns the Screenbound engine thread and the native event subscription (what the Tauri plugin does,
//! without Tauri), and keeps a snapshot of the engine's state that the window can read without ever
//! waiting for the engine: a background thread refreshes it when something changed and every couple
//! of seconds for the list of open windows.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use screenbound_module::bridge::protocol::Product;
use screenbound_module::bridge::server::BridgeServer;
use screenbound_module::core::config::AppConfig;
use screenbound_module::core::monitor::MonitorInfo;
use screenbound_module::core::window::WindowInfo;
use screenbound_module::engine::activity::ActivityEntry;
use screenbound_module::engine::{
    Engine, EngineHandle, EngineMessage, EnginePaths, EngineSink, EngineStatus, HostIdentity, Notification, Notifier,
};
use screenbound_module::platform::{self, EventSubscription};

/// What the window shows. Replaced as a whole, so reading it never sees half an update.
#[derive(Clone, Default)]
pub struct Snapshot {
    pub config: Option<AppConfig>,
    pub monitors: Vec<MonitorInfo>,
    pub windows: Vec<WindowInfo>,
    pub status: Option<EngineStatus>,
    pub activity: Vec<ActivityEntry>,
}

/// The parts the refresher thread shares with the app.
struct Shared {
    handle: EngineHandle,
    snapshot: Mutex<Arc<Snapshot>>,
    /// Bumped on every snapshot: the window redraws when it changes.
    generation: AtomicU64,
    wake: (Mutex<bool>, std::sync::Condvar),
    stopping: AtomicBool,
    /// The browser bridge (the pipe the extension talks to), while integration is on.
    bridge: Mutex<Option<BridgeServer>>,
    /// When a window last read the snapshot (milliseconds since `epoch`): the list of open windows is
    /// only worth its cost while someone looks at it.
    viewed: AtomicU64,
    epoch: std::time::Instant,
    /// The last list of open windows, and when it was read.
    windows: Mutex<(Option<std::time::Instant>, Vec<WindowInfo>)>,
}

/// The list of windows is read while a window looked at the snapshot this recently.
const VIEWED_WITHIN: Duration = Duration::from_secs(6);
/// How often the list of windows is read while someone looks.
const WINDOWS_EVERY: Duration = Duration::from_secs(2);
/// Snapshots are made at most this often, so a burst of engine notifications (every step of an
/// animation) does not keep the engine thread busy answering them.
const SNAPSHOT_GAP: Duration = Duration::from_millis(300);

/// What the views need from Screenbound: the engine inside the app itself ([`Runtime`]), or, in the
/// Desqu window, the saved config plus a message to the running app.
pub trait Backend {
    fn snapshot(&self) -> Arc<Snapshot>;
    /// Bumped whenever the snapshot changed: the views redraw then.
    fn generation(&self) -> u64;
    fn change_boxed(&self, edit: Box<dyn FnOnce(&mut AppConfig) -> bool + Send>);
    fn release_all(&self);
    fn sync_browser(&self);
}

impl dyn Backend {
    /// Changes the config (never blocks the caller).
    pub fn change(&self, edit: impl FnOnce(&mut AppConfig) -> bool + Send + 'static) {
        self.change_boxed(Box::new(edit));
    }
}

impl Backend for Runtime {
    fn snapshot(&self) -> Arc<Snapshot> {
        Runtime::snapshot(self)
    }
    fn generation(&self) -> u64 {
        Runtime::generation(self)
    }
    fn change_boxed(&self, edit: Box<dyn FnOnce(&mut AppConfig) -> bool + Send>) {
        Runtime::change(self, edit);
    }
    fn release_all(&self) {
        Runtime::release_all(self);
    }
    fn sync_browser(&self) {
        Runtime::sync_browser(self);
    }
}

pub struct Runtime {
    shared: Arc<Shared>,
    _subscription: Option<EventSubscription>,
    refresher: Mutex<Option<JoinHandle<()>>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The native host that ships next to this app (it relays the browser extension to the bridge pipe).
fn host_exe_path() -> Result<std::path::PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let host = exe.with_file_name("desqu-browser-host.exe");
    if host.is_file() {
        Ok(host)
    } else {
        Err("desqu-browser-host.exe is not next to this app".into())
    }
}

/// Writes the browser manifests and registry entries so browsers can start the host.
fn register_host() -> Result<(), String> {
    use screenbound_module::bridge::manifest::{chromium_manifest, chromium_origins, firefox_manifest};
    let host = host_exe_path()?.to_string_lossy().into_owned();
    platform::native_host::install(&chromium_manifest(&host, &chromium_origins()), &firefox_manifest(&host)).map(|_| ()).map_err(|e| e.to_string())
}

/// Whether the registered browser host is the one next to this app (the Tauri Screenbound registers the
/// same entries, so what belongs to another install is left alone).
fn host_is_ours() -> bool {
    let Some(local) = std::env::var_os("LOCALAPPDATA") else { return false };
    let manifest = std::path::Path::new(&local).join("Desqu").join("browser-host").join("chromium.json");
    let Ok(text) = std::fs::read_to_string(manifest) else { return false };
    let registered = serde_json::from_str::<serde_json::Value>(&text).ok().and_then(|v| v.get("path").and_then(|p| p.as_str().map(str::to_string)));
    let ours = host_exe_path().ok();
    match (registered, ours) {
        (Some(r), Some(o)) => r.eq_ignore_ascii_case(&o.to_string_lossy()),
        _ => false,
    }
}

/// Removes the browser entries (what "Browser integration: off" and uninstalling do), if they are ours.
pub fn unregister_host() -> Result<(), String> {
    if !host_is_ours() {
        return Ok(());
    }
    platform::native_host::uninstall().map_err(|e| e.to_string())
}

impl Runtime {
    /// Starts the engine with its config and journal in `dir`.
    /// `legacy` is the pre-rebrand settings folder: its config is copied once if there is none yet, and
    /// its journal is read to put windows back that an old crash left constrained.
    pub fn start(dir: &Path, version: &str, legacy: Option<&Path>) -> Result<Arc<Self>, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        if let Some(legacy) = legacy {
            match screenbound_module::migrate::migrate_config(legacy, &dir.join("config.json")) {
                Ok(true) => eprintln!("copied settings from {}", legacy.display()),
                Ok(false) => {}
                Err(e) => eprintln!("settings migration failed: {e}"),
            }
        }
        // The notifier needs the shared state, which needs the engine handle, which needs the notifier:
        // a cell breaks the circle.
        let cell: Arc<std::sync::OnceLock<Arc<Shared>>> = Arc::new(std::sync::OnceLock::new());
        let notify_cell = cell.clone();
        let notify: Notifier = Arc::new(move |_: Notification| {
            if let Some(shared) = notify_cell.get() {
                shared.poke();
            }
        });
        let paths = EnginePaths {
            config: dir.join("config.json"),
            journal: dir.join("managed-windows.json"),
            legacy_journal: legacy.map(|d| d.join(screenbound_module::migrate::LEGACY_JOURNAL_FILE)),
            identity: HostIdentity { product: Product::Screenbound, app_version: version.to_string() },
        };
        let platform = platform::create();
        let handle = EngineHandle::spawn(Engine::new(platform.clone(), paths, notify)).map_err(|e| e.to_string())?;
        let tx = handle.sender();
        let subscription = match platform.subscribe(Box::new(move |ev| {
            let _ = tx.send(EngineMessage::Platform(ev));
        })) {
            Ok(s) => {
                let _ = handle.call(|e| e.set_events_active(true));
                Some(s)
            }
            Err(e) => {
                eprintln!("native event subscription failed: {e}");
                None
            }
        };
        let shared = Arc::new(Shared {
            handle,
            snapshot: Mutex::new(Arc::new(Snapshot::default())),
            generation: AtomicU64::new(0),
            wake: (Mutex::new(false), std::sync::Condvar::new()),
            stopping: AtomicBool::new(false),
            bridge: Mutex::new(None),
            viewed: AtomicU64::new(0),
            epoch: std::time::Instant::now(),
            windows: Mutex::new((None, Vec::new())),
        });
        let _ = cell.set(shared.clone());
        shared.refresh();
        shared.sync_bridge();
        let worker = shared.clone();
        let join = std::thread::Builder::new().name("screenbound-snapshot".into()).spawn(move || worker.refresh_loop()).map_err(|e| e.to_string())?;
        #[allow(clippy::arc_with_non_send_sync)] // only used on the UI thread
        Ok(Arc::new(Self { shared, _subscription: subscription, refresher: Mutex::new(Some(join)) }))
    }

    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.shared.mark_viewed();
        lock(&self.shared.snapshot).clone()
    }

    pub fn generation(&self) -> u64 {
        self.shared.generation.load(Ordering::Relaxed)
    }

    /// Changes the config on the engine thread (never blocks the caller) and refreshes the snapshot.
    pub fn change(&self, edit: impl FnOnce(&mut AppConfig) -> bool + Send + 'static) {
        self.shared.handle.post(move |e| {
            let mut config = e.config();
            if edit(&mut config) {
                if let Err(err) = e.set_config(config) {
                    eprintln!("could not apply the settings: {err}");
                }
            }
        });
        self.shared.poke();
    }

    /// After the browser integration setting changed: starts or stops the bridge to match (off the UI thread).
    pub fn sync_browser(&self) {
        let shared = self.shared.clone();
        std::thread::spawn(move || shared.sync_bridge());
    }

    /// Gives every constrained window its original size and position back.
    pub fn release_all(&self) {
        self.shared.handle.post(|e| e.release_all());
        self.shared.poke();
    }

    /// Restores every managed window and ends the engine.
    pub fn stop(&self) {
        self.shared.stopping.store(true, Ordering::Relaxed);
        self.shared.poke();
        if let Some(join) = lock(&self.refresher).take() {
            let _ = join.join();
        }
        drop(lock(&self.shared.bridge).take());
        self.shared.handle.stop(Duration::from_secs(3));
    }
}

impl Shared {
    /// Brings the browser bridge in line with the saved setting: serves the pipe and registers the host
    /// while integration is on; stops and unregisters when it is off.
    fn sync_bridge(&self) {
        let on = self.handle.call(|e| e.config().browser.integration_enabled).unwrap_or(false);
        let mut slot = lock(&self.bridge);
        if on {
            if slot.is_none() {
                if let Err(e) = register_host() {
                    eprintln!("browser host is not registered: {e}");
                }
                match BridgeServer::start(Arc::new(EngineSink(self.handle.clone()))) {
                    Ok(server) => *slot = Some(server),
                    Err(e) => eprintln!("browser bridge did not start: {e}"),
                }
            }
        } else if slot.take().is_some() {
            if let Err(e) = unregister_host() {
                eprintln!("could not remove the browser host entries: {e}");
            }
        }
    }

    /// Notes that a window is looking at the snapshot. After a pause it also asks for a fresh one, so
    /// the list of windows is there right away.
    fn mark_viewed(&self) {
        let now = self.epoch.elapsed().as_millis() as u64 + 1;
        let before = self.viewed.swap(now, Ordering::Relaxed);
        if before == 0 || now - before > VIEWED_WITHIN.as_millis() as u64 {
            self.poke();
        }
    }

    fn being_viewed(&self) -> bool {
        let at = self.viewed.load(Ordering::Relaxed);
        at != 0 && (self.epoch.elapsed().as_millis() as u64 + 1).saturating_sub(at) <= VIEWED_WITHIN.as_millis() as u64
    }

    fn poke(&self) {
        *lock(&self.wake.0) = true;
        self.wake.1.notify_one();
    }

    fn refresh_loop(&self) {
        while !self.stopping.load(Ordering::Relaxed) {
            self.refresh();
            // At most a few snapshots a second, whatever the engine reports.
            std::thread::sleep(SNAPSHOT_GAP);
            let (flag, cv) = &self.wake;
            let mut woken = lock(flag);
            if !*woken {
                // The list of open windows changes without the engine noticing: look every 2 s.
                woken = cv.wait_timeout(woken, Duration::from_secs(2)).unwrap_or_else(|e| e.into_inner()).0;
            }
            *woken = false;
        }
    }

    /// Reads the engine's state into a new snapshot.
    pub fn refresh(&self) {
        // Listing the open windows asks Windows about every one of them, on the engine thread, which
        // also moves the windows: only do it while a window shows the list, and not on every pass.
        let list_windows = self.being_viewed() && lock(&self.windows).0.is_none_or(|at| at.elapsed() >= WINDOWS_EVERY);
        let read = self.handle.call(move |e| {
            let windows = if list_windows { e.list_windows().ok() } else { None };
            (e.config(), e.monitors(), windows, e.status(), e.activity())
        });
        if let Ok((config, monitors, windows, status, activity)) = read {
            let windows = {
                let mut last = lock(&self.windows);
                if let Some(fresh) = windows {
                    *last = (Some(std::time::Instant::now()), fresh);
                }
                last.1.clone()
            };
            *lock(&self.snapshot) = Arc::new(Snapshot { config: Some(config), monitors, windows, status: Some(status), activity });
            self.generation.fetch_add(1, Ordering::Relaxed);
        }
    }
}
