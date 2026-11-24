//! Saved layouts: where every vial sits (not what it holds). A layout can be named and restored by hand, and the
//! program also keeps one per screen setup (which monitors, how big, where): when you dock or undock, the
//! vials go to where they were the last time you had that setup. Pure.

use serde::{Deserialize, Serialize};

use super::layout::MonitorRef;
use super::model::{NormalizedRect, VialsConfig};

/// Where one vial sits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub vial: String,
    pub monitor: Option<String>,
    pub rect: NormalizedRect,
    #[serde(default)]
    pub collapsed: bool,
}

/// A saved layout. A named one has a `name`; the one kept for a screen setup has that setup's key in `setup`
/// (and no name).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub setup: String,
    pub places: Vec<Place>,
}

/// A text that says which screen setup this is: every monitor's name, size and place, in a fixed order. The
/// same monitors the same way give the same key; another resolution or arrangement gives another.
pub fn setup_key(monitors: &[MonitorRef]) -> String {
    let mut parts: Vec<String> = monitors
        .iter()
        .map(|m| format!("{}:{}x{}@{},{}", m.name, m.bounds.width(), m.bounds.height(), m.bounds.left, m.bounds.top))
        .collect();
    parts.sort();
    parts.join("|")
}

/// Where the vials are now.
pub fn capture(cfg: &VialsConfig) -> Vec<Place> {
    cfg.vials
        .iter()
        .filter(|v| !v.inbox)
        .map(|v| Place { vial: v.id.clone(), monitor: v.monitor.clone(), rect: v.rect, collapsed: v.collapsed })
        .collect()
}

/// Puts the vials of `places` where they say. A vial that is gone is skipped, one that is locked stays where it
/// is. Returns whether anything moved.
pub fn apply(cfg: &mut VialsConfig, places: &[Place]) -> bool {
    let mut changed = false;
    for place in places {
        let Some(vial) = cfg.vials.iter_mut().find(|v| v.id == place.vial && !v.locked) else { continue };
        if vial.monitor != place.monitor || vial.rect != place.rect || vial.collapsed != place.collapsed {
            vial.monitor = place.monitor.clone();
            vial.rect = place.rect;
            vial.collapsed = place.collapsed;
            changed = true;
        }
    }
    changed
}

/// Saves the current places under a name (a name that exists is replaced). Returns the name used.
pub fn save_named(cfg: &mut VialsConfig, name: &str) -> String {
    let name = name.trim();
    let name = if name.is_empty() { next_name(&cfg.layouts) } else { name.to_string() };
    let places = capture(cfg);
    match cfg.layouts.iter_mut().find(|l| l.setup.is_empty() && l.name.eq_ignore_ascii_case(&name)) {
        Some(existing) => existing.places = places,
        None => cfg.layouts.push(Layout { name: name.clone(), setup: String::new(), places }),
    }
    name
}

fn next_name(layouts: &[Layout]) -> String {
    (1..)
        .map(|n| format!("Layout {n}"))
        .find(|name| !layouts.iter().any(|l| l.setup.is_empty() && l.name.eq_ignore_ascii_case(name)))
        .expect("endless range")
}

/// The named layouts, in the order they were saved.
pub fn named(cfg: &VialsConfig) -> Vec<&Layout> {
    cfg.layouts.iter().filter(|l| l.setup.is_empty()).collect()
}

/// Brings back a named layout. Returns whether anything moved.
pub fn restore_named(cfg: &mut VialsConfig, name: &str) -> bool {
    let Some(places) = cfg.layouts.iter().find(|l| l.setup.is_empty() && l.name == name).map(|l| l.places.clone()) else { return false };
    apply(cfg, &places)
}

pub fn delete_named(cfg: &mut VialsConfig, name: &str) -> bool {
    let before = cfg.layouts.len();
    cfg.layouts.retain(|l| !(l.setup.is_empty() && l.name == name));
    cfg.layouts.len() != before
}

/// The screen setup changed from `old` to `new`: the places of the vials are kept for `old`, and the ones kept
/// for `new` (if there are any) are put back. Returns whether the vials moved.
pub fn switch_setup(cfg: &mut VialsConfig, old: &str, new: &str) -> bool {
    if old == new {
        return false;
    }
    let places = capture(cfg);
    remember(cfg, old, places);
    match cfg.layouts.iter().find(|l| l.setup == new).map(|l| l.places.clone()) {
        Some(places) => apply(cfg, &places),
        None => false,
    }
}

fn remember(cfg: &mut VialsConfig, setup: &str, places: Vec<Place>) {
    if setup.is_empty() {
        return;
    }
    match cfg.layouts.iter_mut().find(|l| l.setup == setup) {
        Some(existing) => existing.places = places,
        None => cfg.layouts.push(Layout { name: String::new(), setup: setup.to_string(), places }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PixelRect, Vial};

    fn rect(x: f64) -> NormalizedRect {
        NormalizedRect { x, y: 0.1, width: 0.2, height: 0.3 }
    }

    fn cfg() -> VialsConfig {
        let mut cfg = VialsConfig::default();
        for (id, x) in [("a", 0.1), ("b", 0.5)] {
            let mut v: Vial = serde_json::from_str(&format!(r#"{{"id":"{id}","name":"{id}","rect":{{"x":{x},"y":0.1,"width":0.2,"height":0.3}},"items":[]}}"#)).unwrap();
            v.monitor = Some("M1".into());
            cfg.vials.push(v);
        }
        cfg
    }

    fn monitor(name: &str, w: i32, left: i32) -> MonitorRef {
        MonitorRef { name: name.into(), bounds: PixelRect { left, top: 0, right: left + w, bottom: 1080 }, primary: left == 0, scale: 1.0 }
    }

    #[test]
    fn a_named_layout_remembers_the_places_and_brings_them_back() {
        let mut c = cfg();
        assert_eq!(save_named(&mut c, ""), "Layout 1");
        assert_eq!(save_named(&mut c, ""), "Layout 2");
        c.vials[0].rect = rect(0.7);
        c.vials[1].collapsed = true;
        assert!(restore_named(&mut c, "Layout 1"));
        assert_eq!(c.vials[0].rect, rect(0.1));
        assert!(!c.vials[1].collapsed);
        assert!(!restore_named(&mut c, "Layout 1"), "nothing left to move");
        assert!(!restore_named(&mut c, "nope"));
        assert_eq!(named(&c).len(), 2);
        assert_eq!(save_named(&mut c, "layout 1"), "layout 1", "the same name (any case) is replaced, not added");
        assert_eq!(named(&c).len(), 2);
        assert!(delete_named(&mut c, "Layout 2") && !delete_named(&mut c, "Layout 2"));
    }

    #[test]
    fn a_locked_vial_stays_and_a_deleted_one_is_skipped() {
        let mut c = cfg();
        save_named(&mut c, "x");
        c.vials[0].rect = rect(0.7);
        c.vials[0].locked = true;
        c.vials.remove(1);
        assert!(!restore_named(&mut c, "x"));
        assert_eq!(c.vials[0].rect, rect(0.7), "locked: not moved");
    }

    #[test]
    fn the_same_monitors_give_the_same_key_in_any_order_and_other_ones_another() {
        let docked = [monitor("A", 1920, 0), monitor("B", 2560, 1920)];
        let reversed = [monitor("B", 2560, 1920), monitor("A", 1920, 0)];
        assert_eq!(setup_key(&docked), setup_key(&reversed));
        assert_ne!(setup_key(&docked), setup_key(&[monitor("A", 1920, 0)]));
        assert_ne!(setup_key(&docked), setup_key(&[monitor("A", 1920, 0), monitor("B", 1920, 1920)]), "another resolution");
    }

    #[test]
    fn docking_and_undocking_bring_back_where_the_vials_were_in_each_setup() {
        let (laptop, desk) = ("laptop".to_string(), "desk".to_string());
        let mut c = cfg();
        // At the desk the vials are as configured; unplugging moves them for the laptop.
        assert!(!switch_setup(&mut c, &desk, &laptop), "nothing remembered for the laptop yet: they stay");
        c.vials[0].rect = rect(0.6); // the user arranges them on the laptop
        assert!(switch_setup(&mut c, &laptop, &desk), "back at the desk");
        assert_eq!(c.vials[0].rect, rect(0.1));
        assert!(switch_setup(&mut c, &desk, &laptop));
        assert_eq!(c.vials[0].rect, rect(0.6), "and the laptop arrangement returns");
        assert!(!switch_setup(&mut c, &laptop, &laptop));
        assert!(c.layouts.iter().filter(|l| l.setup == "desk").count() == 1, "one entry per setup");
    }
}
