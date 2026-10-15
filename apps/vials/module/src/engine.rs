//! Runtime of the module: the shared desktop layer, one worker thread that reconciles the desktop
//! with the config, and one window per vial.
//!
//! Two ways to keep a vial's items off the loose desktop. Where Windows lets icons be parked
//! off-screen they are parked. Where it does not (build 26300 since October 2026), Vials takes over
//! the desktop: the desktop's own icons are switched off (as "View, Show desktop icons" does) and
//! the built-in "Desktop" vial draws every item that is in no other vial. That mode is the default
//! (`draw_desktop`) and also lets Workspaces hide items per virtual desktop.
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
use tauri::window::{Color, Effect, EffectsBuilder};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewUrl, WebviewWindowBuilder,
    WindowEvent,
};

use crate::core::clip;
use crate::core::gesture;
use crate::core::layout::{self, MonitorRef};
use crate::core::model::{GlassBlur, PixelRect, Vial, VialTheme, VialsConfig};
use crate::core::rules;
use crate::platform::{self, IconsWatcher, MouseEvent, MouseWatcher};

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
    /// The right button went down on the desktop here: remembered for "Create vial here".
    ContextDown((i32, i32)),
    /// A mouse button went down at this screen point (closes the popup when it is outside it).
    ButtonDown((i32, i32)),
    /// The desktop's icon view was shown or hidden ("Show desktop icons").
    IconsChanged,
    /// Another owner (Workspaces) hid or showed items: the vials only need to redraw.
    ClaimsChanged,
    /// Ctrl+V on the desktop with these files cut: the ones that sit in a vial leave it.
    CutPasted(Vec<String>),
    /// A vial window lost the keyboard focus (named by its label).
    VialBlurred(String),
    Stop,
}

struct Running {
    layer: Arc<dyn DesktopLayer>,
    jobs: Sender<Job>,
    _subscription: Subscription,
    _claims: Subscription,
    _mouse: Option<MouseWatcher>,
    _paste: Option<platform::CutPasteWatcher>,
    _icons: Option<IconsWatcher>,
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
    /// Where the desktop was last right-clicked (screen pixels).
    context_point: Mutex<Option<(i32, i32)>>,
    /// Where the popup is on screen while it is open.
    menu_rect: Mutex<Option<PixelRect>>,
    /// The look the open vial windows currently have; a change restyles them in place.
    applied_theme: Mutex<Option<(VialTheme, GlassBlur, u8)>>,
    /// `false` while the desktop's "Show desktop icons" is off: the vials hide with them.
    icons_visible: AtomicBool,
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
            context_point: Mutex::new(None),
            menu_rect: Mutex::new(None),
            applied_theme: Mutex::new(None),
            icons_visible: AtomicBool::new(true),
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
        if self.is_running() {
            apply_context_menu(config.context_menu);
        }
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

    let claims = {
        let sink = jobs.clone();
        layer
            .subscribe_claims(Box::new(move || {
                let _ = sink.send(Job::ClaimsChanged);
            }))
            .map_err(|e| e.to_string())?
    };

    // Not fatal: without the hook everything works except creating vials by dragging a box.
    let mouse = {
        let sink = jobs.clone();
        MouseWatcher::start(Box::new(move |event| {
            let _ = sink.send(match event {
                MouseEvent::Marquee(down, up) => Job::Marquee(down, up),
                MouseEvent::ContextDown(at) => Job::ContextDown(at),
                MouseEvent::ButtonDown(at) => Job::ButtonDown(at),
            });
        }))
        .map_err(|e| tracing::warn!("could not watch the desktop for dragged boxes: {e}"))
        .ok()
    };

    // When Vials draws the desktop itself its windows stay up whatever the icons' switch says.
    // Not fatal: without the hook a vial's items still leave it when they are dragged out.
    let paste = {
        let sink = jobs.clone();
        platform::watch_cut_paste(Box::new(move |paths| {
            let _ = sink.send(Job::CutPasted(paths));
        }))
        .map_err(|e| tracing::warn!("could not watch Ctrl+V on the desktop: {e}"))
        .ok()
    };

    state
        .icons_visible
        .store(state.config().draw_desktop || desqu_desktop::icons_visible().unwrap_or(true), Ordering::Relaxed);
    let icons = {
        let sink = jobs.clone();
        IconsWatcher::start(Box::new(move || {
            let _ = sink.send(Job::IconsChanged);
        }))
        .map_err(|e| tracing::warn!("could not watch \"Show desktop icons\": {e}"))
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
    apply_context_menu(state.config().context_menu);
    tracing::info!(config = %state.config_path.display(), "starting Vials");
    *running = Some(Running {
        layer,
        jobs,
        _subscription: subscription,
        _claims: claims,
        _mouse: mouse,
        _paste: paste,
        _icons: icons,
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
    apply_context_menu(false);
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
            Ok(Job::IconsChanged) => apply_icons_visibility(&app, &layer),
            Ok(Job::ClaimsChanged) => {
                let _ = app.emit(EVENT_CHANGED, ());
            }
            Ok(Job::CutPasted(paths)) => release_cut_files(&app, &paths),
            Ok(Job::VialBlurred(label)) => release_focus_later(&app, label),
            Ok(Job::ButtonDown(at)) => {
                let outside = app.try_state::<ModuleState>().is_some_and(|state| {
                    lock(&state.menu_rect).is_some_and(|r| at.0 < r.left || at.0 >= r.right || at.1 < r.top || at.1 >= r.bottom)
                });
                if outside {
                    close_menu(&app);
                }
            }
            Ok(Job::ContextDown(at)) => {
                if let Some(state) = app.try_state::<ModuleState>() {
                    *lock(&state.context_point) = Some(at);
                }
            }
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
    // The Desktop vial exists while Vials draws the desktop, and the desktop's own icons follow.
    {
        let mut cfg = lock(&state.config);
        if cfg.ensure_inbox() {
            if let Err(e) = cfg.save(&state.config_path) {
                tracing::warn!("could not save vials config: {e}");
            }
        }
    }
    let surface_problem = sync_surface(layer, &state.config());
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
    // Taking over the desktop, nothing is parked: a vial's items simply are not drawn in the Desktop vial.
    let desired: HashSet<String> =
        if cfg.hide_on_desktop && !cfg.draw_desktop { cfg.assigned_ids().into_iter().collect() } else { HashSet::new() };
    let on_desktop: HashSet<&str> = items.iter().map(|i| i.id.as_str()).collect();
    let mut missing = false;
    let mut problem = surface_problem;

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
            Ok(report) => {
                missing |= !report.missing.is_empty();
                if !report.failed.is_empty() {
                    // System items (Recycle Bin) have no file to mark hidden, and Windows refuses the
                    // Public Desktop without administrator rights: they stay on the desktop.
                    problem = Some(format!(
                        "{} item(s) in a vial cannot be hidden on the desktop (system items such as the Recycle Bin, or files Windows will not let Vials mark hidden), so they stay visible there too",
                        report.failed.len()
                    ));
                }
            }
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
    // The Desktop vial only exists as a window while Vials draws the desktop.
    let shown: Vec<&Vial> = cfg.vials.iter().filter(|v| !v.inbox || cfg.draw_desktop).collect();
    let wanted: HashSet<String> = shown.iter().map(|v| label_for(&v.id)).collect();
    // A new look is applied to the open windows in place. (Rebuilding them under the same label
    // fails: the old window is still being destroyed, so the vials would vanish.)
    let theme_changed = app.try_state::<ModuleState>().is_some_and(|state| {
        let mut applied = lock(&state.applied_theme);
        let now = (cfg.theme, cfg.glass_blur, cfg.glass_tint);
        let changed = applied.is_some_and(|t| t != now);
        *applied = Some(now);
        changed
    });
    close_vial_windows(app, &wanted);

    for vial in shown {
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
            if theme_changed {
                style_window(&window, cfg);
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
            .shadow(cfg.theme == VialTheme::Glass)
            .transparent(true)
            .visible(false)
            .build();
        match built {
            Ok(window) => {
                let _ = window.set_position(position);
                let _ = window.set_size(size);
                let jobs = jobs.clone();
                let blurred = label.clone();
                window.on_window_event(move |event| match event {
                    WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                        let _ = jobs.send(Job::WindowChanged);
                    }
                    WindowEvent::Focused(false) => {
                        let _ = jobs.send(Job::VialBlurred(blurred.clone()));
                    }
                    _ => {}
                });
                if cfg.theme == VialTheme::Glass {
                    apply_glass(&window, cfg.glass_blur, cfg.glass_tint);
                }
                // With "Show desktop icons" off the vial stays hidden until the icons come back.
                let icons_on = app.try_state::<ModuleState>().map(|s| s.icons_visible.load(Ordering::Relaxed));
                if icons_on.unwrap_or(true) {
                    show_vial_window(&window);
                }
            }
            Err(e) => tracing::warn!("could not create the window for vial '{}': {e}", vial.name),
        }
    }
}

/// Shows a vial window without taking focus and keeps it out of Alt+Tab (skip_taskbar alone does
/// not). After show(): Tauri rewrites the extended style while showing.
fn show_vial_window<R: Runtime>(window: &tauri::WebviewWindow<R>) {
    let _ = window.show();
    if let Ok(hwnd) = window.hwnd() {
        if let Err(e) = platform::mark_as_tool_window(hwnd.0 as isize) {
            tracing::warn!("could not make the vial window a tool window: {e}");
        }
    }
}

/// Brings the desktop's own icon layer in line with `draw_desktop`. Returns a problem to show.
fn sync_surface(layer: &Arc<dyn DesktopLayer>, cfg: &VialsConfig) -> Option<String> {
    let have = layer.surface_active();
    let result = if cfg.draw_desktop {
        // Also re-asserts it when the icons were switched back on behind our back.
        let icons_back = desqu_desktop::icons_visible().unwrap_or(false);
        if have && !icons_back {
            return None;
        }
        layer.set_surface(true)
    } else if have {
        layer.set_surface(false)
    } else {
        return None;
    };
    result.err().map(|e| {
        tracing::warn!("could not switch the desktop's own icons: {e}");
        format!("Vials could not take over the desktop: {e}")
    })
}

/// The desktop's "Show desktop icons" was switched: the vials follow, since they are desktop icons
/// grouped in a box.
fn apply_icons_visibility<R: Runtime>(app: &AppHandle<R>, layer: &Arc<dyn DesktopLayer>) {
    let Some(state) = app.try_state::<ModuleState>() else { return };
    let mut visible = desqu_desktop::icons_visible().unwrap_or(true);
    if state.config().draw_desktop {
        // Vials draws the desktop, so the desktop's own icons stay off (the setting in Vials decides,
        // not the View menu) and the vials stay up.
        if visible {
            if let Err(e) = layer.set_surface(true) {
                tracing::warn!("could not switch the desktop icons off again: {e}");
                *lock(&state.problem) = Some(e.to_string());
            }
        }
        visible = true;
    }
    if state.icons_visible.swap(visible, Ordering::Relaxed) == visible {
        return;
    }
    for (label, window) in app.webview_windows() {
        if !label.starts_with(LABEL_PREFIX) {
            continue;
        }
        if visible {
            show_vial_window(&window);
        } else {
            let _ = window.hide();
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
    if let Some(state) = app.try_state::<ModuleState>() {
        *lock(&state.menu_rect) = None;
    }
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

    let look = app.try_state::<ModuleState>().map(|s| s.config());
    let glass = look.as_ref().is_some_and(|c| c.theme == VialTheme::Glass);
    let window = WebviewWindowBuilder::new(app, MENU_LABEL, WebviewUrl::App("vials-menu".into()))
        .title("Create a vial")
        .decorations(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .focused(false)
        .shadow(glass)
        .transparent(true)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;
    if let Some(cfg) = look.as_ref().filter(|c| c.theme == VialTheme::Glass) {
        apply_glass(&window, cfg.glass_blur, cfg.glass_tint);
    }
    let _ = window.set_size(PhysicalSize::new(size.0 as u32, size.1 as u32));
    let _ = window.set_position(PhysicalPosition::new(x, y));
    if let Some(state) = app.try_state::<ModuleState>() {
        *lock(&state.menu_rect) = Some(PixelRect { left: x, top: y, right: x + size.0, bottom: y + size.1 });
    }
    // It stays hidden until its page asks to be shown (`menu_ready`), so there is no blank flash and
    // no focus fight while it loads. If the page never answers it is shown anyway.
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1500));
        menu_ready(&handle);
    });
    Ok(())
}

/// Frosted glass: the desktop behind the window is blurred (acrylic) with a dark tint so labels
/// stay readable on any wallpaper, and the corners are rounded. Both are best effort.
fn apply_glass<R: Runtime>(window: &tauri::WebviewWindow<R>, blur: GlassBlur, tint_percent: u8) {
    let alpha = (u16::from(tint_percent.min(100)) * 255 / 100) as u8;
    let effect = match blur {
        GlassBlur::Frosted => Effect::Acrylic,
        GlassBlur::Clear => Effect::Blur,
    };
    let effects = EffectsBuilder::new().effect(effect).color(Color(24, 28, 38, alpha)).build();
    if let Err(e) = window.set_effects(effects) {
        tracing::debug!("no glass effect on this system: {e}");
    }
    if let Ok(hwnd) = window.hwnd() {
        platform::round_corners(hwnd.0 as isize, true);
    }
}

/// Gives an existing vial window the configured look (the same properties it is built with).
fn style_window<R: Runtime>(window: &tauri::WebviewWindow<R>, cfg: &VialsConfig) {
    match cfg.theme {
        VialTheme::Glass => {
            apply_glass(window, cfg.glass_blur, cfg.glass_tint);
            let _ = window.set_shadow(true);
        }
        VialTheme::Classic => {
            let _ = window.set_effects(None::<tauri::utils::config::WindowEffectsConfig>);
            let _ = window.set_shadow(false);
            if let Ok(hwnd) = window.hwnd() {
                platform::round_corners(hwnd.0 as isize, false);
            }
        }
    }
}

/// The popup's page has loaded: show it and give it the keyboard (for Esc). Dismissal by clicking
/// elsewhere is done from the mouse hook, not from focus changes, which the web view causes itself.
pub fn menu_ready<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MENU_LABEL) {
        if window.is_visible().unwrap_or(false) {
            return;
        }
        let _ = window.show();
        let _ = window.set_focus();
    }
}

// ------------------------------------------------------------------------ renaming on the desktop

/// A vial window is non-focusable so it never steals focus; typing a name needs it focusable for
/// the moment of the edit.
pub fn begin_rename<R: Runtime>(app: &AppHandle<R>, vial_id: &str) -> Result<(), String> {
    let window = app.get_webview_window(&label_for(vial_id)).ok_or("that vial has no window")?;
    window.set_focusable(true).map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())
}

/// A click inside a vial gives it the keyboard (for Ctrl+C, Ctrl+X, Ctrl+V), without raising it over
/// the windows above the desktop. Once it loses the focus again it stops taking focus.
pub fn focus_vial<R: Runtime>(app: &AppHandle<R>, vial_id: &str) -> Result<(), String> {
    let window = app.get_webview_window(&label_for(vial_id)).ok_or("that vial has no window")?;
    if window.is_focused().unwrap_or(false) {
        return Ok(());
    }
    window.set_focusable(true).map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    if let Ok(hwnd) = window.hwnd() {
        platform::send_to_bottom(hwnd.0 as isize);
    }
    Ok(())
}

/// A vial lost the focus: if it did not get it back within a moment (it can flicker while the window
/// changes), it goes back to a window that never takes focus.
fn release_focus_later<R: Runtime>(app: &AppHandle<R>, label: String) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(250));
        let Some(window) = app.get_webview_window(&label) else { return };
        if window.is_focused().unwrap_or(false) {
            return;
        }
        let _ = window.set_focusable(false);
        if let Ok(hwnd) = window.hwnd() {
            let _ = platform::mark_as_tool_window(hwnd.0 as isize);
        }
    });
}

/// Back to a window that never takes focus, and back under every other window.
pub fn end_rename<R: Runtime>(app: &AppHandle<R>, vial_id: &str) -> Result<(), String> {
    let window = app.get_webview_window(&label_for(vial_id)).ok_or("that vial has no window")?;
    window.set_focusable(false).map_err(|e| e.to_string())?;
    if let Ok(hwnd) = window.hwnd() {
        platform::mark_as_tool_window(hwnd.0 as isize)?;
    }
    Ok(())
}

/// The popup was clicked: make a vial where the box was dragged. Returns its id.
pub fn create_vial_from_pending<R: Runtime>(app: &AppHandle<R>) -> Result<String, String> {
    let state = app.try_state::<ModuleState>().ok_or("the Vials plugin is not registered")?;
    let rect = lock(&state.pending).take().ok_or("there is no box to make a vial from")?;
    close_menu(app);
    create_vial_in(app, rect)
}

/// Default size of a vial made from the right-click menu, in logical pixels.
const CONTEXT_VIAL_SIZE: (f64, f64) = (380.0, 260.0);

/// "Create vial here" from the desktop's right-click menu: a vial with its top-left corner where
/// the desktop was right-clicked.
pub fn create_vial_at_context<R: Runtime>(app: &AppHandle<R>) -> Result<String, String> {
    let state = app.try_state::<ModuleState>().ok_or("the Vials plugin is not registered")?;
    if !state.is_running() {
        return Err("Vials is off".into());
    }
    let monitors = monitor_refs(app);
    let primary = monitors.iter().find(|m| m.primary).or(monitors.first()).ok_or("no monitor")?;
    let point = lock(&state.context_point).take().unwrap_or((
        (primary.bounds.left + primary.bounds.right) / 2,
        (primary.bounds.top + primary.bounds.bottom) / 2,
    ));
    let at = PixelRect { left: point.0, top: point.1, right: point.0 + 1, bottom: point.1 + 1 };
    let monitor = layout::monitor_containing(&at, &monitors).unwrap_or(primary);
    let size = ((CONTEXT_VIAL_SIZE.0 * monitor.scale).round() as i32, (CONTEXT_VIAL_SIZE.1 * monitor.scale).round() as i32);
    let (x, y) = gesture::popup_position(point, size, &monitor.bounds);
    create_vial_in(app, PixelRect { left: x, top: y, right: x + size.0, bottom: y + size.1 })
}

fn create_vial_in<R: Runtime>(app: &AppHandle<R>, rect: PixelRect) -> Result<String, String> {
    let state = app.try_state::<ModuleState>().ok_or("the Vials plugin is not registered")?;
    let monitors = monitor_refs(app);
    let monitor = layout::monitor_containing(&rect, &monitors).ok_or("that spot is not on a monitor")?;
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
        inbox: false,
    });
    state.set_config(cfg)?;
    Ok(id)
}

/// Adds or removes the "Create vial here" entry of the desktop's right-click menu. It points at the
/// running executable, so it is only present while Vials runs.
fn apply_context_menu(enabled: bool) {
    let exe = enabled.then(|| std::env::current_exe().ok()).flatten();
    let result = platform::set_context_menu(exe.as_deref().and_then(|p| p.to_str()));
    if let Err(e) = result {
        tracing::warn!("could not update the desktop right-click menu: {e}");
    }
}

// --------------------------------------------------------------------- dragging an item out

/// Runs the native drag on the main (UI) thread, waits for it to end and settles the result.
pub async fn drag_item_out<R: Runtime>(app: &AppHandle<R>, item_id: &str) -> Result<(), String> {
    let state = app.try_state::<ModuleState>().ok_or("the Vials plugin is not registered")?;
    ensure_known_item(&state, item_id)?;
    let path = item_id.to_string();
    let (tx, rx) = mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(platform::drag_item(&path));
    })
    .map_err(|e| e.to_string())?;
    // The drag is modal on the main thread: wait for it without blocking this async worker's peers.
    let end = tauri::async_runtime::spawn_blocking(move || rx.recv().map_err(|e| e.to_string())?)
        .await
        .map_err(|e| e.to_string())??;

    if let platform::DragEnd::OnDesktop(at) = end {
        // Back on the desktop: it lands where it was dropped and stops belonging to the vial.
        let layer = state.layer()?;
        if let Err(e) = layer.show_at(item_id, desqu_desktop::Point { x: at.0, y: at.1 }) {
            tracing::warn!("could not place the item where it was dropped: {e}");
        }
        let mut cfg = state.config();
        cfg.assign(item_id, None);
        state.set_config(cfg)?;
    }
    Ok(())
}

// ------------------------------------------------------------------- the item's right-click menu

/// Shows Explorer's own right-click menu for an item that sits in a vial, at the cursor, and runs
/// what the user picks. The menu is modal, so it runs on the main (UI) thread like the drag does.
pub async fn show_item_menu<R: Runtime>(app: &AppHandle<R>, item_id: &str) -> Result<(), String> {
    let state = app.try_state::<ModuleState>().ok_or("the Vials plugin is not registered")?;
    ensure_known_item(&state, item_id)?;
    let path = item_id.to_string();
    let (tx, rx) = mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(platform::show_item_menu(&path));
    })
    .map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || rx.recv().map_err(|e| e.to_string())?)
        .await
        .map_err(|e| e.to_string())?
}

/// Only things Vials knows about may be dragged, opened or given a menu, so the UI cannot be used
/// to act on arbitrary paths: items in a vial, or on the desktop.
pub(crate) fn ensure_known_item(state: &ModuleState, id: &str) -> Result<(), String> {
    ensure_known_items(state, std::slice::from_ref(&id.to_string()))
}

pub(crate) fn ensure_known_items(state: &ModuleState, ids: &[String]) -> Result<(), String> {
    let cfg = state.config();
    let unassigned: Vec<&String> = ids.iter().filter(|id| cfg.vial_of(id).is_none()).collect();
    if unassigned.is_empty() {
        return Ok(());
    }
    let on_desktop: HashSet<String> = state.layer()?.items().map_err(|e| e.to_string())?.into_iter().map(|i| i.id).collect();
    match unassigned.into_iter().find(|id| !on_desktop.contains(*id)) {
        Some(id) => Err(format!("{id} is not an item on the desktop")),
        None => Ok(()),
    }
}

// -------------------------------------------------------------------- copy, cut and paste

/// Ctrl+C / Ctrl+X on items in a vial: the real files go on the clipboard, so Explorer and any other
/// program can paste them (a cut moves them there). System items (Recycle Bin) have no file.
pub async fn copy_items<R: Runtime>(app: &AppHandle<R>, item_ids: Vec<String>, cut: bool) -> Result<(), String> {
    let state = app.try_state::<ModuleState>().ok_or("the Vials plugin is not registered")?;
    let paths: Vec<String> = item_ids.into_iter().filter(|id| !id.starts_with("::")).collect();
    if paths.is_empty() {
        return Err("System items such as the Recycle Bin have no file to copy".into());
    }
    ensure_known_items(&state, &paths)?;
    let (tx, rx) = mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(platform::clipboard::put_files(&paths, cut));
    })
    .map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || rx.recv().map_err(|e| e.to_string())?)
        .await
        .map_err(|e| e.to_string())?
}

/// Ctrl+V in a vial: the files on the clipboard come to the Desktop (moved when they were cut, copied
/// otherwise, by Explorer's own file operation) and go into the vial. Files cut on the Desktop already
/// are only moved between vials. Returns how many items the vial received.
pub async fn paste_into_vial<R: Runtime>(app: &AppHandle<R>, vial_id: &str) -> Result<usize, String> {
    let state = app.try_state::<ModuleState>().ok_or("the Vials plugin is not registered")?;
    if state.config().vial(vial_id).is_none() {
        return Err("that vial no longer exists".into());
    }
    let folders: Vec<String> = desqu_desktop::desktop_folders().iter().map(|p| p.to_string_lossy().into_owned()).collect();
    let target = folders.first().cloned().ok_or("the Desktop folder was not found")?;
    let ids: Vec<String> = state.layer()?.items().map_err(|e| e.to_string())?.into_iter().map(|i| i.id).collect();

    // A thread of its own: the file operation runs until it is done and shows its own dialogs.
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("vials-paste".into())
        .spawn(move || {
            let _ = tx.send(paste_files(&folders, &target, &ids));
        })
        .map_err(|e| e.to_string())?;
    let items = tauri::async_runtime::spawn_blocking(move || rx.recv().map_err(|e| e.to_string())?)
        .await
        .map_err(|e| e.to_string())??;

    let count = items.len();
    if count > 0 {
        let vial = vial_id.to_string();
        state.update_config(|cfg| {
            for item in &items {
                cfg.assign(item, Some(&vial));
            }
        })?;
    }
    Ok(count)
}

/// Brings the clipboard's files to the Desktop folder. Returns the ids of the items that now belong in
/// the vial.
fn paste_files(folders: &[String], target: &str, ids: &[String]) -> Result<Vec<String>, String> {
    let clip = platform::clipboard::files()?.ok_or("There are no files on the clipboard")?;
    let plan = clip::plan_paste(&clip.paths, clip.cut, folders);
    let mut items = clip::matching_ids(&plan.adopt, ids);
    items.extend(platform::clipboard::transfer(&plan.copy_here, false, target, true)?);
    items.extend(platform::clipboard::transfer(&plan.transfer, clip.cut, target, false)?);
    if clip.cut {
        // As in Explorer, a pasted cut is used up.
        platform::clipboard::clear();
    }
    Ok(items)
}

/// Ctrl+V on the desktop right after files were cut: Explorer does nothing when they are pasted into
/// the folder they are in, so the ones that sit in a vial are taken out of it here.
fn release_cut_files<R: Runtime>(app: &AppHandle<R>, paths: &[String]) {
    let Some(state) = app.try_state::<ModuleState>() else { return };
    let ids = clip::matching_ids(paths, &state.config().assigned_ids());
    if ids.is_empty() {
        return;
    }
    if let Err(e) = state.update_config(|cfg| {
        for id in &ids {
            cfg.assign(id, None);
        }
    }) {
        tracing::warn!("could not take the pasted items out of their vial: {e}");
    }
}
