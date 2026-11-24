//! The zone hotkey: the window in front goes into the zone once.

use std::sync::Arc;

use super::test_support::*;
use crate::core::geometry::Rect;

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
