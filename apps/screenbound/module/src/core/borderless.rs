//! Platform-independent decisions for borderless windows (a framed window turned borderless and
//! fitted to a monitor or zone). Platforms supply `WindowState` snapshots; what they mean lives
//! here so it can be unit tested.

use super::fullscreen::{is_fullscreen_like, BOUNDS_TOLERANCE};
use super::monitor::MonitorInfo;
use super::window::{ShowState, WindowState};

/// Should a window that matched a borderless rule be turned borderless now?
///
/// Only visible, normal-looking, *framed* top-level windows qualify:
/// - not minimized, hidden or cloaked, and not a click-through overlay,
/// - with a title bar or resize frame (otherwise there is nothing to remove, and a borderless
///   window that covers a monitor is the fullscreen zone feature's job),
/// - not owned by another window (dialogs, launchers, tool panels),
/// - with a real size.
pub fn should_apply(state: &WindowState, has_owner: bool, monitors: &[MonitorInfo]) -> bool {
    state.visible
        && !state.cloaked
        && !state.click_through
        && state.show_state != ShowState::Minimized
        && (state.has_title_bar || state.has_resize_frame)
        && !has_owner
        && !state.bounds.is_empty()
        && !is_fullscreen_like(state, monitors)
}

/// What a state change on a window we made borderless means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderlessObservation {
    /// Still exactly as we left it (typically our own SetWindowPos echo).
    Unchanged,
    /// Minimized/hidden/cloaked: keep tracking, do nothing now.
    Dormant,
    /// The app moved or re-framed itself: put it back.
    Reassert,
    /// The app went fullscreen on its own: stop managing and leave it as the app made it.
    AppFullscreen,
}

/// `target_is_monitor`: the target is the whole monitor, so a fullscreen-like window is expected
/// and only a *change* (e.g. another monitor) counts as the app acting.
pub fn observe(
    current: &WindowState,
    applied: &WindowState,
    monitors: &[MonitorInfo],
    target_is_monitor: bool,
) -> BorderlessObservation {
    if !current.visible || current.cloaked || current.show_state == ShowState::Minimized {
        return BorderlessObservation::Dormant;
    }
    // Compare chrome flags rather than raw styles: apps toggle state bits like WS_VISIBLE
    // transiently, which must not read as a change.
    if current.bounds.approx_eq(&applied.bounds, BOUNDS_TOLERANCE)
        && current.has_title_bar == applied.has_title_bar
        && current.has_resize_frame == applied.has_resize_frame
        && current.show_state == applied.show_state
    {
        return BorderlessObservation::Unchanged;
    }
    if !target_is_monitor && is_fullscreen_like(current, monitors) {
        return BorderlessObservation::AppFullscreen;
    }
    BorderlessObservation::Reassert
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::fullscreen::test_state;
    use crate::core::geometry::Rect;
    use crate::core::monitor::test_monitor;

    fn monitor() -> MonitorInfo {
        test_monitor("m1", Rect::new(0, 0, 2560, 1440))
    }

    fn framed(bounds: Rect) -> WindowState {
        WindowState {
            has_title_bar: true,
            has_resize_frame: true,
            ..test_state(bounds)
        }
    }

    #[test]
    fn a_framed_window_qualifies() {
        let s = framed(Rect::new(100, 100, 1380, 820));
        assert!(should_apply(&s, false, &[monitor()]));
    }

    #[test]
    fn minimized_hidden_cloaked_and_click_through_windows_do_not() {
        let m = [monitor()];
        let base = framed(Rect::new(100, 100, 1380, 820));
        let minimized = WindowState { show_state: ShowState::Minimized, ..base.clone() };
        let hidden = WindowState { visible: false, ..base.clone() };
        let cloaked = WindowState { cloaked: true, ..base.clone() };
        let overlay = WindowState { click_through: true, ..base.clone() };
        for s in [minimized, hidden, cloaked, overlay] {
            assert!(!should_apply(&s, false, &m));
        }
    }

    #[test]
    fn owned_windows_and_frameless_windows_do_not() {
        let m = [monitor()];
        let s = framed(Rect::new(100, 100, 1380, 820));
        assert!(!should_apply(&s, true, &m), "dialogs and launchers are owned");
        let frameless = test_state(Rect::new(100, 100, 1380, 820));
        assert!(!should_apply(&frameless, false, &m), "nothing to remove");
    }

    #[test]
    fn empty_bounds_do_not() {
        let s = framed(Rect::new(10, 10, 10, 10));
        assert!(!should_apply(&s, false, &[monitor()]));
    }

    #[test]
    fn a_maximized_framed_window_still_qualifies() {
        let s = WindowState {
            show_state: ShowState::Maximized,
            ..framed(Rect::new(0, 0, 2560, 1400))
        };
        assert!(should_apply(&s, false, &[monitor()]));
    }

    #[test]
    fn unchanged_when_bounds_and_chrome_match() {
        let applied = test_state(Rect::new(0, 0, 2560, 1440));
        let current = applied.clone();
        assert_eq!(observe(&current, &applied, &[monitor()], true), BorderlessObservation::Unchanged);
    }

    #[test]
    fn minimized_is_dormant() {
        let applied = test_state(Rect::new(0, 0, 2560, 1440));
        let current = WindowState { show_state: ShowState::Minimized, ..applied.clone() };
        assert_eq!(observe(&current, &applied, &[monitor()], true), BorderlessObservation::Dormant);
    }

    #[test]
    fn the_app_restoring_its_frame_is_reasserted() {
        let applied = test_state(Rect::new(0, 0, 2560, 1440));
        let current = framed(Rect::new(100, 100, 1380, 820));
        assert_eq!(observe(&current, &applied, &[monitor()], true), BorderlessObservation::Reassert);
    }

    #[test]
    fn going_fullscreen_itself_stops_management_for_a_zone_target() {
        let applied = test_state(Rect::new(0, 0, 1920, 1080)); // a zone
        let current = test_state(Rect::new(0, 0, 2560, 1440)); // the app's own fullscreen
        assert_eq!(observe(&current, &applied, &[monitor()], false), BorderlessObservation::AppFullscreen);
    }

    #[test]
    fn for_a_monitor_target_a_changed_fullscreen_rect_is_just_reasserted() {
        let applied = test_state(Rect::new(0, 0, 2560, 1440));
        let moved = test_state(Rect::new(-5, 0, 2555, 1440));
        assert_eq!(observe(&moved, &applied, &[monitor()], true), BorderlessObservation::Reassert);
    }
}
