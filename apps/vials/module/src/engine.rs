//! Runtime of the module: the shared desktop layer, one worker thread that reconciles the desktop
//! with the config, and one window per vial.
//!
//! Everything is event-driven. The worker sleeps until a desktop change, a window move, a command
//! or one of two timers fires: a single re-check a few seconds after each desktop change (Explorer's
//! view lags the file system), and a debounce for window moves.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use desqu_desktop::{DesktopError, DesktopLayer, Subscription};
use desqu_shell::mutex::{self, ModuleLock};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewUrl, WebviewWindowBuilder,
    WindowEvent,
};

use crate::core::gesture;
use crate::core::layout::{self, MonitorRef};
use crate::core::model::{PixelRect, Vial, VialsConfig};
use crate::core::rules;
use crate::platform::{self, MouseWatcher};

/// Id of the cross-process module lock.
pub const MODULE_ID: &str = "vials";
pub const EVENT_CHANGED: &str = "vials://changed";
pub const STOP_TIMEOUT: Duration = Duration::from_secs(5);
const LABEL_PREFIX: &str = "vial-";
/// The small "create a vial" popup. Its label must not start with `vial-` (those are vials).
pub const MENU_LABEL: &str = "vials-menu";
const MENU_SIZE: (f64, f64) = (230.0, 46.0);
/// Explorer updates its selection a moment after the button is released.
const SELECTION_SETTLE: Duration = Duration::from_millis(220);
/// The desktop view shows a new item several seconds after the file appears.
const LAG_RECHECK: Duration = Duration::from_secs(5);
const RETRY_DELAY: Duration = Duration::from_secs(3);
const MAX_RETRIES: u8 = 4;
const MOVE_DEBOUNCE: Duration = Duration::from_millis(500);

pub(crate) enum Job {
    /// Config or command change: reconcile now.
    Reconcile,
    /// The desktop folders changed: reconcile now and again after the view has caught up.
    DesktopChanged,
    /// A vial window was moved or resized (debounced).
    WindowChanged,
    /// The user dragged a box (screen pixels: start, end) over the desktop's icon view.
    Marquee((i32, i32), (i32, i32)),
    Stop,
}

struct Running {
    layer: Arc<dyn DesktopLayer>,
    jobs: Sender<Job>,
    _subscription: Subscription,
    _mouse: Option<MouseWatcher>,
    worker: Option<JoinHandle<()>>,
    _lock: ModuleLock,
}

pub struct ModuleState {
    pub(crate) config_path: PathBuf,
    pub(crate) journal_path: PathBuf,
    config: Mutex<VialsConfig>,
    running: Mutex<Option<Running>>,
    owned_elsewhere: AtomicBool,
    /// The box the user dragged, waiting for them to pick "create a vial" in the popup.
    pending: Mutex<Option<PixelRect>>,
    problem: Mutex<Option<String>>,
    icons: Mutex<HashMap<String, String>>,
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl ModuleState {
    pub fn new(config_path: PathBuf, journal_path: PathBuf) -> Self {
        let config = match VialsConfig::load(&config_path) {
            Ok(c) => c,
            Err(e) => {
                // Never overwrite a config we could not read: keep it aside.
                let backup = config_path.with_extension("json.broken");
                let _ = std::fs::rename(&config_path, &backup);
                tracing::warn!("vials config unreadable ({e}); moved to {} and using defaults", backup.display());
                VialsConfig::default()
            }
        };
        Self {
            config_path,
            journal_path,
            config: Mutex::new(config),
            running: Mutex::new(None),
            owned_elsewhere: AtomicBool::new(false),
            pending: Mutex::new(None),
            problem: Mutex::new(None),
            icons: Mutex::new(HashMap::new()),
        }
    }

    pub fn config(&self) -> VialsConfig {
        lock(&self.config).clone()
    }

    /// Replaces the config (validated and saved) and asks the worker to apply it.
    pub fn set_config(&self, config: VialsConfig) -> Result<VialsConfig, String> {
        let config = config.validated().map_err(|e| e.to_string())?;
        config.save(&self.config_path).map_err(|e| e.to_string())?;
        *lock(&self.config) = config.clone();
        self.request_reconcile();
        Ok(config)
    }

    /// Applies a change to the config under the lock, saves it and reconciles.
    pub fn update_config<T>(&self, f: impl FnOnce(&mut VialsConfig) -> T) -> Result<(VialsConfig, T), String> {
        let (config, out) = {
            let mut cfg = lock(&self.config);
            let out = f(&mut cfg);
            (cfg.clone(), out)
        };
        let config = config.validated().map_err(|e| e.to_string())?;
        config.save(&self.config_path).map_err(|e| e.to_string())?;
        *lock(&self.config) = config.clone();
        self.request_reconcile();
        Ok((config, out))
    }

    pub fn is_running(&self) -> bool {
        lock(&self.running).is_some()
    }

    pub fn owned_elsewhere(&self) -> bool {
        self.owned_elsewhere.load(Ordering::Relaxed)
    }

    pub fn problem(&self) -> Option<String> {
        lock(&self.problem).clone()
    }

    pub fn layer(&self) -> Result<Arc<dyn DesktopLayer>, String> {
        lock(&self.running)
            .as_ref()
            .map(|r| r.layer.clone())
            .ok_or_else(|| "Vials is off".to_string())
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

/// A window label for a vial id (ids are validated to be label-safe).
pub fn label_for(id: &str) -> String {
    format!("{LABEL_PREFIX}{id}")
}

// -------------------------------------------------------------------------------- start / stop

pub fn start<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let state = app.try_state::<ModuleState>().ok_or("the Vials plugin is not registered")?;
    let mut running = lock(&state.running);
    if running.is_some() {
        return Ok(());
    }
    // One process runs the module (a standalone app or Desqu), like every other module.
    let Some(module_lock) = mutex::try_acquire(MODULE_ID)? else {
        state.owned_elsewhere.store(true, Ordering::Relaxed);
        return Err("Vials is already running in another process".into());
    };
    // Items a previous run (or the other install) left parked: put them back first.
    if let Err(e) = desqu_desktop::recover_from(&state.journal_path) {
        tracing::warn!("could not recover desktop items: {e}");
    }
    let layer = match desqu_desktop::open("vials", &state.journal_path) {
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

    // Not fatal: without the hook everything works except creating vials by dragging a box.
    let mouse = {
        let sink = jobs.clone();
        MouseWatcher::start(Box::new(move |down, up| {
            let _ = sink.send(Job::Marquee(down, up));
        }))
        .map_err(|e| tracing::warn!("could not watch the desktop for dragged boxes: {e}"))
        .ok()
    };

    let worker = {
        let app = app.clone();
        let layer = layer.clone();
        let jobs = jobs.clone();
        std::thread::Builder::new()
            .name("vials-engine".into())
            .spawn(move || worker(app, rx, jobs, layer))
            .map_err(|e| e.to_string())?
    };
    let _ = jobs.send(Job::Reconcile);
    tracing::info!(config = %state.config_path.display(), "starting Vials");
    *running = Some(Running {
        layer,
        jobs,
        _subscription: subscription,
        _mouse: mouse,
        worker: Some(worker),
        _lock: module_lock,
    });
    Ok(())
}

/// Closes the vial windows, puts every hidden item back and ends the worker. A no-op when off.
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
    close_vial_windows(app, &HashSet::new());
    close_menu(app);
    if let Err(e) = running.layer.show_all() {
        tracing::warn!("could not restore hidden desktop items: {e}");
    }
    // Dropping `running` releases the subscription, the layer handle and the module lock.
    state.owned_elsewhere.store(false, Ordering::Relaxed);
    let _ = app.emit(EVENT_CHANGED, ());
}

// ---------------------------------------------------------------------------------- the worker

fn worker<R: Runtime>(app: AppHandle<R>, rx: Receiver<Job>, jobs: Sender<Job>, layer: Arc<dyn DesktopLayer>) {
    let mut later: Option<Instant> = None;
    let mut moved: Option<Instant> = None;
    let mut retries = 0u8;
    loop {
        let deadline = [later, moved].into_iter().flatten().min();
        let received = match deadline {
            Some(t) => rx.recv_timeout(t.saturating_duration_since(Instant::now())),
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        let now = Instant::now();
        match received {
            Ok(Job::Stop) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(Job::Reconcile) => {
                if reconcile(&app, &layer, &jobs).missing && retries < MAX_RETRIES {
                    retries += 1;
                    later = Some(now + RETRY_DELAY);
                }
            }
            Ok(Job::DesktopChanged) => {
                retries = 0;
                if reconcile(&app, &layer, &jobs).missing {
                    retries += 1;
                }
                later = Some(now + LAG_RECHECK);
            }
            Ok(Job::WindowChanged) => moved = Some(now + MOVE_DEBOUNCE),
            Ok(Job::Marquee(down, up)) => offer_new_vial(&app, down, up),
            Err(RecvTimeoutError::Timeout) => {
                if later.is_some_and(|t| t <= now) {
                    later = None;
                    if reconcile(&app, &layer, &jobs).missing && retries < MAX_RETRIES {
                        retries += 1;
                        later = Some(now + RETRY_DELAY);
                    }
                }
                if moved.is_some_and(|t| t <= now) {
                    moved = None;
                    persist_window_rects(&app);
                }
            }
        }
    }
}

struct Outcome {
    /// Some assigned items are not in the desktop view yet: worth another look shortly.
    missing: bool,
}

fn reconcile<R: Runtime>(app: &AppHandle<R>, layer: &Arc<dyn DesktopLayer>, jobs: &Sender<Job>) -> Outcome {
    let Some(state) = app.try_state::<ModuleState>() else {
        return Outcome { missing: false };
    };
    let items = match layer.items() {
        Ok(items) => items,
        Err(e) => {
            tracing::warn!("could not read the desktop: {e}");
            *lock(&state.problem) = Some(e.to_string());
            let _ = app.emit(EVENT_CHANGED, ());
            return Outcome { missing: true };
        }
    };

    // 1. New items that match a sort rule go into their vial.
    {
        let mut cfg = lock(&state.config);
        let assignments = rules::assign_new_items(&cfg, &items);
        if !assignments.is_empty() {
            for (item, vial) in assignments {
                cfg.assign(&item, Some(&vial));
            }
            if let Err(e) = cfg.save(&state.config_path) {
                tracing::warn!("could not save vials config: {e}");
            }
        }
    }

    // 2. Hide what the vials hold, show what no longer belongs in one.
    let cfg = state.config();
    let desired: HashSet<String> = if cfg.hide_on_desktop { cfg.assigned_ids().into_iter().collect() } else { HashSet::new() };
    let on_desktop: HashSet<&str> = items.iter().map(|i| i.id.as_str()).collect();
    let mut missing = false;
    let mut problem = None;

    match layer.hidden() {
        Ok(hidden) => {
            let to_show: Vec<String> = hidden.into_iter().filter(|id| !desired.contains(id)).collect();
            if !to_show.is_empty() {
                if let Err(e) = layer.show(&to_show) {
                    problem = Some(e.to_string());
                }
            }
        }
        Err(e) => problem = Some(e.to_string()),
    }
    let to_hide: Vec<String> = desired.iter().filter(|id| on_desktop.contains(id.as_str())).cloned().collect();
    // An assigned item that is not on the desktop (yet) may just be lagging in the view.
    if desired.iter().any(|id| !on_desktop.contains(id.as_str())) {
        missing = true;
    }
    if !to_hide.is_empty() {
        match layer.hide(&to_hide) {
            Ok(report) => missing |= !report.missing.is_empty(),
            Err(DesktopError::AutoArrange) => problem = Some(DesktopError::AutoArrange.to_string()),
            Err(e) => problem = Some(e.to_string()),
        }
    }
    *lock(&state.problem) = problem;

    // 3. One window per vial.
    sync_windows(app, &cfg, jobs);

    let _ = app.emit(EVENT_CHANGED, ());
    Outcome { missing }
}

// --------------------------------------------------------------------------------- the windows

fn monitor_refs<R: Runtime>(app: &AppHandle<R>) -> Vec<MonitorRef> {
    let primary = app.primary_monitor().ok().flatten();
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let pos = m.position();
            let size = m.size();
            MonitorRef {
                name: m.name().cloned().unwrap_or_else(|| format!("monitor-{i}")),
                bounds: PixelRect {
                    left: pos.x,
                    top: pos.y,
                    right: pos.x + size.width as i32,
                    bottom: pos.y + size.height as i32,
                },
                primary: primary.as_ref().is_some_and(|p| p.position() == pos && p.size() == size),
                scale: m.scale_factor(),
            }
        })
        .collect()
}

pub fn monitors<R: Runtime>(app: &AppHandle<R>) -> Vec<MonitorRef> {
    monitor_refs(app)
}

fn close_vial_windows<R: Runtime>(app: &AppHandle<R>, keep: &HashSet<String>) {
    for (label, window) in app.webview_windows() {
        if label.starts_with(LABEL_PREFIX) && !keep.contains(&label) {
            let _ = window.destroy();
        }
    }
}

/// Creates, moves and removes vial windows so they match `cfg`.
fn sync_windows<R: Runtime>(app: &AppHandle<R>, cfg: &VialsConfig, jobs: &Sender<Job>) {
    let monitors = monitor_refs(app);
    let wanted: HashSet<String> = cfg.vials.iter().map(|v| label_for(&v.id)).collect();
    close_vial_windows(app, &wanted);

    for vial in &cfg.vials {
        let Some(monitor) = layout::pick_monitor(vial.monitor.as_deref(), &monitors) else {
            continue;
        };
        let rect = layout::resolve(&vial.rect, &monitor.bounds);
        let label = label_for(&vial.id);
        let position = PhysicalPosition::new(rect.left, rect.top);
        let height = layout::window_height(rect.height(), vial.collapsed, monitor.scale);
        let size = PhysicalSize::new(rect.width().max(120) as u32, height.max(1) as u32);

        if let Some(window) = app.get_webview_window(&label) {
            let _ = window.set_title(&vial.name);
            if window.outer_position().ok() != Some(position) {
                let _ = window.set_position(position);
            }
            if window.outer_size().ok() != Some(size) {
                let _ = window.set_size(size);
            }
            continue;
        }

        // A bottom-most, non-activating tool window: visible above the desktop icons, never takes
        // focus, never shows in the taskbar (measured in the P8.1 spike).
        let built = WebviewWindowBuilder::new(app, &label, WebviewUrl::App(format!("vial/{}", vial.id).into()))
            .title(&vial.name)
            .decorations(false)
            .resizable(true)
            .skip_taskbar(true)
            .always_on_bottom(true)
            .focusable(false)
            .shadow(false)
            .transparent(true)
            .visible(false)
            .build();
        match built {
            Ok(window) => {
                let _ = window.set_position(position);
                let _ = window.set_size(size);
                let jobs = jobs.clone();
                window.on_window_event(move |event| {
                    if matches!(event, WindowEvent::Moved(_) | WindowEvent::Resized(_)) {
                        let _ = jobs.send(Job::WindowChanged);
                    }
                });
                let _ = window.show();
                // Keep it out of Alt+Tab too (skip_taskbar alone does not). After show(): Tauri
                // rewrites the extended style while showing.
                if let Ok(hwnd) = window.hwnd() {
                    if let Err(e) = platform::mark_as_tool_window(hwnd.0 as isize) {
                        tracing::warn!("could not make the vial window a tool window: {e}");
                    }
                }
            }
            Err(e) => tracing::warn!("could not create the window for vial '{}': {e}", vial.name),
        }
    }
}

/// The user dragged or resized a vial window: remember where it is, as fractions of its monitor.
fn persist_window_rects<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<ModuleState>() else {
        return;
    };
    let monitors = monitor_refs(app);
    let mut changed = false;
    {
        let mut cfg = lock(&state.config);
        for vial in &mut cfg.vials {
            let Some(window) = app.get_webview_window(&label_for(&vial.id)) else {
                continue;
            };
            let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) else {
                continue;
            };
            let pixels = PixelRect {
                left: pos.x,
                top: pos.y,
                right: pos.x + size.width as i32,
                bottom: pos.y + size.height as i32,
            };
            let Some(monitor) = layout::monitor_containing(&pixels, &monitors) else {
                continue;
            };
            let mut rect = layout::to_normalized(&pixels, &monitor.bounds);
            if vial.collapsed {
                // The window is only a header: keep the height the user chose for the open vial.
                rect.height = vial.rect.height;
            }
            let same = (rect.x - vial.rect.x).abs() < 0.002
                && (rect.y - vial.rect.y).abs() < 0.002
                && (rect.width - vial.rect.width).abs() < 0.002
                && (rect.height - vial.rect.height).abs() < 0.002;
            if !same || vial.monitor.as_deref() != Some(monitor.name.as_str()) {
                vial.rect = rect;
                vial.monitor = Some(monitor.name.clone());
                changed = true;
            }
        }
        if changed {
            if let Err(e) = cfg.save(&state.config_path) {
                tracing::warn!("could not save vials config: {e}");
            }
        }
    }
    if changed {
        let _ = app.emit(EVENT_CHANGED, ());
    }
}

// ------------------------------------------------------------------- the "create a vial" popup

/// After a box was dragged over the desktop: if it selected nothing, offer to make a vial there.
fn offer_new_vial<R: Runtime>(app: &AppHandle<R>, down: (i32, i32), up: (i32, i32)) {
    let Some(state) = app.try_state::<ModuleState>() else { return };
    if !state.config().create_on_drag {
        return;
    }
    let Some(rect) = gesture::marquee(down, up) else { return };
    // Let Explorer finish selecting, then ask it: a box that caught icons is a selection, not a vial.
    std::thread::sleep(SELECTION_SETTLE);
    match desqu_desktop::selection_count() {
        Ok(0) => {}
        Ok(_) => return,
        Err(e) => {
            tracing::debug!("could not read the desktop selection: {e}");
            return;
        }
    }
    *lock(&state.pending) = Some(rect);
    if let Err(e) = show_menu(app, up) {
        tracing::warn!("could not show the create-a-vial popup: {e}");
    }
}

pub fn close_menu<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MENU_LABEL) {
        let _ = window.destroy();
    }
}

fn show_menu<R: Runtime>(app: &AppHandle<R>, cursor: (i32, i32)) -> Result<(), String> {
    close_menu(app);
    let monitors = monitor_refs(app);
    let at = PixelRect { left: cursor.0, top: cursor.1, right: cursor.0 + 1, bottom: cursor.1 + 1 };
    let monitor = layout::monitor_containing(&at, &monitors).or(monitors.first()).ok_or("no monitor")?;
    let size = ((MENU_SIZE.0 * monitor.scale).round() as i32, (MENU_SIZE.1 * monitor.scale).round() as i32);
    let (x, y) = gesture::popup_position(cursor, size, &monitor.bounds);

    let window = WebviewWindowBuilder::new(app, MENU_LABEL, WebviewUrl::App("vials-menu".into()))
        .title("Create a vial")
        .decorations(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .focused(true)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;
    let _ = window.set_size(PhysicalSize::new(size.0 as u32, size.1 as u32));
    let _ = window.set_position(PhysicalPosition::new(x, y));
    // Like a context menu: it goes away as soon as you click anywhere else.
    let handle = app.clone();
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Focused(false)) {
            close_menu(&handle);
        }
    });
    window.show().map_err(|e| e.to_string())?;
    let _ = window.set_focus();
    Ok(())
}

/// The popup was clicked: make a vial where the box was dragged. Returns its id.
pub fn create_vial_from_pending<R: Runtime>(app: &AppHandle<R>) -> Result<String, String> {
    let state = app.try_state::<ModuleState>().ok_or("the Vials plugin is not registered")?;
    let rect = lock(&state.pending).take().ok_or("there is no box to make a vial from")?;
    close_menu(app);
    let monitors = monitor_refs(app);
    let monitor = layout::monitor_containing(&rect, &monitors).ok_or("the box is not on a monitor")?;
    let normalized = layout::to_normalized(&rect, &monitor.bounds);

    let mut cfg = state.config();
    let id = gesture::next_id(cfg.vials.iter().map(|v| v.id.as_str()));
    let name = gesture::next_name(cfg.vials.iter().map(|v| v.name.as_str()));
    cfg.vials.push(Vial {
        id: id.clone(),
        name,
        monitor: Some(monitor.name.clone()),
        rect: normalized,
        items: Vec::new(),
        collapsed: false,
    });
    state.set_config(cfg)?;
    Ok(id)
}
