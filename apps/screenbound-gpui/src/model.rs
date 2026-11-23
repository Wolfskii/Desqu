//! What the Screenbound window decides, as pure functions: the zone presets, drawing and nudging a
//! zone, and the list of apps. (The same rules the Tauri window has in `zones.ts` and `apps.ts`.)

use screenbound_module::core::config::{AppConfig, KnownApp};
use screenbound_module::core::monitor::MonitorInfo;
use screenbound_module::core::window::WindowInfo;
use screenbound_module::core::zone::{NormalizedRect, ZoneUnit};

const MIN: f64 = 0.02;

pub fn sanitize(r: NormalizedRect) -> NormalizedRect {
    let width = r.width.clamp(MIN, 1.0);
    let height = r.height.clamp(MIN, 1.0);
    NormalizedRect { x: r.x.clamp(0.0, 1.0 - width), y: r.y.clamp(0.0, 1.0 - height), width, height }
}

const MAGNETS: [f64; 7] = [0.0, 0.25, 1.0 / 3.0, 0.5, 2.0 / 3.0, 0.75, 1.0];

/// Snaps to a 0.5 % grid, pulling toward common fractions (halves, thirds, quarters) when close.
pub fn snap(v: f64) -> f64 {
    MAGNETS.iter().copied().find(|m| (m - v).abs() < 0.012).unwrap_or_else(|| (v * 200.0).round() / 200.0)
}

/// The zone drawn by dragging from `start` to `end` (both as fractions of the monitor, 0..1).
pub fn drag_rect(start: (f64, f64), end: (f64, f64)) -> NormalizedRect {
    let (x0, x1) = (snap(start.0.clamp(0.0, 1.0).min(end.0.clamp(0.0, 1.0))), snap(start.0.clamp(0.0, 1.0).max(end.0.clamp(0.0, 1.0))));
    let (y0, y1) = (snap(start.1.clamp(0.0, 1.0).min(end.1.clamp(0.0, 1.0))), snap(start.1.clamp(0.0, 1.0).max(end.1.clamp(0.0, 1.0))));
    sanitize(NormalizedRect { x: x0, y: y0, width: x1 - x0, height: y1 - y0 })
}

pub struct Preset {
    pub label: &'static str,
    pub rect: fn(Option<&MonitorInfo>) -> NormalizedRect,
}

fn r(x: f64, y: f64, width: f64, height: f64) -> NormalizedRect {
    NormalizedRect { x, y, width, height }
}

/// The largest 16:9 area, centred, on a monitor.
fn centered_16_9(monitor: Option<&MonitorInfo>) -> NormalizedRect {
    let Some(m) = monitor else { return r(0.125, 0.0, 0.75, 1.0) };
    let (w, h) = (f64::from(m.bounds.right - m.bounds.left), f64::from(m.bounds.bottom - m.bounds.top));
    let width = (h * 16.0 / 9.0 / w).min(1.0);
    let height = (w * width * 9.0 / 16.0 / h).min(1.0);
    r((1.0 - width) / 2.0, (1.0 - height) / 2.0, width, height)
}

pub const PRESETS: [Preset; 10] = [
    Preset { label: "Left 75%", rect: |_| r(0.0, 0.0, 0.75, 1.0) },
    Preset { label: "Right 25%", rect: |_| r(0.75, 0.0, 0.25, 1.0) },
    Preset { label: "Right 75%", rect: |_| r(0.25, 0.0, 0.75, 1.0) },
    Preset { label: "Left 25%", rect: |_| r(0.0, 0.0, 0.25, 1.0) },
    Preset { label: "Left 50%", rect: |_| r(0.0, 0.0, 0.5, 1.0) },
    Preset { label: "Right 50%", rect: |_| r(0.5, 0.0, 0.5, 1.0) },
    Preset { label: "Top half", rect: |_| r(0.0, 0.0, 1.0, 0.5) },
    Preset { label: "Bottom half", rect: |_| r(0.0, 0.5, 1.0, 0.5) },
    Preset { label: "Centered 16:9", rect: centered_16_9 },
    Preset { label: "Full", rect: |_| r(0.0, 0.0, 1.0, 1.0) },
];

/// Sets the first zone to `rect` (as percentages of the monitor). Returns whether the config changed.
pub fn set_zone(cfg: &mut AppConfig, rect: NormalizedRect) -> bool {
    let rect = sanitize(rect);
    let Some(zone) = cfg.zones.first_mut() else { return false };
    if zone.rect == rect && zone.unit == ZoneUnit::Percent && zone.pixels.is_none() {
        return false;
    }
    zone.rect = rect;
    zone.unit = ZoneUnit::Percent;
    zone.pixels = None;
    true
}

/// One line of the Apps list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppRow {
    pub process_name: String,
    pub title: String,
    pub running: bool,
    pub applied: bool,
}

/// Merges the open windows into the remembered apps. Keeps a useful title when one was seen.
pub fn remember_apps(seen: &[KnownApp], windows: &[WindowInfo]) -> Vec<KnownApp> {
    let mut out: Vec<KnownApp> = seen.to_vec();
    for window in windows {
        let name = window.identity.process_name.trim();
        if name.is_empty() {
            continue;
        }
        let title = window.identity.title.trim();
        match out.iter_mut().find(|a| a.process_name.eq_ignore_ascii_case(name)) {
            Some(app) => {
                if !title.is_empty() {
                    app.title = title.to_string();
                }
            }
            None => out.push(KnownApp { process_name: name.to_string(), title: title.to_string(), enabled: false, group_id: None }),
        }
    }
    out
}

/// Apps that are open now first, then by name.
pub fn app_rows(seen: &[KnownApp], running: &[String]) -> Vec<AppRow> {
    let mut rows: Vec<AppRow> = seen
        .iter()
        .map(|a| AppRow {
            process_name: a.process_name.clone(),
            title: a.title.clone(),
            running: running.iter().any(|n| n.eq_ignore_ascii_case(&a.process_name)),
            applied: a.enabled,
        })
        .collect();
    rows.sort_by(|a, b| {
        b.running.cmp(&a.running).then_with(|| {
            let label = |r: &AppRow| if r.title.is_empty() { r.process_name.to_lowercase() } else { r.title.to_lowercase() };
            label(a).cmp(&label(b))
        })
    });
    rows
}

/// Turns an app on or off (and remembers it when it was not in the list yet). Returns whether anything changed.
pub fn set_app_enabled(cfg: &mut AppConfig, process_name: &str, enabled: bool) -> bool {
    match cfg.known_apps.iter_mut().find(|a| a.process_name.eq_ignore_ascii_case(process_name)) {
        Some(app) if app.enabled == enabled => false,
        Some(app) => {
            app.enabled = enabled;
            true
        }
        None => {
            cfg.known_apps.push(KnownApp { process_name: process_name.to_string(), title: String::new(), enabled, group_id: None });
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenbound_module::core::geometry::Rect;

    fn monitor(w: i32, h: i32) -> MonitorInfo {
        MonitorInfo {
            id: screenbound_module::core::monitor::MonitorId("m1".into()),
            device_name: "D".into(),
            friendly_name: "D".into(),
            number: 1,
            is_primary: true,
            bounds: Rect::new(0, 0, w, h),
            work_area: Rect::new(0, 0, w, h),
            dpi: 96,
        }
    }

    #[test]
    fn dragging_snaps_to_halves_and_stays_inside_the_monitor() {
        let z = drag_rect((0.01, 0.0), (0.49, 1.0));
        assert_eq!((z.x, z.width, z.height), (0.0, 0.5, 1.0), "close to 0 and 0.5: pulled onto them");
        let back = drag_rect((0.9, 0.9), (0.1, 0.1));
        assert!(back.x < 0.15 && back.width > 0.7, "dragging backwards works the same");
        let outside = drag_rect((-1.0, -1.0), (2.0, 2.0));
        assert_eq!((outside.x, outside.y, outside.width, outside.height), (0.0, 0.0, 1.0, 1.0));
        assert!(drag_rect((0.5, 0.5), (0.5, 0.5)).width >= 0.02, "never collapses to nothing");
    }

    #[test]
    fn a_16_9_zone_on_an_ultrawide_is_a_centred_strip() {
        let z = (PRESETS[8].rect)(Some(&monitor(5120, 1440)));
        assert!((z.width - 0.5).abs() < 1e-9 && (z.height - 1.0).abs() < 1e-9, "1440 px high needs 2560 px of 5120 for 16:9: {z:?}");
        assert!((z.x - (1.0 - z.width) / 2.0).abs() < 1e-9);
        let on_16_9 = (PRESETS[8].rect)(Some(&monitor(1920, 1080)));
        assert!((on_16_9.width - 1.0).abs() < 1e-9, "a 16:9 monitor is filled");
    }

    #[test]
    fn setting_the_zone_changes_the_first_zone_once() {
        let mut cfg = AppConfig::default();
        assert!(set_zone(&mut cfg, NormalizedRect { x: 0.25, y: 0.0, width: 0.5, height: 1.0 }));
        assert_eq!(cfg.zones[0].rect.x, 0.25);
        assert!(!set_zone(&mut cfg, NormalizedRect { x: 0.25, y: 0.0, width: 0.5, height: 1.0 }), "same zone: no change");
        let mut none = AppConfig { zones: vec![], ..AppConfig::default() };
        assert!(!set_zone(&mut none, NormalizedRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 }));
    }

    #[test]
    fn apps_are_remembered_toggled_and_listed_running_first() {
        let mut cfg = AppConfig::default();
        assert!(set_app_enabled(&mut cfg, "vlc.exe", true));
        assert!(!set_app_enabled(&mut cfg, "VLC.EXE", true), "case does not matter, and nothing changed");
        cfg.known_apps.push(KnownApp { process_name: "a.exe".into(), title: "Alpha".into(), enabled: false, group_id: None });
        cfg.known_apps.push(KnownApp { process_name: "z.exe".into(), title: "Zulu".into(), enabled: true, group_id: None });
        let rows = app_rows(&cfg.known_apps, &["Z.exe".to_string()]);
        assert_eq!(rows[0].process_name, "z.exe", "running first");
        assert!(rows[0].running && rows[0].applied);
        assert_eq!(rows[1].process_name, "a.exe", "then by label: Alpha before vlc.exe");
        assert!(!rows[1].running);
        assert_eq!(rows.len(), 3);
    }
}
