//! Browser zone mode: the browser's page is placed in a Screenbound zone. The browser window moves so
//! its page lands on the zone, and a clip region hides the tabs and toolbar. Runs on the engine
//! thread. The extension asks for this with `zoneEnter` and ends it with `zoneExit`.

use super::{journal, ActivityLevel, Engine, Notification, ReleaseReason};
use crate::bridge::protocol::{ErrorCode, ReleaseReason as Wire, ServerMessage};
use crate::bridge::server::ConnId;
use crate::core::browser::zone_window_plan;
use crate::core::fullscreen::{fullscreen_monitor, BOUNDS_TOLERANCE};
use crate::core::geometry::Rect;
use crate::core::monitor::monitor_for_rect;
use crate::core::window::{WindowId, WindowIdentity, WindowState};
use crate::core::zone::Zone;

/// The page may be a pixel or two off after a move (DPI rounding). Within this, we accept it.
const PAGE_TOLERANCE: i32 = 2;

/// A browser window we are holding in a zone.
pub(super) struct BrowserZoneWindow {
    pub id: WindowId,
    pub conn: ConnId,
    pub tab_id: i64,
    pub identity: WindowIdentity,
    pub zone_id: String,
    /// The browser's own window state, handed back on release.
    pub detected: WindowState,
    /// What we last set: window rect after our move.
    pub applied: WindowState,
    /// The zone's visible rect.
    pub target: Rect,
    /// Clip region, relative to the window.
    pub region: Rect,
}

impl Engine {
    pub(super) fn on_zone_enter(
        &mut self,
        conn: ConnId,
        request: String,
        tab_id: i64,
        tab_title: &str,
        zone_id: Option<String>,
        viewport: Option<Rect>,
    ) {
        if !self.config.enabled {
            self.error(conn, request, ErrorCode::Disabled, "Screenbound is paused".into());
            return;
        }
        let Some(window) = self.find_browser_window(conn, tab_title) else {
            self.error(
                conn,
                request,
                ErrorCode::WindowNotFound,
                "The browser window for this tab is not in the foreground".into(),
            );
            return;
        };
        // A window can be held by one owner at a time: the fullscreen limiter or a zone.
        self.release_for_takeover(window);
        self.release_browser_zone(window, Wire::Released, false);

        let zone = match zone_id.as_deref().and_then(|z| self.config.zone(z)).or_else(|| self.config.zones.first()) {
            Some(zone) => zone.clone(),
            None => {
                self.error(conn, request, ErrorCode::ZoneUnavailable, "There is no zone to use".into());
                return;
            }
        };
        let Ok(detected) = self.platform.state(window) else {
            self.error(conn, request, ErrorCode::WindowNotFound, "The browser window went away".into());
            return;
        };
        let Ok(identity) = self.platform.identity(window) else {
            self.error(conn, request, ErrorCode::WindowNotFound, "The browser window went away".into());
            return;
        };
        let target = match zone_rect(&self.monitors, &zone, &detected) {
            Ok(rect) => rect,
            Err(e) => {
                self.error(conn, request, ErrorCode::ZoneUnavailable, e);
                return;
            }
        };
        let page = viewport.or_else(|| self.platform.largest_child_rect(window));
        let Some(page) = page else {
            self.error(
                conn,
                request,
                ErrorCode::WindowNotFound,
                "The browser's page area could not be measured".into(),
            );
            return;
        };
        let (window_rect, region) = zone_window_plan(target, detected.bounds, page);

        let who = super::label(&identity, window);
        let entry = BrowserZoneWindow {
            id: window,
            conn,
            tab_id,
            identity,
            zone_id: zone.id.clone(),
            detected: detected.clone(),
            applied: detected.clone(),
            target,
            region,
        };
        // Journal first: a crash between the two steps must still put the browser back.
        self.browser_zone.insert(window, entry);
        self.save_journal();

        let applied = self
            .platform
            .set_bounds(window, window_rect)
            .and_then(|()| self.platform.set_region(window, Some(region)));
        if let Err(e) = applied {
            self.browser_zone.remove(&window);
            let _ = self.platform.set_region(window, None);
            let _ = self.platform.restore_state(window, &detected);
            self.save_journal();
            self.reply(
                conn,
                ServerMessage::ZoneApplied {
                    reply_to: request,
                    ok: false,
                    zone_name: None,
                    error: Some(format!("The browser did not accept the move: {e}")),
                },
            );
            return;
        }
        self.cover_taskbar_for_zone(window);
        // Measure the page where it is now, after the move, and fix it if the browser disagrees.
        let measured = self.platform.largest_child_rect(window);
        self.correct_browser_zone(window, measured);

        if let Some(entry) = self.browser_zone.get_mut(&window) {
            entry.applied = self.platform.state(window).unwrap_or_else(|_| entry.applied.clone());
        }
        self.save_journal();
        (self.notify)(Notification::ManagedChanged);
        self.log(
            ActivityLevel::Info,
            Some(who),
            format!("Browser page placed in zone '{}' (tab {tab_id})", zone.name),
        );
        self.reply(
            conn,
            ServerMessage::ZoneApplied {
                reply_to: request,
                ok: true,
                zone_name: Some(zone.name),
                error: None,
            },
        );
    }

    pub(super) fn on_zone_exit(&mut self, conn: ConnId, request: String, tab_id: i64) {
        if let Some(window) = self.browser_zone_for_tab(conn, tab_id) {
            self.release_browser_zone(window, Wire::Released, false);
        }
        self.reply(conn, ServerMessage::Ack { reply_to: request });
    }

    /// The browser measured its page again. If it is off the zone, move the window once more.
    pub(super) fn on_viewport_report(&mut self, conn: ConnId, request: String, tab_id: i64, viewport: Rect) {
        if let Some(window) = self.browser_zone_for_tab(conn, tab_id) {
            self.correct_browser_zone(window, Some(viewport));
            if let Some(entry) = self.browser_zone.get_mut(&window) {
                entry.applied = self.platform.state(window).unwrap_or_else(|_| entry.applied.clone());
            }
            self.save_journal();
        }
        self.reply(conn, ServerMessage::Ack { reply_to: request });
    }

    fn browser_zone_for_tab(&self, conn: ConnId, tab_id: i64) -> Option<WindowId> {
        self.browser_zone
            .values()
            .find(|z| z.conn == conn && z.tab_id == tab_id)
            .map(|z| z.id)
    }

    /// Moves the window so `page` lands on the zone. One correction only: a browser that keeps
    /// moving is left where it is, rather than fought.
    fn correct_browser_zone(&mut self, window: WindowId, page: Option<Rect>) {
        let (Some(target), Some(page)) = (self.browser_zone.get(&window).map(|z| z.target), page) else {
            return;
        };
        if page.approx_eq(&target, PAGE_TOLERANCE) {
            return;
        }
        let Ok(current) = self.platform.state(window) else {
            return;
        };
        let (window_rect, region) = zone_window_plan(target, current.bounds, page);
        if let Err(e) = self
            .platform
            .set_bounds(window, window_rect)
            .and_then(|()| self.platform.set_region(window, Some(region)))
        {
            self.log(
                ActivityLevel::Warn,
                None,
                format!("Could not correct the browser's place in the zone: {e}"),
            );
            return;
        }
        if let Some(entry) = self.browser_zone.get_mut(&window) {
            entry.region = region;
        }
    }

    /// A window is held by one owner: the fullscreen limiter or a zone. Entering a zone hands it over.
    fn release_for_takeover(&mut self, window: WindowId) {
        if self.managed.contains_key(&window) {
            self.release(window, ReleaseReason::Requested);
        }
        if self.borderless.contains_key(&window) {
            self.release_borderless(window, ReleaseReason::Requested);
        }
    }

    fn cover_taskbar_for_zone(&self, window: WindowId) {
        if let Err(e) = self.platform.set_topmost(window, true) {
            tracing::warn!(window = %window, "could not raise the browser above the taskbar: {e}");
        }
    }

    /// Gives the window back: clip lifted, original state restored. `notify` tells the browser (the
    /// extension then stays in window mode).
    pub(super) fn release_browser_zone(&mut self, window: WindowId, reason: Wire, notify: bool) {
        let Some(entry) = self.browser_zone.remove(&window) else {
            return;
        };
        let _ = self.platform.set_region(window, None);
        let who = super::label(&entry.identity, window);
        match self.platform.restore_state(window, &entry.detected) {
            Ok(()) => self.log(
                ActivityLevel::Info,
                Some(who),
                format!("Browser page released ({reason:?}); restored {}", entry.detected.bounds),
            ),
            Err(e) => self.log(
                ActivityLevel::Warn,
                Some(who),
                format!("Browser page release ({reason:?}) could not restore: {e}"),
            ),
        }
        self.restore_taskbar(window, false);
        let _ = self.platform.set_hides_taskbar(window, false);
        self.save_journal();
        (self.notify)(Notification::ManagedChanged);
        if notify {
            self.reply(
                entry.conn,
                ServerMessage::ZoneReleased {
                    tab_id: entry.tab_id,
                    reason,
                },
            );
        }
    }

    /// Releases every browser zone that belongs to a browser that just left. Nobody to tell.
    pub(super) fn release_zones_of(&mut self, conn: ConnId) {
        let ids: Vec<WindowId> = self
            .browser_zone
            .values()
            .filter(|z| z.conn == conn)
            .map(|z| z.id)
            .collect();
        for id in ids {
            self.release_browser_zone(id, Wire::Released, false);
        }
    }

    pub(super) fn release_all_browser_zones(&mut self) {
        let ids: Vec<WindowId> = self.browser_zone.keys().copied().collect();
        for id in ids {
            self.release_browser_zone(id, Wire::Disabled, true);
        }
    }

    /// The window was destroyed: nothing to restore, but the browser should know.
    pub(super) fn forget_browser_zone(&mut self, window: WindowId) {
        if let Some(entry) = self.browser_zone.remove(&window) {
            self.save_journal();
            self.reply(
                entry.conn,
                ServerMessage::ZoneReleased {
                    tab_id: entry.tab_id,
                    reason: Wire::WindowGone,
                },
            );
        }
    }

    /// Called for a held window on each native event. The browser may have moved it, or left fullscreen
    /// (F11), or the user may have resized it. Any of those gives the window back.
    pub(super) fn evaluate_browser_zone(&mut self, window: WindowId) {
        let Ok(state) = self.platform.state(window) else {
            self.forget_browser_zone(window);
            return;
        };
        if fullscreen_monitor(&state, &self.monitors).is_some() {
            self.release_browser_zone(window, Wire::Released, true);
            return;
        }
        let Some(entry) = self.browser_zone.get(&window) else {
            return;
        };
        if !state.bounds.approx_eq(&entry.applied.bounds, BOUNDS_TOLERANCE) {
            self.release_browser_zone(window, Wire::UserMoved, true);
        }
    }

    /// A user drag ends the zone; we do not keep pulling the window back.
    pub(super) fn user_moved_browser_zone(&mut self, window: WindowId) {
        self.release_browser_zone(window, Wire::UserMoved, true);
    }

    pub(super) fn browser_zone_journal(&self) -> impl Iterator<Item = journal::JournalEntry> + '_ {
        self.browser_zone.values().map(|z| journal::JournalEntry {
            kind: journal::JournalKind::BrowserZone,
            window: z.id,
            process: z.identity.process.clone(),
            process_name: z.identity.process_name.clone(),
            detected: z.detected.clone(),
            applied: z.applied.clone(),
        })
    }
}

/// Where the zone is, on the monitor the browser window is mostly on.
pub(super) fn zone_rect(
    monitors: &[crate::core::monitor::MonitorInfo],
    zone: &Zone,
    detected: &WindowState,
) -> Result<Rect, String> {
    let window_monitor = monitor_for_rect(monitors, &detected.bounds);
    let monitor = zone
        .target_monitor(monitors, window_monitor)
        .ok_or("no monitor is available for the zone")?;
    Ok(zone.resolve_on(monitor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::protocol::{ClientMessage, ServerMessage};
    use crate::engine::test_support::*;
    use crate::platform::PlatformWindowManager;
    use std::sync::mpsc;
    use std::sync::Arc;
    use std::time::Duration;

    fn connect(h: &mut Harness) -> mpsc::Receiver<ServerMessage> {
        let (tx, rx) = mpsc::channel();
        h.engine.on_browser_connected(ConnId(1), tx);
        h.engine.on_browser_message(
            ConnId(1),
            ClientMessage::HostHello {
                browser_pid: CHROME_PID,
                browser_process_name: "chrome.exe".into(),
                browser_exe_path: r"C:\Chrome\chrome.exe".into(),
                host_version: "0.1.0".into(),
            },
        );
        rx
    }

    fn next(rx: &mpsc::Receiver<ServerMessage>) -> ServerMessage {
        rx.recv_timeout(Duration::from_secs(2)).expect("a reply")
    }

    fn enter(h: &mut Harness, tab: i64, viewport: Option<Rect>) {
        h.engine.on_browser_message(
            ConnId(1),
            ClientMessage::ZoneEnter {
                id: "z".into(),
                tab_id: tab,
                tab_title: String::new(),
                zone_id: None,
                viewport,
            },
        );
    }

    /// Page at 80 px below the window top; the default zone is the left 75% of the monitor.
    fn page_below_chrome() -> Rect {
        Rect::new(100, 180, 1020, 900)
    }

    #[test]
    fn entering_a_zone_moves_the_page_onto_it_and_clips_the_chrome() {
        let platform = Arc::new(FakePlatform::chrome(Some(page_below_chrome())));
        let mut h = harness(platform.clone());
        let rx = connect(&mut h);
        enter(&mut h, 42, None);

        match next(&rx) {
            ServerMessage::ZoneApplied { reply_to, ok, zone_name, error } => {
                assert_eq!(reply_to, "z");
                assert!(ok, "zone applied: {error:?}");
                assert_eq!(zone_name.as_deref(), Some("75% Left"));
            }
            other => panic!("expected zoneApplied, got {other:?}"),
        }
        let zone = Rect::new(0, 0, 1440, 1080);
        // The page lands on the zone: the window rises by the 80 px of tabs and toolbar, and is sized to
        // the zone's width because there is no chrome on the sides.
        assert_eq!(platform.state_of(CHROME_WINDOW).unwrap().bounds, Rect::new(0, -80, 1440, 1080));
        assert_eq!(*platform.region_log.lock().unwrap().last().unwrap(), Some(Rect::new(0, 80, 1440, 1160)));
        assert_eq!(platform.largest_child_rect(CHROME_WINDOW), Some(zone));
    }

    #[test]
    fn releasing_restores_the_window_and_lifts_the_clip() {
        let platform = Arc::new(FakePlatform::chrome(Some(page_below_chrome())));
        let mut h = harness(platform.clone());
        let _rx = connect(&mut h);
        enter(&mut h, 42, None);
        assert!(platform.region_log.lock().unwrap().last().copied().flatten().is_some(), "the page is clipped");

        h.engine.on_browser_message(
            ConnId(1),
            ClientMessage::ZoneExit { id: "x".into(), tab_id: 42 },
        );
        assert_eq!(*platform.region_log.lock().unwrap().last().unwrap(), None, "the clip is lifted");
        assert_eq!(platform.state_of(CHROME_WINDOW).unwrap().bounds, Rect::new(100, 100, 1020, 900));
    }

    #[test]
    fn a_host_that_leaves_gives_the_browser_window_back() {
        let platform = Arc::new(FakePlatform::chrome(Some(page_below_chrome())));
        let mut h = harness(platform.clone());
        let _rx = connect(&mut h);
        enter(&mut h, 42, None);
        h.engine.on_browser_disconnected(ConnId(1));
        assert_eq!(*platform.region_log.lock().unwrap().last().unwrap(), None);
        assert!(h.engine.browser_zone.is_empty());
    }

    #[test]
    fn a_user_move_ends_the_zone_and_tells_the_browser() {
        let platform = Arc::new(FakePlatform::chrome(Some(page_below_chrome())));
        let mut h = harness(platform.clone());
        let rx = connect(&mut h);
        enter(&mut h, 42, None);
        let _ = next(&rx); // zoneApplied
        h.engine.user_moved_browser_zone(CHROME_WINDOW);
        match next(&rx) {
            ServerMessage::ZoneReleased { tab_id, reason } => {
                assert_eq!(tab_id, 42);
                assert_eq!(reason, Wire::UserMoved);
            }
            other => panic!("expected zoneReleased, got {other:?}"),
        }
    }

    #[test]
    fn the_journal_holds_the_zone_before_the_window_moves() {
        let platform = Arc::new(FakePlatform::chrome(Some(page_below_chrome())));
        let mut h = harness(platform.clone());
        let _rx = connect(&mut h);
        enter(&mut h, 42, None);
        let text = std::fs::read_to_string(&h.journal_path).unwrap();
        assert!(text.contains("browserZone"), "the journal records the zone: {text}");
    }

    #[test]
    fn zone_mode_needs_the_browser_window_in_front() {
        let fake = FakePlatform::chrome(Some(page_below_chrome()));
        *fake.foreground.lock().unwrap() = None;
        let platform = Arc::new(fake);
        let mut h = harness(platform);
        let rx = connect(&mut h);
        enter(&mut h, 42, None);
        match next(&rx) {
            ServerMessage::Error { code, .. } => assert_eq!(code, ErrorCode::WindowNotFound),
            other => panic!("expected error, got {other:?}"),
        }
    }

    #[test]
    fn disabling_screenbound_releases_every_zone() {
        let platform = Arc::new(FakePlatform::chrome(Some(page_below_chrome())));
        let mut h = harness(platform.clone());
        let rx = connect(&mut h);
        enter(&mut h, 42, None);
        let _ = next(&rx);
        let mut config = h.engine.config();
        config.enabled = false;
        h.engine.set_config(config).unwrap();
        assert!(h.engine.browser_zone.is_empty());
    }
}
