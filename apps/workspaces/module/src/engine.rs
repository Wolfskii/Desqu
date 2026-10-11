//! Runtime of the module: the shared desktop layer and one worker thread that makes the desktop
//! icons match the workspace of the current virtual desktop.
//!
//! Everything is event-driven. The worker sleeps until the virtual desktop registry key changes,
//! the desktop folders change, a command arrives, or the single delayed re-check fires (Explorer's
//! view lags the file system by a few seconds).

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use desqu_desktop::{DesktopError, DesktopLayer, Subscription};
use desqu_shell::mutex::{self, ModuleLock};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::core::model::WorkspacesConfig;
use crate::core::paste;
use crate::core::visibility;
use crate::platform::{self, CutPasteWatcher, SwitchWatcher};

/// Id of the cross-process module lock.
pub const MODULE_ID: &str = "workspaces";
pub const EVENT_CHANGED: &str = "workspaces://changed";
/// The desktop view shows a new item several seconds after the file appears.
const LAG_RECHECK: Duration = Duration::from_secs(5);
const RETRY_DELAY: Duration = Duration::from_secs(3);
const MAX_RETRIES: u8 = 4;

pub(crate) enum Job {
    /// Config or command change: reconcile now.
    Reconcile,
    /// The virtual desktop registry key changed.
    Switched,
    /// The desktop folders changed: reconcile now and again after the view has caught up.
    DesktopChanged,
    /// Ctrl+V on the desktop with files cut: they now belong to the desktop you are on.
    Pasted(Vec<String>),
    Stop,
}

struct Running {
    layer: Arc<dyn DesktopLayer>,
    jobs: Sender<Job>,
    _subscription: Subscription,
    _paste: Option<CutPasteWatcher>,
    worker: Option<JoinHandle<()>>,
    _lock: ModuleLock,
}

/// What the last pass over the desktop did; shown in Settings so a problem is visible.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileInfo {
    /// Seconds since the Unix epoch.
    pub at: u64,
    pub current_desktop: Option<String>,
    pub items: usize,
    pub wanted_hidden: usize,
    pub hidden_now: usize,
    pub failed: usize,
    pub error: Option<String>,
}

pub struct ModuleState {
    pub(crate) config_path: PathBuf,
    pub(crate) journal_path: PathBuf,
    config: Mutex<WorkspacesConfig>,
    running: Mutex<Option<Running>>,
    owned_elsewhere: AtomicBool,
    problem: Mutex<Option<String>>,
    last: Mutex<Option<ReconcileInfo>>,
    icons: Mutex<std::collections::HashMap<String, String>>,
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl ModuleState {
    pub fn new(config_path: PathBuf, journal_path: PathBuf) -> Self {
        let config = match WorkspacesConfig::load(&config_path) {
            Ok(c) => c,
            Err(e) => {
                // Never overwrite a config we could not read: keep it aside.
                let backup = config_path.with_extension("json.broken");
                let _ = std::fs::rename(&config_path, &backup);
                tracing::warn!(
                    "workspaces config unreadable ({e}); moved to {} and using defaults",
                    backup.display()
                );
                WorkspacesConfig::default()
            }
        };
        Self {
            config_path,
            journal_path,
            config: Mutex::new(config),
            running: Mutex::new(None),
            owned_elsewhere: AtomicBool::new(false),
            problem: Mutex::new(None),
            last: Mutex::new(None),
            icons: Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn config(&self) -> WorkspacesConfig {
        lock(&self.config).clone()
    }

    /// Replaces the config (validated and saved) and asks the worker to apply it.
    pub fn set_config(&self, config: WorkspacesConfig) -> Result<WorkspacesConfig, String> {
        let config = config.validated().map_err(|e| e.to_string())?;
        config.save(&self.config_path).map_err(|e| e.to_string())?;
        *lock(&self.config) = config.clone();
        self.request_reconcile();
        Ok(config)
    }

    pub fn is_running(&self) -> bool {
        lock(&self.running).is_some()
    }

    pub fn owned_elsewhere(&self) -> bool {
        self.owned_elsewhere.load(Ordering::Relaxed)
    }

    pub fn last_reconcile(&self) -> Option<ReconcileInfo> {
        lock(&self.last).clone()
    }

    pub fn problem(&self) -> Option<String> {
        lock(&self.problem).clone()
    }

    pub fn layer(&self) -> Result<Arc<dyn DesktopLayer>, String> {
        lock(&self.running)
            .as_ref()
            .map(|r| r.layer.clone())
            .ok_or_else(|| "Workspaces is off".to_string())
    }

    pub fn request_reconcile(&self) {
        if let Some(r) = lock(&self.running).as_ref() {
            let _ = r.jobs.send(Job::Reconcile);
        }
    }

    pub(crate) fn cached_icon(&self, key: &str) -> Option<String> {
        lock(&self.icons).get(key).cloned()
    }

    pub(crate) fn cache_icon(&self, key: String, url: String) {
        lock(&self.icons).insert(key, url);
    }
}

// -------------------------------------------------------------------------------- start / stop

pub fn start<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let state = app
        .try_state::<ModuleState>()
        .ok_or("the Workspaces plugin is not registered")?;
    let mut running = lock(&state.running);
    if running.is_some() {
        return Ok(());
    }
    // One process runs the module (a standalone app or Desqu), like every other module.
    let Some(module_lock) = mutex::try_acquire(MODULE_ID)? else {
        state.owned_elsewhere.store(true, Ordering::Relaxed);
        return Err("Workspaces is already running in another process".into());
    };
    // Items a previous run (or the other install) left parked: put them back first.
    if let Err(e) = desqu_desktop::recover_from(&state.journal_path) {
        tracing::warn!("could not recover desktop items: {e}");
    }
    let layer = match desqu_desktop::open("workspaces", &state.journal_path) {
        Ok(layer) => layer,
        Err(DesktopError::OwnedElsewhere) => {
            state.owned_elsewhere.store(true, Ordering::Relaxed);
            return Err("another Desqu app is arranging the desktop".into());
        }
        Err(e) => return Err(e.to_string()),
    };
    state.owned_elsewhere.store(false, Ordering::Relaxed);
    *lock(&state.problem) = None;

    let (jobs, rx) = mpsc::channel();
    let sink = jobs.clone();
    let subscription = layer
        .subscribe(Box::new(move |_| {
            let _ = sink.send(Job::DesktopChanged);
        }))
        .map_err(|e| e.to_string())?;

    // Not fatal: without the hook everything works except moving icons with Ctrl+X / Ctrl+V.
    let paste = {
        let sink = jobs.clone();
        platform::watch_cut_paste(Box::new(move |paths| {
            let _ = sink.send(Job::Pasted(paths));
        }))
        .map_err(|e| tracing::warn!("could not watch Ctrl+V on the desktop: {e}"))
        .ok()
    };

    let worker = {
        let app = app.clone();
        let layer = layer.clone();
        let jobs = jobs.clone();
        std::thread::Builder::new()
            .name("workspaces-engine".into())
            .spawn(move || worker(app, rx, jobs, layer))
            .map_err(|e| e.to_string())?
    };
    let _ = jobs.send(Job::Reconcile);
    tracing::info!(config = %state.config_path.display(), "starting Workspaces");
    *running = Some(Running {
        layer,
        jobs,
        _subscription: subscription,
        _paste: paste,
        worker: Some(worker),
        _lock: module_lock,
    });
    Ok(())
}

/// Puts every hidden item back and ends the worker. A no-op when off.
pub fn stop<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<ModuleState>() else {
        return;
    };
    let running = lock(&state.running).take();
    let Some(mut running) = running else {
        state.owned_elsewhere.store(false, Ordering::Relaxed);
        return;
    };
    let _ = running.jobs.send(Job::Stop);
    if let Some(worker) = running.worker.take() {
        let _ = worker.join();
    }
    if let Err(e) = running.layer.show_all() {
        tracing::warn!("could not restore hidden desktop items: {e}");
    }
    // Dropping `running` releases the subscription, the layer handle and the module lock.
    state.owned_elsewhere.store(false, Ordering::Relaxed);
    let _ = app.emit(EVENT_CHANGED, ());
}

// ---------------------------------------------------------------------------------- the worker

fn worker<R: Runtime>(
    app: AppHandle<R>,
    rx: Receiver<Job>,
    jobs: Sender<Job>,
    layer: Arc<dyn DesktopLayer>,
) {
    let mut watcher: Option<SwitchWatcher> = None;
    // Items seen so far; a file that shows up later is "new" and may be given to the current desktop.
    let mut known: Option<HashSet<String>> = None;
    let mut later: Option<Instant> = None;
    let mut retries = 0u8;
    loop {
        // The registry key only exists once Windows has more than one desktop: keep trying.
        if watcher.is_none() {
            let sink = std::sync::Mutex::new(jobs.clone());
            watcher = platform::watch_switches(Arc::new(move || {
                let _ = lock(&sink).send(Job::Switched);
            }))
            .map_err(|e| tracing::debug!("not watching virtual desktops yet: {e}"))
            .ok();
        }
        let received = match later {
            Some(t) => rx.recv_timeout(t.saturating_duration_since(Instant::now())),
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        let now = Instant::now();
        match received {
            Ok(Job::Stop) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(Job::Reconcile) | Ok(Job::Switched) => {}
            Ok(Job::Pasted(paths)) => adopt_pasted(&app, &layer, &paths),
            Ok(Job::DesktopChanged) => {
                retries = 0;
                later = Some(now + LAG_RECHECK);
            }
            Err(RecvTimeoutError::Timeout) => later = None,
        }
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| reconcile(&app, &layer, &mut known)));
        let lagging = outcome.unwrap_or_else(|_| {
            tracing::error!("the desktop pass panicked; trying again later");
            true
        });
        if lagging && retries < MAX_RETRIES && later.is_none() {
            retries += 1;
            later = Some(now + RETRY_DELAY);
        }
    }
}

/// Makes the hidden icons match the current virtual desktop. Returns whether the desktop view
/// could not be read (worth another look shortly).
fn reconcile<R: Runtime>(
    app: &AppHandle<R>,
    layer: &Arc<dyn DesktopLayer>,
    known: &mut Option<HashSet<String>>,
) -> bool {
    let Some(state) = app.try_state::<ModuleState>() else {
        return false;
    };
    let items = match layer.items() {
        Ok(items) => items,
        Err(e) => {
            tracing::warn!("could not read the desktop: {e}");
            *lock(&state.problem) = Some(e.to_string());
            let _ = app.emit(EVENT_CHANGED, ());
            return true;
        }
    };
    let ids: Vec<String> = items.iter().map(|i| i.id.clone()).collect();
    let desktops = platform::desktops().unwrap_or_default();
    let live: Vec<String> = desktops.list.iter().map(|d| d.id.clone()).collect();
    let current = desktops.current.clone();

    // A file that appeared since the last look belongs to the desktop you are on.
    let fresh: Vec<String> = match known.as_ref() {
        Some(seen) => ids
            .iter()
            .filter(|id| !seen.contains(*id))
            .cloned()
            .collect(),
        None => Vec::new(),
    };
    known
        .get_or_insert_with(HashSet::new)
        .extend(ids.iter().cloned());
    if !fresh.is_empty() {
        if let Some(cur) = current
            .as_deref()
            .filter(|_| state.config().auto_assign_new)
        {
            let index = desktops.list.iter().position(|d| d.id == cur).unwrap_or(0);
            let name = match desktops.list.get(index).map(|d| d.name.as_str()) {
                Some(n) if !n.is_empty() => n.to_string(),
                _ => format!("Desktop {}", index + 1),
            };
            let mut cfg = lock(&state.config);
            // Only files nobody owns yet; a restored or re-listed item keeps its owner.
            let mut changed = false;
            for id in &fresh {
                if cfg.owner_of(id).is_none() {
                    changed |= cfg.assign(id, Some((cur, name.as_str())));
                }
            }
            if changed {
                if let Err(e) = cfg.save(&state.config_path) {
                    tracing::warn!("could not save workspaces config: {e}");
                }
            }
        }
    }

    let hidden = layer.hidden().unwrap_or_default();
    let cfg = state.config();
    let plan = visibility::plan(&ids, &hidden, &cfg, current.as_deref(), &live);
    tracing::info!(
        "desktop pass: current={:?} items={} claimed_hidden={} plan: hide {} show {}",
        current,
        ids.len(),
        hidden.len(),
        plan.hide.len(),
        plan.show.len()
    );

    let mut problem = None;
    if !plan.show.is_empty() {
        if let Err(e) = layer.show(&plan.show) {
            problem = Some(e.to_string());
        }
    }
    let mut lagging = false;
    let mut failed = 0usize;
    // Claimed as hidden but found on screen (a park that failed or was undone): park it again.
    let mut to_hide = plan.hide.clone();
    to_hide.extend(
        items
            .iter()
            .filter(|i| !i.parked && hidden.contains(&i.id) && !plan.show.contains(&i.id))
            .map(|i| i.id.clone()),
    );
    if !to_hide.is_empty() {
        match layer.hide(&to_hide) {
            Ok(report) => {
                lagging = !report.missing.is_empty() || !report.failed.is_empty();
                failed = report.failed.len();
            }
            Err(e) => {
                tracing::warn!("could not hide desktop items: {e}");
                problem = Some(e.to_string());
            }
        }
    }
    let hidden_now = layer.hidden().map_or(0, |h| h.len());
    *lock(&state.last) = Some(ReconcileInfo {
        at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
        current_desktop: current.clone(),
        items: ids.len(),
        wanted_hidden: to_hide.len() + hidden.len().saturating_sub(plan.show.len()),
        hidden_now,
        failed,
        error: problem.clone(),
    });
    *lock(&state.problem) = problem;
    let _ = app.emit(EVENT_CHANGED, ());
    lagging
}

/// Files were cut on the desktop and Ctrl+V was pressed on a desktop: they are now tied to the virtual
/// desktop you are on (and hidden on the others). Explorer itself does nothing for a cut-and-paste
/// into the folder the files are already in.
fn adopt_pasted<R: Runtime>(app: &AppHandle<R>, layer: &Arc<dyn DesktopLayer>, paths: &[String]) {
    let Some(state) = app.try_state::<ModuleState>() else { return };
    let desktops = platform::desktops().unwrap_or_default();
    let Some(current) = desktops.current.clone() else {
        tracing::info!("Ctrl+V on the desktop ignored: the current virtual desktop is unknown");
        return;
    };
    let ids: Vec<String> = match layer.items() {
        Ok(items) => items.into_iter().map(|i| i.id).collect(),
        Err(e) => {
            tracing::warn!("could not read the desktop for Ctrl+V: {e}");
            return;
        }
    };
    let picked = paste::desktop_items_in(paths, &ids);
    if picked.is_empty() {
        return;
    }
    let index = desktops.list.iter().position(|d| d.id == current).unwrap_or(0);
    let name = match desktops.list.get(index).map(|d| d.name.as_str()) {
        Some(n) if !n.is_empty() => n.to_string(),
        _ => format!("Desktop {}", index + 1),
    };
    let mut cfg = state.config();
    let mut changed = 0;
    for id in &picked {
        if cfg.assign(id, Some((&current, &name))) {
            changed += 1;
        }
    }
    tracing::info!("Ctrl+V on the desktop: {} of {} cut items now belong to '{name}'", changed, picked.len());
    if changed > 0 {
        if let Err(e) = state.set_config(cfg) {
            tracing::warn!("could not save the moved items: {e}");
        }
    }
}
