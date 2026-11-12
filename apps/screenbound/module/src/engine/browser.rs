//! Browser bridge, engine side. Runs on the engine thread: what the browser asks for, and what
//! Screenbound tells it. The transport lives in `bridge/`.

use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use serde::Serialize;

use super::handle::EngineHandle;
use super::{activity, ActivityLevel, Engine, Notification};
use crate::bridge::protocol::{
    BrowserInfo, ClientMessage, ErrorCode, Product, ServerMessage, ZoneRef, PROTOCOL_VERSION,
};
use crate::bridge::server::{BridgeSink, ConnId};
use crate::core::browser::SiteRules;
use crate::core::window::WindowId;

/// How long a "leave this window alone" request holds. The page asks just before it goes fullscreen.
const EXEMPT_FOR: Duration = Duration::from_secs(5);
/// After this long outside fullscreen, an exemption is stale and is dropped.
const EXEMPT_STALE_AFTER: Duration = Duration::from_secs(1);

/// Who this process is, for the `helloAck`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostIdentity {
    pub product: Product,
    pub app_version: String,
}

#[derive(Debug, Clone)]
pub struct HostInfo {
    pub browser_pid: u32,
    pub process_name: String,
    pub exe_path: String,
}

#[derive(Debug, Clone)]
pub struct HelloInfo {
    pub browser_name: String,
    pub browser_version: String,
    pub extension_version: String,
}

pub(super) struct BrowserConn {
    outbox: Sender<ServerMessage>,
    host: Option<HostInfo>,
    hello: Option<HelloInfo>,
    since_ms: u64,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Exemption {
    since: Instant,
    until: Instant,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserConnInfo {
    pub conn_id: u64,
    pub process_name: String,
    pub browser_name: String,
    pub browser_version: String,
    pub extension_version: String,
    pub since_ms: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStatus {
    pub integration_enabled: bool,
    /// Browsers that have said hello (the extension is connected).
    pub connections: Vec<BrowserConnInfo>,
}

/// Forwards bridge events onto the engine thread.
pub struct EngineSink(pub EngineHandle);

impl BridgeSink for EngineSink {
    fn connected(&self, id: ConnId, outbox: Sender<ServerMessage>) {
        self.0.post(move |e| e.on_browser_connected(id, outbox));
    }

    fn message(&self, id: ConnId, message: ClientMessage) {
        self.0.post(move |e| e.on_browser_message(id, message));
    }

    fn disconnected(&self, id: ConnId) {
        self.0.post(move |e| e.on_browser_disconnected(id));
    }
}

impl Engine {
    pub fn on_browser_connected(&mut self, conn: ConnId, outbox: Sender<ServerMessage>) {
        self.browser_conns.insert(
            conn,
            BrowserConn {
                outbox,
                host: None,
                hello: None,
                since_ms: activity::now_ms(),
            },
        );
    }

    pub fn on_browser_message(&mut self, conn: ConnId, message: ClientMessage) {
        if !self.browser_conns.contains_key(&conn) {
            return;
        }
        match message {
            ClientMessage::HostHello {
                browser_pid,
                browser_process_name,
                browser_exe_path,
                ..
            } => {
                if let Some(c) = self.browser_conns.get_mut(&conn) {
                    c.host = Some(HostInfo {
                        browser_pid,
                        process_name: browser_process_name,
                        exe_path: browser_exe_path,
                    });
                }
            }
            ClientMessage::Hello {
                id,
                protocol,
                extension_version,
                browser,
                rules,
            } => self.on_hello(conn, id, protocol, extension_version, browser, rules),
            ClientMessage::SetRules { id, rules } => self.on_set_rules(conn, id, rules),
            ClientMessage::ExemptNext { id, tab_title } => self.on_exempt_next(conn, id, &tab_title),
            ClientMessage::ZoneEnter { id, .. } => self.reply(
                conn,
                ServerMessage::ZoneApplied {
                    reply_to: id,
                    ok: false,
                    zone_name: None,
                    error: Some("Zone mode is not available in this version".into()),
                },
            ),
            ClientMessage::ViewportReport { id, .. } | ClientMessage::ZoneExit { id, .. } => {
                self.reply(conn, ServerMessage::Ack { reply_to: id })
            }
            ClientMessage::FullscreenNative { host, .. } => {
                self.log(
                    ActivityLevel::Info,
                    None,
                    format!("Browser fullscreen on {host} (left to the browser)"),
                );
            }
        }
        self.notify_browser_changed();
    }

    pub fn on_browser_disconnected(&mut self, conn: ConnId) {
        if self.browser_conns.remove(&conn).is_some() {
            self.notify_browser_changed();
        }
    }

    fn on_hello(
        &mut self,
        conn: ConnId,
        request: String,
        protocol: u32,
        extension_version: String,
        browser: BrowserInfo,
        rules: SiteRules,
    ) {
        if protocol != PROTOCOL_VERSION {
            self.error(
                conn,
                request,
                ErrorCode::UnsupportedProtocol,
                format!("This Screenbound speaks protocol {PROTOCOL_VERSION}; the extension sent {protocol}"),
            );
            return;
        }
        if let Some(c) = self.browser_conns.get_mut(&conn) {
            c.hello = Some(HelloInfo {
                browser_name: browser.name,
                browser_version: browser.version,
                extension_version,
            });
        }

        // Same rule as the extension (`mergeRules`): the strictly newer copy wins, ties keep Screenbound's.
        let extension_wins = rules.updated_at > self.config.browser.site_rules.updated_at;
        if extension_wins {
            let mut config = self.config.clone();
            config.browser.site_rules = rules;
            if let Err(e) = self.set_config(config) {
                self.error(conn, request, ErrorCode::BadRequest, e);
                return;
            }
            let rules = self.config.browser.site_rules.clone();
            self.broadcast_except(conn, ServerMessage::RulesChanged { rules });
        }

        let rules = self.config.browser.site_rules.clone();
        let zones = self.zone_refs();
        self.reply(
            conn,
            ServerMessage::HelloAck {
                reply_to: request,
                protocol: PROTOCOL_VERSION,
                product: self.identity.product,
                app_version: self.identity.app_version.clone(),
                zones,
                rules,
            },
        );
    }

    fn on_set_rules(&mut self, conn: ConnId, request: String, rules: SiteRules) {
        let mut config = self.config.clone();
        config.browser.site_rules = rules;
        match self.set_config(config) {
            Ok(saved) => {
                let rules = saved.browser.site_rules;
                self.broadcast_except(conn, ServerMessage::RulesChanged { rules });
                self.reply(conn, ServerMessage::Ack { reply_to: request });
            }
            Err(e) => self.error(conn, request, ErrorCode::BadRequest, e),
        }
    }

    fn on_exempt_next(&mut self, conn: ConnId, request: String, tab_title: &str) {
        let Some(window) = self.find_browser_window(conn, tab_title) else {
            self.error(
                conn,
                request,
                ErrorCode::WindowNotFound,
                "The browser window that asked for fullscreen is not in the foreground".into(),
            );
            return;
        };
        let now = Instant::now();
        self.exempt.insert(
            window,
            Exemption {
                since: now,
                until: now + EXEMPT_FOR,
            },
        );
        let who = self.browser_conns.get(&conn).and_then(|c| c.host.as_ref()).map(|h| h.process_name.clone());
        self.log(
            ActivityLevel::Info,
            who,
            "Left alone: the browser asked for real fullscreen".into(),
        );
        self.reply(conn, ServerMessage::Ack { reply_to: request });
    }

    /// The foreground window, if it belongs to this browser and (when given) shows the tab's title.
    fn find_browser_window(&self, conn: ConnId, tab_title: &str) -> Option<WindowId> {
        let host = self.browser_conns.get(&conn)?.host.clone()?;
        let window = self.platform.foreground_window()?;
        let identity = self.platform.identity(window).ok()?;
        let same_process = self.platform.process_id(window) == Some(host.browser_pid);
        let same_name = identity.process_name.eq_ignore_ascii_case(&host.process_name);
        if !(same_process || same_name) {
            return None;
        }
        let needle = tab_title.trim().to_lowercase();
        if !needle.is_empty() && !identity.title.to_lowercase().contains(&needle) {
            return None;
        }
        Some(window)
    }

    /// True while a browser has asked that this window is not constrained.
    pub fn is_exempt(&self, window: WindowId, now: Instant) -> bool {
        self.exempt.get(&window).is_some_and(|e| now < e.until)
    }

    /// Called when a window is seen outside fullscreen. Drops exemptions that never got used.
    pub(super) fn forget_stale_exemption(&mut self, window: WindowId, now: Instant) {
        if let Some(e) = self.exempt.get(&window) {
            if now.duration_since(e.since) >= EXEMPT_STALE_AFTER {
                self.exempt.remove(&window);
            }
        }
    }

    pub(super) fn prune_exemptions(&mut self, now: Instant) {
        self.exempt.retain(|_, e| now < e.until);
    }

    pub fn browser_status(&self) -> BrowserStatus {
        let mut connections: Vec<BrowserConnInfo> = self
            .browser_conns
            .iter()
            .filter_map(|(id, c)| {
                let hello = c.hello.as_ref()?;
                Some(BrowserConnInfo {
                    conn_id: id.0,
                    process_name: c.host.as_ref().map(|h| h.process_name.clone()).unwrap_or_default(),
                    browser_name: hello.browser_name.clone(),
                    browser_version: hello.browser_version.clone(),
                    extension_version: hello.extension_version.clone(),
                    since_ms: c.since_ms,
                })
            })
            .collect();
        connections.sort_by_key(|c| c.conn_id);
        BrowserStatus {
            integration_enabled: self.config.browser.integration_enabled,
            connections,
        }
    }

    fn zone_refs(&self) -> Vec<ZoneRef> {
        self.config
            .zones
            .iter()
            .map(|z| ZoneRef {
                id: z.id.clone(),
                name: z.name.clone(),
            })
            .collect()
    }

    fn reply(&self, conn: ConnId, message: ServerMessage) {
        if let Some(c) = self.browser_conns.get(&conn) {
            let _ = c.outbox.send(message);
        }
    }

    fn error(&self, conn: ConnId, request: String, code: ErrorCode, message: String) {
        self.reply(
            conn,
            ServerMessage::Error {
                reply_to: request,
                code,
                message,
            },
        );
    }

    fn broadcast_except(&self, except: ConnId, message: ServerMessage) {
        for (id, c) in &self.browser_conns {
            if *id != except {
                let _ = c.outbox.send(message.clone());
            }
        }
    }

    fn notify_browser_changed(&self) {
        (self.notify)(Notification::BrowserChanged);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::browser::{FullscreenMode, SitePattern, SiteRule};
    use crate::core::geometry::Rect;
    use crate::core::monitor::MonitorInfo;
    use crate::core::window::{ProcessRef, WindowIdentity, WindowState};
    use crate::engine::EnginePaths;
    use crate::platform::{EventSubscription, EventSink, PlatformError, PlatformResult, PlatformWindowManager};
    use std::collections::HashMap as Map;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{mpsc, Arc, Mutex};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// Just enough of a platform for the browser handlers: foreground window and identities.
    #[derive(Default)]
    struct FakePlatform {
        foreground: Mutex<Option<WindowId>>,
        identities: Map<WindowId, WindowIdentity>,
        pids: Map<WindowId, u32>,
    }

    impl PlatformWindowManager for FakePlatform {
        fn monitors(&self) -> PlatformResult<Vec<MonitorInfo>> {
            Ok(Vec::new())
        }
        fn enumerate_windows(&self) -> PlatformResult<Vec<WindowId>> {
            Ok(Vec::new())
        }
        fn exists(&self, id: WindowId) -> bool {
            self.identities.contains_key(&id)
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
            Err(PlatformError::WindowGone(id))
        }
        fn is_hung(&self, _: WindowId) -> bool {
            false
        }
        fn set_bounds(&self, _: WindowId, _: Rect) -> PlatformResult<()> {
            Ok(())
        }
        fn set_borderless(&self, _: WindowId) -> PlatformResult<()> {
            Ok(())
        }
        fn set_style(&self, _: WindowId, _: crate::core::window::NativeStyle) -> PlatformResult<()> {
            Ok(())
        }
        fn restore_state(&self, _: WindowId, _: &WindowState) -> PlatformResult<()> {
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
        fn subscribe(&self, _: EventSink) -> PlatformResult<EventSubscription> {
            Ok(EventSubscription::new(|| {}))
        }
    }

    const CHROME_PID: u32 = 4242;
    const CHROME_WINDOW: WindowId = WindowId(7);

    fn chrome_platform(foreground: Option<WindowId>) -> FakePlatform {
        let mut identities = Map::new();
        identities.insert(
            CHROME_WINDOW,
            WindowIdentity {
                process: ProcessRef { pid: CHROME_PID, start_time: 1 },
                process_name: "chrome.exe".into(),
                executable_path: r"C:\Chrome\chrome.exe".into(),
                class_name: "Chrome_WidgetWin_1".into(),
                title: "Big Buck Bunny - Google Chrome".into(),
            },
        );
        let mut pids = Map::new();
        pids.insert(CHROME_WINDOW, CHROME_PID);
        FakePlatform {
            foreground: Mutex::new(foreground),
            identities,
            pids,
        }
    }

    struct Harness {
        engine: Engine,
        notes: Arc<Mutex<Vec<Notification>>>,
        config_path: std::path::PathBuf,
    }

    fn harness(platform: FakePlatform) -> Harness {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("desqu-browser-test-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("config.json");
        let paths = EnginePaths {
            config: config_path.clone(),
            journal: dir.join("managed-windows.json"),
            legacy_journal: None,
            identity: HostIdentity {
                product: Product::Desqu,
                app_version: "9.9.9".into(),
            },
        };
        let notes = Arc::new(Mutex::new(Vec::new()));
        let sink = notes.clone();
        let engine = Engine::new(
            Arc::new(platform),
            paths,
            Arc::new(move |n| sink.lock().unwrap().push(n)),
        );
        Harness {
            engine,
            notes,
            config_path,
        }
    }

    fn connect(h: &mut Harness, conn: u64, pid: u32, process: &str) -> mpsc::Receiver<ServerMessage> {
        let (tx, rx) = mpsc::channel();
        h.engine.on_browser_connected(ConnId(conn), tx);
        h.engine.on_browser_message(
            ConnId(conn),
            ClientMessage::HostHello {
                browser_pid: pid,
                browser_process_name: process.into(),
                browser_exe_path: format!(r"C:\{process}"),
                host_version: "0.1.0".into(),
            },
        );
        rx
    }

    fn hello(rules: SiteRules) -> ClientMessage {
        ClientMessage::Hello {
            id: "1".into(),
            protocol: PROTOCOL_VERSION,
            extension_version: "0.1.0".into(),
            browser: BrowserInfo {
                name: "Chrome".into(),
                version: "130".into(),
            },
            rules,
        }
    }

    fn rules_at(updated_at: u64, mode: FullscreenMode) -> SiteRules {
        SiteRules {
            updated_at,
            default_mode: mode,
            rules: vec![SiteRule {
                id: "r".into(),
                enabled: true,
                pattern: SitePattern::Domain("example.com".into()),
                mode,
                zone_id: None,
            }],
            ..SiteRules::default()
        }
    }

    fn next(rx: &mpsc::Receiver<ServerMessage>) -> ServerMessage {
        rx.recv_timeout(Duration::from_secs(2)).expect("a reply")
    }

    #[test]
    fn a_newer_extension_copy_is_saved_and_acked_with_the_merged_rules() {
        let mut h = harness(chrome_platform(None));
        let rx = connect(&mut h, 1, CHROME_PID, "chrome.exe");
        h.engine.on_browser_message(ConnId(1), hello(rules_at(500, FullscreenMode::Window)));

        match next(&rx) {
            ServerMessage::HelloAck { reply_to, protocol, product, app_version, rules, zones } => {
                assert_eq!(reply_to, "1");
                assert_eq!(protocol, PROTOCOL_VERSION);
                assert_eq!(product, Product::Desqu);
                assert_eq!(app_version, "9.9.9");
                assert_eq!(rules.default_mode, FullscreenMode::Window);
                assert_eq!(zones.len(), 1, "the default zone is offered");
            }
            other => panic!("expected helloAck, got {other:?}"),
        }
        assert_eq!(h.engine.config().browser.site_rules.default_mode, FullscreenMode::Window);
        assert!(std::fs::read_to_string(&h.config_path).unwrap().contains("example.com"));
    }

    #[test]
    fn an_older_extension_copy_is_replaced_by_screenbounds_rules() {
        let mut h = harness(chrome_platform(None));
        // Screenbound already holds newer rules, saved from its own UI.
        let _setup = connect(&mut h, 0, 1, "setup.exe");
        h.engine.on_browser_message(
            ConnId(0),
            ClientMessage::SetRules { id: "x".into(), rules: rules_at(900, FullscreenMode::Zone) },
        );
        let rx = connect(&mut h, 1, CHROME_PID, "chrome.exe");
        h.engine.on_browser_message(ConnId(1), hello(rules_at(100, FullscreenMode::Window)));

        match next(&rx) {
            ServerMessage::HelloAck { rules, .. } => assert_eq!(rules.default_mode, FullscreenMode::Zone),
            other => panic!("expected helloAck, got {other:?}"),
        }
    }

    #[test]
    fn a_different_protocol_version_is_refused() {
        let mut h = harness(chrome_platform(None));
        let rx = connect(&mut h, 1, CHROME_PID, "chrome.exe");
        h.engine.on_browser_message(
            ConnId(1),
            ClientMessage::Hello {
                id: "9".into(),
                protocol: 2,
                extension_version: "9".into(),
                browser: BrowserInfo { name: "x".into(), version: "x".into() },
                rules: SiteRules::default(),
            },
        );
        match next(&rx) {
            ServerMessage::Error { reply_to, code, .. } => {
                assert_eq!(reply_to, "9");
                assert_eq!(code, ErrorCode::UnsupportedProtocol);
            }
            other => panic!("expected error, got {other:?}"),
        }
    }

    #[test]
    fn saved_rules_reach_the_other_browsers_but_not_the_sender() {
        let mut h = harness(chrome_platform(None));
        let sender = connect(&mut h, 1, CHROME_PID, "chrome.exe");
        let other = connect(&mut h, 2, 5555, "brave.exe");
        h.engine.on_browser_message(
            ConnId(1),
            ClientMessage::SetRules { id: "s".into(), rules: rules_at(42, FullscreenMode::Window) },
        );

        match next(&other) {
            ServerMessage::RulesChanged { rules } => assert_eq!(rules.updated_at, 42),
            other => panic!("expected rulesChanged, got {other:?}"),
        }
        assert!(matches!(next(&sender), ServerMessage::Ack { .. }));
        assert!(sender.try_recv().is_err(), "the sender gets only its ack");
    }

    #[test]
    fn exempt_next_marks_the_browser_window_in_the_foreground() {
        let mut h = harness(chrome_platform(Some(CHROME_WINDOW)));
        let rx = connect(&mut h, 1, CHROME_PID, "chrome.exe");
        h.engine.on_browser_message(
            ConnId(1),
            ClientMessage::ExemptNext { id: "e".into(), tab_title: "Big Buck Bunny".into() },
        );
        assert!(matches!(next(&rx), ServerMessage::Ack { .. }));
        assert!(h.engine.is_exempt(CHROME_WINDOW, Instant::now()));
        assert!(!h.engine.is_exempt(CHROME_WINDOW, Instant::now() + EXEMPT_FOR + Duration::from_millis(1)));
    }

    #[test]
    fn exempt_next_is_refused_when_no_browser_window_is_in_front() {
        let mut h = harness(chrome_platform(None));
        let rx = connect(&mut h, 1, CHROME_PID, "chrome.exe");
        h.engine.on_browser_message(
            ConnId(1),
            ClientMessage::ExemptNext { id: "e".into(), tab_title: String::new() },
        );
        match next(&rx) {
            ServerMessage::Error { code, .. } => assert_eq!(code, ErrorCode::WindowNotFound),
            other => panic!("expected error, got {other:?}"),
        }
        assert!(!h.engine.is_exempt(CHROME_WINDOW, Instant::now()));
    }

    #[test]
    fn a_foreground_window_of_another_app_is_not_exempted() {
        let mut platform = chrome_platform(Some(CHROME_WINDOW));
        // Same window, but the connected host is some other browser.
        platform.pids.insert(CHROME_WINDOW, 1);
        let mut h = harness(platform);
        let rx = connect(&mut h, 1, 9999, "msedge.exe");
        h.engine.on_browser_message(
            ConnId(1),
            ClientMessage::ExemptNext { id: "e".into(), tab_title: String::new() },
        );
        assert!(matches!(next(&rx), ServerMessage::Error { code: ErrorCode::WindowNotFound, .. }));
    }

    #[test]
    fn a_disconnect_forgets_the_browser_and_says_so() {
        let mut h = harness(chrome_platform(None));
        let rx = connect(&mut h, 1, CHROME_PID, "chrome.exe");
        h.engine.on_browser_message(ConnId(1), hello(SiteRules::default()));
        assert!(matches!(next(&rx), ServerMessage::HelloAck { .. }));
        assert_eq!(h.engine.browser_status().connections.len(), 1);

        h.engine.on_browser_disconnected(ConnId(1));
        assert!(h.engine.browser_status().connections.is_empty());
        let notes = h.notes.lock().unwrap();
        assert!(notes.iter().any(|n| matches!(n, Notification::BrowserChanged)));
    }

    #[test]
    fn a_stale_exemption_is_dropped_once_the_window_is_not_fullscreen() {
        let mut h = harness(chrome_platform(Some(CHROME_WINDOW)));
        let _rx = connect(&mut h, 1, CHROME_PID, "chrome.exe");
        h.engine.on_browser_message(
            ConnId(1),
            ClientMessage::ExemptNext { id: "e".into(), tab_title: String::new() },
        );
        let later = Instant::now() + EXEMPT_STALE_AFTER + Duration::from_millis(10);
        h.engine.forget_stale_exemption(CHROME_WINDOW, later);
        assert!(!h.engine.is_exempt(CHROME_WINDOW, Instant::now()));
    }
}
