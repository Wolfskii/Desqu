//! Screenbound as the Desqu window sees it. The views are Screenbound's own; what is behind them is
//! different here: the settings come from Screenbound's config file, the monitors and open windows
//! are read from Windows directly, and a change goes to the running Screenbound (which applies it and
//! saves it) or, when it is off, straight into the file.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use desqu_native_shell::tray;
use screenbound_gpui::runtime::{Backend, Snapshot};
use screenbound_module::core::config::AppConfig;
use screenbound_module::core::window::WindowInfo;
use screenbound_module::platform::{self, PlatformWindowManager};

use crate::modules::Kind;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn config_path() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default().join("io.github.wolfskii.screenbound").join("config.json")
}

pub struct Remote {
    snapshot: Mutex<Arc<Snapshot>>,
    generation: AtomicU64,
    stop: AtomicBool,
    path: PathBuf,
}

impl Remote {
    /// Starts looking at Windows every couple of seconds (and at once).
    pub fn start() -> Arc<Self> {
        let remote = Arc::new(Self { snapshot: Mutex::new(Arc::default()), generation: AtomicU64::new(0), stop: AtomicBool::new(false), path: config_path() });
        remote.refresh(&platform::create());
        let worker = remote.clone();
        let _ = std::thread::Builder::new().name("screenbound-remote".into()).spawn(move || {
            platform::init_thread();
            let manager = platform::create();
            while !worker.stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_secs(2));
                worker.refresh(&manager);
            }
        });
        remote
    }

    fn refresh(&self, manager: &Arc<dyn PlatformWindowManager>) {
        let monitors = manager.monitors().unwrap_or_default();
        let windows: Vec<WindowInfo> = manager
            .enumerate_windows()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|id| {
                let identity = manager.identity(id).ok()?;
                let state = manager.state(id).ok()?;
                let monitor_name = state.monitor.as_ref().and_then(|mid| monitors.iter().find(|m| &m.id == mid)).map(|m| m.friendly_name.clone());
                Some(WindowInfo { id, id_hex: id.to_string(), identity, state, monitor_name, fullscreen_like: false, hung: manager.is_hung(id), managed: false, matched_rule: None })
            })
            .collect();
        let config = AppConfig::load(&self.path).ok();
        *lock(&self.snapshot) = Arc::new(Snapshot { config, monitors, windows, status: None, activity: Vec::new() });
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    /// Ends the thread that looks at Windows.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Backend for Remote {
    fn snapshot(&self) -> Arc<Snapshot> {
        lock(&self.snapshot).clone()
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    fn change_boxed(&self, edit: Box<dyn FnOnce(&mut AppConfig) -> bool + Send>) {
        let mut config = match AppConfig::load(&self.path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("could not read the Screenbound settings: {e}");
                return;
            }
        };
        if !edit(&mut config) {
            return;
        }
        // A running Screenbound applies the whole new config (and saves it); otherwise it is just saved.
        let pending = self.path.with_file_name("from-desqu.json");
        let delivered = config.save(&pending).is_ok() && tray::forward_action(&Kind::Screenbound.identity(), &format!("replace-config:{}", pending.display()));
        if !delivered {
            let _ = std::fs::remove_file(&pending);
            if let Err(e) = config.save(&self.path) {
                eprintln!("could not save the Screenbound settings: {e}");
            }
        }
        // Shown at once as it is now; the pass every couple of seconds reads what the app saved.
        let mut shown = lock(&self.snapshot);
        let mut next = (**shown).clone();
        next.config = Some(config);
        *shown = Arc::new(next);
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    fn release_all(&self) {
        let _ = tray::forward_action(&Kind::Screenbound.identity(), "release");
    }

    fn sync_browser(&self) {
        let _ = tray::forward_action(&Kind::Screenbound.identity(), "sync-browser");
    }
}
