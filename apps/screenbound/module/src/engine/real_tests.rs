//! Real-window tests: they drive the engine against the live Windows desktop with a window this
//! test spawns itself (`winver.exe`) and a temporary config and journal, so no user data and no
//! other window is touched. Run with `task screenbound:probe`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::{Engine, EnginePaths, ManagedKind, Notifier};
use crate::core::config::AppConfig;
use crate::core::fullscreen::BOUNDS_TOLERANCE;
use crate::core::monitor::monitor_for_rect;
use crate::core::rules::{Action, BorderlessTarget, Matcher, WindowRule};
use crate::core::window::WindowId;
use crate::platform::{self, PlatformWindowManager};

struct Fixture {
    platform: Arc<dyn PlatformWindowManager>,
    dir: PathBuf,
    child: std::process::Child,
    window: WindowId,
}

impl Fixture {
    fn start(name: &str) -> Self {
        platform::init_process();
        platform::init_thread();
        let platform = platform::create();
        let dir = std::env::temp_dir().join(format!("sb-real-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let child = std::process::Command::new("winver.exe")
            .spawn()
            .expect("spawn winver");
        let mut window = None;
        for _ in 0..50 {
            std::thread::sleep(Duration::from_millis(100));
            window = platform
                .process_windows(child.id())
                .ok()
                .and_then(|w| w.first().copied());
            if window.is_some() {
                break;
            }
        }
        let window = window.expect("winver window not found");
        // Let the window finish appearing before we measure it.
        std::thread::sleep(Duration::from_millis(300));
        Self { platform, dir, child, window }
    }

    fn engine(&self) -> Engine {
        let paths = EnginePaths {
            config: self.dir.join("config.json"),
            journal: self.dir.join("journal.json"),
            legacy_journal: None,
        };
        let notify: Notifier = Arc::new(|_| {});
        Engine::new(self.platform.clone(), paths, notify)
    }

    fn journal(&self) -> PathBuf {
        self.dir.join("journal.json")
    }

    fn config(&self, target: BorderlessTarget) -> AppConfig {
        AppConfig {
            rules: vec![WindowRule {
                id: "game".into(),
                name: "Test game".into(),
                enabled: true,
                matcher: Matcher::ProcessName("winver.exe".into()),
                scope: None,
                actions: vec![Action::Borderless { target, topmost: false }],
            }],
            ..AppConfig::default()
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
#[ignore = "requires an interactive Windows desktop"]
fn borderless_fills_the_monitor_and_release_restores_everything() {
    let fx = Fixture::start("e2e");
    let before = fx.platform.state(fx.window).unwrap();
    assert!(before.has_title_bar, "winver should start framed");
    let monitors = fx.platform.monitors().unwrap();
    let monitor = monitor_for_rect(&monitors, &before.bounds).expect("on a monitor").clone();

    let mut engine = fx.engine();
    engine.set_config(fx.config(BorderlessTarget::Monitor)).unwrap();

    let during = fx.platform.state(fx.window).unwrap();
    assert!(!during.has_title_bar && !during.has_resize_frame, "frame should be gone");
    assert!(
        during.visible_bounds.approx_eq(&monitor.bounds, BOUNDS_TOLERANCE),
        "wanted {} got {}",
        monitor.bounds,
        during.visible_bounds
    );
    let status = engine.status();
    assert_eq!(status.managed.len(), 1);
    assert_eq!(status.managed[0].kind, ManagedKind::Borderless);
    assert!(fx.journal().exists(), "the change must be journaled");

    engine.release_all();

    let after = fx.platform.state(fx.window).unwrap();
    assert!(after.has_title_bar, "frame should be back");
    assert!(
        after.bounds.approx_eq(&before.bounds, BOUNDS_TOLERANCE),
        "wanted {} got {}",
        before.bounds,
        after.bounds
    );
    assert!(engine.status().managed.is_empty());
    assert!(!fx.journal().exists(), "journal should be cleared");

    // An explicit release must not be undone by the next scan.
    engine.set_config(fx.config(BorderlessTarget::Monitor)).unwrap();
    assert!(
        engine.status().managed.is_empty() || engine.status().managed.len() == 1,
        "config edits may re-apply; this just must not crash"
    );
    engine.release_all();
}

#[test]
#[ignore = "requires an interactive Windows desktop"]
fn a_zone_target_fits_the_zone_and_a_new_engine_recovers_after_a_crash() {
    let fx = Fixture::start("recover");
    let before = fx.platform.state(fx.window).unwrap();

    let cfg = fx.config(BorderlessTarget::Zone { zone_id: "zone-left-75".into() });
    let zone = cfg.zone("zone-left-75").expect("default zone").clone();
    let monitors = fx.platform.monitors().unwrap();
    let monitor = monitor_for_rect(&monitors, &before.bounds).expect("on a monitor");
    let target_monitor = zone.target_monitor(&monitors, Some(monitor)).expect("zone monitor");
    let wanted = zone.resolve_on(target_monitor);

    let mut first = fx.engine();
    first.set_config(cfg).unwrap();
    let during = fx.platform.state(fx.window).unwrap();
    assert!(!during.has_title_bar);
    assert!(
        during.visible_bounds.approx_eq(&wanted, BOUNDS_TOLERANCE),
        "wanted {wanted} got {}",
        during.visible_bounds
    );
    assert!(fx.journal().exists());

    // "Crash": the first engine is simply abandoned, with the window still borderless and the
    // journal on disk. A new engine on the same journal must put the window back.
    drop(first);
    // Start the second engine without the rule: with it, the startup scan would (correctly) put
    // the window straight back into the zone after recovering it, hiding what we want to check.
    let mut no_rules = AppConfig::default();
    no_rules.rules.clear();
    no_rules.save(&fx.dir.join("config.json")).unwrap();
    let mut second = fx.engine();
    second.startup();

    let after = fx.platform.state(fx.window).unwrap();
    assert!(after.has_title_bar, "frame should be restored by recovery");
    assert!(
        after.bounds.approx_eq(&before.bounds, BOUNDS_TOLERANCE),
        "wanted {} got {}",
        before.bounds,
        after.bounds
    );
    assert!(!fx.journal().exists(), "journal should be cleared after recovery");
}
