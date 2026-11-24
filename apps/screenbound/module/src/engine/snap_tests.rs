//! The zone hotkey: the window in front goes into the zone once.

use std::sync::Arc;

use super::test_support::*;
use crate::core::config::KnownApp;
use crate::core::geometry::Rect;
use crate::platform::PlatformEvent;

#[test]
fn the_window_in_front_is_put_into_the_zone_and_a_missing_window_is_refused() {
    let platform = Arc::new(FakePlatform::chrome(None));
    let mut h = harness(platform.clone());
    let note = h.engine.snap_foreground_to_zone().expect("the window moves");
    assert!(note.contains("75% Left"), "{note}");
    // The default zone is the left three quarters of the monitor.
    assert_eq!(platform.state_of(CHROME_WINDOW).unwrap().bounds, Rect::new(0, 0, 1440, 1080));

    *platform.foreground.lock().unwrap() = None;
    assert!(h.engine.snap_foreground_to_zone().is_err());
}

#[test]
fn a_game_in_exclusive_fullscreen_is_left_alone() {
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    let run = |game: bool| {
        let platform = Arc::new(FakePlatform::chrome(None));
        // The window fills the monitor, as a window that just went fullscreen does.
        platform.states.lock().unwrap().get_mut(&CHROME_WINDOW).unwrap().bounds = Rect::new(0, 0, 1920, 1080);
        platform.states.lock().unwrap().get_mut(&CHROME_WINDOW).unwrap().visible_bounds = Rect::new(0, 0, 1920, 1080);
        {
            let mut states = platform.states.lock().unwrap();
            let state = states.get_mut(&CHROME_WINDOW).unwrap();
            state.has_title_bar = false;
            state.has_resize_frame = false;
        }
        platform.game.store(game, Ordering::Relaxed);
        let mut h = harness(platform);
        // Chrome is one of the apps that are turned on.
        let mut config = h.engine.config();
        config.known_apps.push(KnownApp { process_name: "chrome.exe".into(), title: String::new(), enabled: true, group_id: None });
        h.engine.set_config(config).unwrap();
        h.engine.startup();
        h.engine.on_platform_event(PlatformEvent::WindowChanged(CHROME_WINDOW));
        h.engine.process_due(Instant::now() + Duration::from_secs(30));
        h.engine.status().managed.len()
    };
    assert_eq!(run(true), 0, "a game is not touched");
    assert_eq!(run(false), 1, "the same window without a game is put in the zone");
}
