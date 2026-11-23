//! Where a vial's window goes: pure geometry on monitor rectangles.

use super::model::{NormalizedRect, PixelRect};

/// A monitor as the placement code sees it (physical pixels).
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorRef {
    pub name: String,
    pub bounds: PixelRect,
    pub primary: bool,
    /// DPI scale (1.0 = 100 %), for sizes defined in device-independent pixels.
    pub scale: f64,
}

/// Height of a collapsed vial: just its header, in device-independent pixels.
pub const COLLAPSED_HEIGHT_DIP: f64 = 34.0;

/// The window height for a vial: its rect's height, or only the header when collapsed.
pub fn window_height(rect_height: i32, collapsed: bool, scale: f64) -> i32 {
    if collapsed {
        (COLLAPSED_HEIGHT_DIP * scale).round().max(1.0) as i32
    } else {
        rect_height
    }
}

/// The monitor a vial lives on: the one it names, else the primary, else the first.
pub fn pick_monitor<'a>(wanted: Option<&str>, monitors: &'a [MonitorRef]) -> Option<&'a MonitorRef> {
    wanted
        .and_then(|name| monitors.iter().find(|m| m.name == name))
        .or_else(|| monitors.iter().find(|m| m.primary))
        .or_else(|| monitors.first())
}

/// Maps a normalized rect onto a monitor. Edges are rounded independently so neighbouring vials
/// share an edge without a gap or overlap.
pub fn resolve(rect: &NormalizedRect, monitor: &PixelRect) -> PixelRect {
    let r = rect.sanitized();
    let (w, h) = (f64::from(monitor.width()), f64::from(monitor.height()));
    let edge = |origin: i32, span: f64, frac: f64| origin + (span * frac).round() as i32;
    PixelRect {
        left: edge(monitor.left, w, r.x),
        top: edge(monitor.top, h, r.y),
        right: edge(monitor.left, w, r.x + r.width),
        bottom: edge(monitor.top, h, r.y + r.height),
    }
}

/// The inverse of [`resolve`]: where a window the user dragged or resized sits on `monitor`.
pub fn to_normalized(window: &PixelRect, monitor: &PixelRect) -> NormalizedRect {
    let (w, h) = (f64::from(monitor.width().max(1)), f64::from(monitor.height().max(1)));
    NormalizedRect {
        x: f64::from(window.left - monitor.left) / w,
        y: f64::from(window.top - monitor.top) / h,
        width: f64::from(window.width()) / w,
        height: f64::from(window.height()) / h,
    }
    .sanitized()
}

/// The monitor that contains the window's centre (the one it is "on").
pub fn monitor_containing<'a>(window: &PixelRect, monitors: &'a [MonitorRef]) -> Option<&'a MonitorRef> {
    let cx = window.left + window.width() / 2;
    let cy = window.top + window.height() / 2;
    monitors
        .iter()
        .find(|m| cx >= m.bounds.left && cx < m.bounds.right && cy >= m.bounds.top && cy < m.bounds.bottom)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(l: i32, t: i32, r: i32, b: i32) -> PixelRect {
        PixelRect { left: l, top: t, right: r, bottom: b }
    }

    fn monitors() -> Vec<MonitorRef> {
        vec![
            MonitorRef { name: "left".into(), bounds: rect(-1920, 0, 0, 1080), primary: false, scale: 1.0 },
            MonitorRef { name: "main".into(), bounds: rect(0, 0, 2560, 1440), primary: true, scale: 1.5 },
        ]
    }

    #[test]
    fn picks_the_named_monitor_else_primary_else_first() {
        let m = monitors();
        assert_eq!(pick_monitor(Some("left"), &m).unwrap().name, "left");
        assert_eq!(pick_monitor(Some("unplugged"), &m).unwrap().name, "main");
        assert_eq!(pick_monitor(None, &m).unwrap().name, "main");
        let no_primary = vec![MonitorRef { primary: false, ..m[0].clone() }];
        assert_eq!(pick_monitor(None, &no_primary).unwrap().name, "left");
        assert!(pick_monitor(None, &[]).is_none());
    }

    #[test]
    fn resolves_on_a_monitor_with_a_negative_origin() {
        let left = rect(-1920, 0, 0, 1080);
        let r = resolve(&NormalizedRect { x: 0.5, y: 0.25, width: 0.25, height: 0.5 }, &left);
        assert_eq!(r, rect(-960, 270, -480, 810));
    }

    #[test]
    fn adjacent_rects_share_an_edge() {
        let m = rect(0, 0, 1001, 700);
        let a = resolve(&NormalizedRect { x: 0.0, y: 0.0, width: 0.333, height: 1.0 }, &m);
        let b = resolve(&NormalizedRect { x: 0.333, y: 0.0, width: 0.667, height: 1.0 }, &m);
        assert_eq!(a.right, b.left);
    }

    #[test]
    fn resolve_and_to_normalized_round_trip_within_a_pixel() {
        let m = rect(0, 0, 2560, 1440);
        let n = NormalizedRect { x: 0.1, y: 0.2, width: 0.3, height: 0.4 };
        let back = to_normalized(&resolve(&n, &m), &m);
        assert!((back.x - n.x).abs() < 0.001 && (back.width - n.width).abs() < 0.001);
        assert!((back.y - n.y).abs() < 0.001 && (back.height - n.height).abs() < 0.001);
    }

    #[test]
    fn a_dragged_window_is_attributed_to_the_monitor_of_its_centre() {
        let m = monitors();
        // Mostly on the left monitor.
        assert_eq!(monitor_containing(&rect(-1000, 100, 100, 400), &m).unwrap().name, "left");
        assert_eq!(monitor_containing(&rect(100, 100, 400, 400), &m).unwrap().name, "main");
        assert!(monitor_containing(&rect(9000, 9000, 9100, 9100), &m).is_none());
    }

    #[test]
    fn a_window_dragged_partly_off_screen_is_clamped_back_inside() {
        let m = rect(0, 0, 1000, 1000);
        let n = to_normalized(&rect(900, 900, 1300, 1300), &m);
        assert!(n.x + n.width <= 1.0 + 1e-9 && n.y + n.height <= 1.0 + 1e-9);
    }

    #[test]
    fn a_collapsed_vial_is_only_as_tall_as_its_header() {
        assert_eq!(window_height(500, false, 1.5), 500);
        assert_eq!(window_height(500, true, 1.0), 34);
        assert_eq!(window_height(500, true, 1.5), 51);
        assert!(window_height(500, true, 0.0) >= 1);
    }
}
