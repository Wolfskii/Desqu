//! Shared fakes for engine tests: a platform that records what the engine asks of it, and a
//! harness that builds an engine over temp files. Only compiled for tests.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use super::{EnginePaths, HostIdentity, Notification};
use crate::bridge::protocol::Product;
use crate::core::geometry::Rect;
use crate::core::monitor::{MonitorId, MonitorInfo};
use crate::core::window::{NativeStyle, ProcessRef, ShowState, WindowId, WindowIdentity, WindowState};
use crate::engine::Engine;
use crate::platform::{EventSink, EventSubscription, PlatformError, PlatformResult, PlatformWindowManager};

pub(crate) const CHROME_PID: u32 = 4242;
pub(crate) const CHROME_WINDOW: WindowId = WindowId(7);
pub(crate) const SCREEN: Rect = Rect::new(0, 0, 1920, 1080);

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A window in its normal state, framed, with the given bounds.
pub(crate) fn normal_state(bounds: Rect) -> WindowState {
    WindowState {
        bounds,
        visible_bounds: bounds,
        restore_bounds: bounds,
        show_state: ShowState::Normal,
        visible: true,
        cloaked: false,
        has_title_bar: true,
        has_resize_frame: true,
        topmost: false,
        style: NativeStyle {
            primary: 0x1600_0000,
            extended: 0x0001_0100,
        },
        monitor: Some(MonitorId("m1".into())),
        dpi: 96,
        zone_snapped: false,
        click_through: false,
    }
}

pub(crate) fn primary_monitor() -> MonitorInfo {
    MonitorInfo {
        id: MonitorId("m1".into()),
        device_name: r"\\.\DISPLAY1".into(),
        friendly_name: "Primary".into(),
        number: 1,
        is_primary: true,
        bounds: SCREEN,
        work_area: Rect::new(0, 0, 1920, 1040),
        dpi: 96,
    }
}

pub(crate) fn chrome_identity() -> WindowIdentity {
    WindowIdentity {
        process: ProcessRef {
            pid: CHROME_PID,
            start_time: 1,
        },
        process_name: "chrome.exe".into(),
        executable_path: r"C:\Chrome\chrome.exe".into(),
        class_name: "Chrome_WidgetWin_1".into(),
        title: "Big Buck Bunny - Google Chrome".into(),
    }
}

/// Records what the engine asks of it, and keeps window states as the app would leave them.
#[derive(Default)]
pub(crate) struct FakePlatform {
    pub foreground: Mutex<Option<WindowId>>,
    pub identities: HashMap<WindowId, WindowIdentity>,
    pub pids: HashMap<WindowId, u32>,
    pub states: Mutex<HashMap<WindowId, WindowState>>,
    /// Where the fake browser's page sits inside its window, as insets from each edge. The page follows
    /// the window when it moves or resizes, as a real page does.
    pub page_insets: Mutex<Option<Rect>>,
    pub monitors: Vec<MonitorInfo>,
    pub bounds_log: Mutex<Vec<Rect>>,
    pub region_log: Mutex<Vec<Option<Rect>>>,
}

impl FakePlatform {
    /// One Chrome window in front, on one monitor, with its page at `page` (or none).
    pub fn chrome(page: Option<Rect>) -> Self {
        let mut identities = HashMap::new();
        identities.insert(CHROME_WINDOW, chrome_identity());
        let mut pids = HashMap::new();
        pids.insert(CHROME_WINDOW, CHROME_PID);
        let window = Rect::new(100, 100, 1020, 900);
        let mut states = HashMap::new();
        states.insert(CHROME_WINDOW, normal_state(window));
        // The page is given in screen pixels for the window above; keep it as insets from its edges.
        let insets = page.map(|p| {
            Rect::new(p.left - window.left, p.top - window.top, window.right - p.right, window.bottom - p.bottom)
        });
        FakePlatform {
            foreground: Mutex::new(Some(CHROME_WINDOW)),
            identities,
            pids,
            states: Mutex::new(states),
            page_insets: Mutex::new(insets),
            monitors: vec![primary_monitor()],
            ..Default::default()
        }
    }

    pub fn state_of(&self, id: WindowId) -> Option<WindowState> {
        self.states.lock().unwrap().get(&id).cloned()
    }
}

impl PlatformWindowManager for FakePlatform {
    fn monitors(&self) -> PlatformResult<Vec<MonitorInfo>> {
        Ok(self.monitors.clone())
    }
    fn enumerate_windows(&self) -> PlatformResult<Vec<WindowId>> {
        Ok(Vec::new())
    }
    fn exists(&self, id: WindowId) -> bool {
        self.identities.contains_key(&id) || self.states.lock().unwrap().contains_key(&id)
    }
    fn process_id(&self, id: WindowId) -> Option<u32> {
        self.pids.get(&id).copied()
    }
    fn owner(&self, _: WindowId) -> Option<WindowId> {
        None
    }
    fn process_windows(&self, _: u32) -> PlatformResult<Vec<WindowId>> {
        Ok(Vec::new())
    }
    fn set_position(&self, _: WindowId, _: i32, _: i32) -> PlatformResult<()> {
        Ok(())
    }
    fn identity(&self, id: WindowId) -> PlatformResult<WindowIdentity> {
        self.identities.get(&id).cloned().ok_or(PlatformError::WindowGone(id))
    }
    fn state(&self, id: WindowId) -> PlatformResult<WindowState> {
        self.state_of(id).ok_or(PlatformError::WindowGone(id))
    }
    fn is_hung(&self, _: WindowId) -> bool {
        false
    }
    fn set_bounds(&self, id: WindowId, bounds: Rect) -> PlatformResult<()> {
        self.bounds_log.lock().unwrap().push(bounds);
        if let Some(state) = self.states.lock().unwrap().get_mut(&id) {
            state.bounds = bounds;
            state.visible_bounds = bounds;
        }
        Ok(())
    }
    fn set_borderless(&self, _: WindowId) -> PlatformResult<()> {
        Ok(())
    }
    fn set_style(&self, _: WindowId, _: NativeStyle) -> PlatformResult<()> {
        Ok(())
    }
    fn restore_state(&self, id: WindowId, state: &WindowState) -> PlatformResult<()> {
        self.states.lock().unwrap().insert(id, state.clone());
        Ok(())
    }
    fn set_topmost(&self, _: WindowId, _: bool) -> PlatformResult<()> {
        Ok(())
    }
    fn set_hides_taskbar(&self, _: WindowId, _: bool) -> PlatformResult<()> {
        Ok(())
    }
    fn foreground_window(&self) -> Option<WindowId> {
        *self.foreground.lock().unwrap()
    }
    fn set_region(&self, _: WindowId, region: Option<Rect>) -> PlatformResult<()> {
        self.region_log.lock().unwrap().push(region);
        Ok(())
    }
    fn largest_child_rect(&self, id: WindowId) -> Option<Rect> {
        let insets = (*self.page_insets.lock().unwrap())?;
        let window = self.state_of(id)?.bounds;
        Some(Rect::new(
            window.left + insets.left,
            window.top + insets.top,
            window.right - insets.right,
            window.bottom - insets.bottom,
        ))
    }
    fn subscribe(&self, _: EventSink) -> PlatformResult<EventSubscription> {
        Ok(EventSubscription::new(|| {}))
    }
}

pub(crate) type Notes = Arc<Mutex<Vec<Notification>>>;

pub(crate) struct Harness {
    pub engine: Engine,
    pub notes: Notes,
    pub config_path: PathBuf,
    pub journal_path: PathBuf,
}

/// An engine over temp files. Identity is Desqu 9.9.9 so handshakes are predictable.
pub(crate) fn harness(platform: Arc<FakePlatform>) -> Harness {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("desqu-engine-test-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let config_path = dir.join("config.json");
    let journal_path = dir.join("managed-windows.json");
    let paths = EnginePaths {
        config: config_path.clone(),
        journal: journal_path.clone(),
        legacy_journal: None,
        identity: HostIdentity {
            product: Product::Desqu,
            app_version: "9.9.9".into(),
        },
    };
    let notes: Notes = Arc::new(Mutex::new(Vec::new()));
    let sink = notes.clone();
    let engine = Engine::new(platform, paths, Arc::new(move |n| sink.lock().unwrap().push(n)));
    Harness {
        engine,
        notes,
        config_path,
        journal_path,
    }
}
