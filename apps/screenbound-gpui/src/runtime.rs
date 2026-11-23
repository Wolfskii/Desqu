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
use screenbound_module::core::config::AppConfig;
use screenbound_module::core::monitor::MonitorInfo;
use screenbound_module::core::window::WindowInfo;
use screenbound_module::engine::activity::ActivityEntry;
use screenbound_module::engine::{
    Engine, EngineHandle, EngineMessage, EnginePaths, EngineStatus, HostIdentity, Notification, Notifier,
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
}

pub struct Runtime {
    shared: Arc<Shared>,
    _subscription: Option<EventSubscription>,
    refresher: Mutex<Option<JoinHandle<()>>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Runtime {
    /// Starts the engine with its config and journal in `dir`.
    pub fn start(dir: &Path, version: &str) -> Result<Arc<Self>, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
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
            legacy_journal: None,
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
        });
        let _ = cell.set(shared.clone());
        shared.refresh();
        let worker = shared.clone();
        let join = std::thread::Builder::new().name("screenbound-snapshot".into()).spawn(move || worker.refresh_loop()).map_err(|e| e.to_string())?;
        #[allow(clippy::arc_with_non_send_sync)] // only used on the UI thread
        Ok(Arc::new(Self { shared, _subscription: subscription, refresher: Mutex::new(Some(join)) }))
    }

    pub fn snapshot(&self) -> Arc<Snapshot> {
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
        self.shared.handle.stop(Duration::from_secs(3));
    }
}

impl Shared {
    fn poke(&self) {
        *lock(&self.wake.0) = true;
        self.wake.1.notify_one();
    }

    fn refresh_loop(&self) {
        while !self.stopping.load(Ordering::Relaxed) {
            self.refresh();
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
        let read = self.handle.call(|e| {
            let windows = e.list_windows().unwrap_or_default();
            (e.config(), e.monitors(), windows, e.status(), e.activity())
        });
        if let Ok((config, monitors, windows, status, activity)) = read {
            *lock(&self.snapshot) = Arc::new(Snapshot { config: Some(config), monitors, windows, status: Some(status), activity });
            self.generation.fetch_add(1, Ordering::Relaxed);
        }
    }
}
