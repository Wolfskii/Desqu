//! The Vials data model: vials, the desktop items they hold (by reference), and sort rules.
//! Items never move out of the Desktop folders; a vial only *refers* to them.

use std::collections::HashSet;
use std::path::Path;

use desqu_desktop::ItemKind;
use serde::{Deserialize, Serialize};

pub const CONFIG_VERSION: u32 = 1;

/// Id of the built-in "Desktop" vial that shows every loose item while Vials draws the desktop.
pub const INBOX_ID: &str = "desktop";

/// A rectangle as fractions (0..1) of a monitor, so a vial keeps its place across resolutions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NormalizedRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl NormalizedRect {
    /// Clamps into the unit square and keeps a minimum size so a vial can never collapse.
    pub fn sanitized(self) -> Self {
        const MIN: f64 = 0.05;
        let finite = |v: f64, fallback: f64| if v.is_finite() { v } else { fallback };
        let width = finite(self.width, 0.2).clamp(MIN, 1.0);
        let height = finite(self.height, 0.3).clamp(MIN, 1.0);
        Self {
            x: finite(self.x, 0.0).clamp(0.0, 1.0 - width),
            y: finite(self.y, 0.0).clamp(0.0, 1.0 - height),
            width,
            height,
        }
    }
}

/// A rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PixelRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl PixelRect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Vial {
    pub id: String,
    pub name: String,
    /// Monitor name (as the OS reports it). `None` = the primary monitor.
    #[serde(default)]
    pub monitor: Option<String>,
    pub rect: NormalizedRect,
    /// Desktop item ids (absolute parsing names), in display order.
    #[serde(default)]
    pub items: Vec<String>,
    #[serde(default)]
    pub collapsed: bool,
    /// The built-in "Desktop" vial: it lists whatever sits in no other vial, so `items` stays empty.
    /// It exists while Vials draws the desktop (`draw_desktop`) and cannot be deleted.
    #[serde(default)]
    pub inbox: bool,
}

/// A predicate over a desktop item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum ItemMatcher {
    /// File extension without the dot, case-insensitive (`pdf`, `lnk`).
    Extension(String),
    Kind(ItemKind),
    /// `*` and `?` wildcards against the label shown on the desktop, case-insensitive.
    NameGlob(String),
    Any(Vec<ItemMatcher>),
    All(Vec<ItemMatcher>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SortRule {
    pub id: String,
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub matcher: ItemMatcher,
    /// The vial a matching item goes into.
    pub target_vial: String,
}

fn yes() -> bool {
    true
}

/// How vial windows look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VialTheme {
    /// Frosted glass: blur behind the window, rounded corners, a shadow and light edges.
    #[default]
    Glass,
    /// The plain dark panel: a flat, mostly opaque box with no blur.
    Classic,
}

/// What the Glass look blurs the desktop with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GlassBlur {
    /// Windows acrylic: strong blur with a fine grain and a slightly grey wash.
    #[default]
    Frosted,
    /// Plain blur behind the window: clearer, no grain.
    Clear,
}

fn default_glass_tint() -> u8 {
    20
}

fn default_version() -> u32 {
    CONFIG_VERSION
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VialsConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub vials: Vec<Vial>,
    #[serde(default)]
    pub rules: Vec<SortRule>,
    /// New desktop items are put into a vial by the first matching rule.
    #[serde(default = "yes")]
    pub auto_sort: bool,
    /// Items in a vial are hidden on the desktop (marked hidden, always restored). Only used when
    /// `draw_desktop` is off: with it on, a vial's items show in the vial only.
    #[serde(default = "yes")]
    pub hide_on_desktop: bool,
    /// Vials draws the whole desktop instead of sharing it with Explorer: Windows' own desktop icons
    /// are switched off (and back on when Vials stops) and every item that is in no vial shows in
    /// the built-in "Desktop" vial. Off by default: the desktop stays Explorer's, with all of its
    /// own behaviour, and only the items in vials are hidden from it.
    #[serde(default)]
    pub draw_desktop: bool,
    /// Dragging a box on the empty desktop offers to create a vial there.
    #[serde(default = "yes")]
    pub create_on_drag: bool,
    /// "Create vial here" in the desktop's right-click menu.
    #[serde(default = "yes")]
    pub context_menu: bool,
    #[serde(default)]
    pub theme: VialTheme,
    #[serde(default)]
    pub glass_blur: GlassBlur,
    /// How strongly the glass is tinted, 0 (clearest) to 100 (nearly solid).
    #[serde(default = "default_glass_tint")]
    pub glass_tint: u8,
    /// Items the user took out of a vial on purpose: rules never put them back.
    #[serde(default)]
    pub declined: Vec<String>,
}

impl Default for VialsConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            vials: Vec::new(),
            rules: Vec::new(),
            auto_sort: true,
            hide_on_desktop: true,
            draw_desktop: false,
            create_on_drag: true,
            context_menu: true,
            theme: VialTheme::default(),
            glass_blur: GlassBlur::default(),
            glass_tint: default_glass_tint(),
            declined: Vec::new(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("config is not valid JSON: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid config: {0}")]
    Invalid(String),
}

impl VialsConfig {
    pub fn vial(&self, id: &str) -> Option<&Vial> {
        self.vials.iter().find(|v| v.id == id)
    }

    /// The vial that holds `item_id`, if any.
    pub fn vial_of(&self, item_id: &str) -> Option<&Vial> {
        self.vials.iter().find(|v| v.items.iter().any(|i| i == item_id))
    }

    /// The built-in "Desktop" vial, if there is one.
    pub fn inbox(&self) -> Option<&Vial> {
        self.vials.iter().find(|v| v.inbox)
    }

    /// Makes sure the "Desktop" vial exists while Vials draws the desktop. Returns whether it was
    /// added.
    pub fn ensure_inbox(&mut self) -> bool {
        if !self.draw_desktop || self.inbox().is_some() {
            return false;
        }
        let id = if self.vial(INBOX_ID).is_none() { INBOX_ID.to_string() } else { format!("{INBOX_ID}-1") };
        self.vials.insert(
            0,
            Vial {
                id,
                name: "Desktop".into(),
                monitor: None,
                rect: NormalizedRect { x: 0.01, y: 0.03, width: 0.38, height: 0.8 },
                items: Vec::new(),
                collapsed: false,
                inbox: true,
            },
        );
        true
    }

    /// Every item id held by any vial.
    pub fn assigned_ids(&self) -> Vec<String> {
        self.vials.iter().flat_map(|v| v.items.iter().cloned()).collect()
    }

    /// Moves `item_id` into `vial_id` (`None` takes it out of every vial). Returns whether anything
    /// changed. Taking an item out records it as declined so rules do not re-add it; putting it
    /// into a vial clears that.
    pub fn assign(&mut self, item_id: &str, vial_id: Option<&str>) -> bool {
        // The "Desktop" vial holds whatever is in no other vial: putting an item there takes it out.
        let vial_id = vial_id.filter(|target| !self.vial(target).is_some_and(|v| v.inbox));
        if let Some(target) = vial_id {
            if self.vial(target).is_none() {
                return false;
            }
        }
        let already = self.vial_of(item_id).map(|v| v.id.clone());
        if already.as_deref() == vial_id {
            return false;
        }
        for vial in &mut self.vials {
            vial.items.retain(|i| i != item_id);
        }
        match vial_id {
            Some(target) => {
                self.declined.retain(|i| i != item_id);
                if let Some(v) = self.vials.iter_mut().find(|v| v.id == target) {
                    v.items.push(item_id.to_string());
                }
            }
            None => {
                if !self.declined.iter().any(|i| i == item_id) {
                    self.declined.push(item_id.to_string());
                }
            }
        }
        true
    }

    /// Normalizes what can be fixed (names, rects, an item listed twice) and rejects what cannot
    /// (empty or duplicate ids, a rule pointing at a vial that does not exist).
    pub fn validated(mut self) -> Result<Self, ConfigError> {
        let mut vial_ids = HashSet::new();
        let mut seen_items = HashSet::new();
        for vial in &mut self.vials {
            if vial.id.trim().is_empty() {
                return Err(ConfigError::Invalid("vial with empty id".into()));
            }
            // Ids become window labels and URL segments.
            if !vial.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                return Err(ConfigError::Invalid(format!(
                    "vial id '{}' may only contain letters, digits, '-' and '_'",
                    vial.id
                )));
            }
            if !vial_ids.insert(vial.id.clone()) {
                return Err(ConfigError::Invalid(format!("duplicate vial id '{}'", vial.id)));
            }
            // It lists loose items by itself: nothing may be assigned to it.
            if vial.inbox {
                vial.items.clear();
            }
            vial.name = vial.name.trim().to_string();
            if vial.name.is_empty() {
                vial.name = "Vial".into();
            }
            vial.rect = vial.rect.sanitized();
            // An item lives in one vial: the first one that lists it wins.
            vial.items.retain(|item| seen_items.insert(item.clone()));
        }
        let mut rule_ids = HashSet::new();
        for rule in &self.rules {
            if rule.id.trim().is_empty() || !rule_ids.insert(rule.id.clone()) {
                return Err(ConfigError::Invalid(format!("rule id '{}' is empty or duplicated", rule.id)));
            }
            if !vial_ids.contains(&rule.target_vial) {
                return Err(ConfigError::Invalid(format!(
                    "rule '{}' targets unknown vial '{}'",
                    rule.name, rule.target_vial
                )));
            }
        }
        // One "Desktop" vial at most, and only while Vials draws the desktop.
        let mut seen_inbox = false;
        let draw = self.draw_desktop;
        self.vials.retain(|v| !v.inbox || (draw && !std::mem::replace(&mut seen_inbox, true)));
        self.glass_tint = self.glass_tint.min(100);
        self.version = CONFIG_VERSION;
        Ok(self)
    }

    /// Early builds created an empty "Vial 1" on first start. Vials are made on the desktop now, so
    /// that untouched starter (empty, default name and id, no rules) is dropped. Anything the user
    /// changed or filled is kept.
    fn without_untouched_starter(mut self) -> Self {
        let untouched = self.vials.len() == 1
            && self.rules.is_empty()
            && self.vials[0].id == "vial-1"
            && self.vials[0].name == "Vial 1"
            && self.vials[0].items.is_empty()
            && self.vials[0].rect == (NormalizedRect { x: 0.02, y: 0.05, width: 0.2, height: 0.35 });
        if untouched {
            self.vials.clear();
        }
        self
    }

    /// A missing file yields defaults; a corrupt one is an error so the caller can back it up
    /// instead of silently overwriting the user's vials.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(serde_json::from_str::<VialsConfig>(&text)?.validated()?.without_untouched_starter()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Temp file + rename, so a crash mid-write cannot leave a truncated file.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vial(id: &str, items: &[&str]) -> Vial {
        Vial {
            id: id.into(),
            name: id.into(),
            monitor: None,
            rect: NormalizedRect { x: 0.0, y: 0.0, width: 0.2, height: 0.3 },
            items: items.iter().map(|s| s.to_string()).collect(),
            collapsed: false,
            inbox: false,
        }
    }

    fn config(vials: Vec<Vial>) -> VialsConfig {
        VialsConfig { vials, ..VialsConfig::default() }
    }

    #[test]
    fn default_config_is_valid_and_round_trips() {
        let cfg = VialsConfig::default().validated().unwrap();
        let json = serde_json::to_string(&cfg).unwrap();
        assert_eq!(serde_json::from_str::<VialsConfig>(&json).unwrap(), cfg);
        assert!(cfg.auto_sort && cfg.hide_on_desktop);
    }

    #[test]
    fn rects_are_clamped_and_cannot_collapse() {
        let r = NormalizedRect { x: 0.9, y: -3.0, width: 0.5, height: 0.0 }.sanitized();
        assert!(r.x + r.width <= 1.0 + 1e-9 && r.x >= 0.0);
        assert!(r.y >= 0.0 && r.height >= 0.05);
        let nan = NormalizedRect { x: f64::NAN, y: f64::INFINITY, width: f64::NAN, height: f64::NAN }.sanitized();
        assert!(nan.width > 0.0 && nan.height > 0.0);
    }

    #[test]
    fn duplicate_or_empty_vial_ids_are_rejected() {
        assert!(config(vec![vial("a", &[]), vial("a", &[])]).validated().is_err());
        assert!(config(vec![vial(" ", &[])]).validated().is_err());
    }

    #[test]
    fn an_item_listed_in_two_vials_stays_in_the_first() {
        let cfg = config(vec![vial("a", &["x", "y"]), vial("b", &["y", "z"])]).validated().unwrap();
        assert_eq!(cfg.vial("a").unwrap().items, ["x", "y"]);
        assert_eq!(cfg.vial("b").unwrap().items, ["z"]);
        assert_eq!(cfg.vial_of("y").unwrap().id, "a");
        assert_eq!(cfg.assigned_ids(), ["x", "y", "z"]);
    }

    #[test]
    fn blank_names_get_a_default() {
        let mut v = vial("a", &[]);
        v.name = "   ".into();
        assert_eq!(config(vec![v]).validated().unwrap().vials[0].name, "Vial");
    }

    #[test]
    fn a_rule_must_target_an_existing_vial() {
        let mut cfg = config(vec![vial("a", &[])]);
        cfg.rules.push(SortRule {
            id: "r".into(),
            name: "PDFs".into(),
            enabled: true,
            matcher: ItemMatcher::Extension("pdf".into()),
            target_vial: "nope".into(),
        });
        assert!(matches!(cfg.validated(), Err(ConfigError::Invalid(m)) if m.contains("unknown vial")));
    }

    #[test]
    fn matcher_serializes_with_a_type_tag() {
        let m = ItemMatcher::Any(vec![ItemMatcher::Extension("pdf".into()), ItemMatcher::Kind(ItemKind::Folder)]);
        let json = serde_json::to_value(&m).unwrap();
        assert_eq!(json["type"], "any");
        assert_eq!(json["value"][0]["type"], "extension");
        assert_eq!(json["value"][1]["value"], "folder");
        assert_eq!(serde_json::from_value::<ItemMatcher>(json).unwrap(), m);
    }

    fn starter_json(extra: &str) -> String {
        format!(
            r#"{{"version":1,"vials":[{{"id":"vial-1","name":"Vial 1","rect":{{"x":0.02,"y":0.05,"width":0.2,"height":0.35}},"items":[{extra}]}}],"rules":[]}}"#
        )
    }

    #[test]
    fn the_theme_defaults_to_glass_and_round_trips_by_name() {
        assert_eq!(VialsConfig::default().theme, VialTheme::Glass);
        let cfg: VialsConfig = serde_json::from_str(r#"{"theme":"classic"}"#).unwrap();
        assert_eq!(cfg.theme, VialTheme::Classic);
        assert!(serde_json::to_string(&cfg).unwrap().contains("\"theme\":\"classic\""));
        let old: VialsConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(old.theme, VialTheme::Glass);
    }

    #[test]
    fn glass_settings_default_round_trip_and_are_clamped() {
        let cfg = VialsConfig::default();
        assert_eq!((cfg.glass_blur, cfg.glass_tint), (GlassBlur::Frosted, 20));
        let cfg: VialsConfig = serde_json::from_str(r#"{"glassBlur":"clear","glassTint":250}"#).unwrap();
        assert_eq!(cfg.glass_blur, GlassBlur::Clear);
        assert_eq!(cfg.validated().unwrap().glass_tint, 100);
    }

    #[test]
    fn the_desktop_vial_is_added_once_while_drawing_the_desktop_and_never_holds_items() {
        let mut cfg = VialsConfig::default();
        assert!(!cfg.draw_desktop, "the desktop stays Explorer's by default");
        assert!(!cfg.ensure_inbox(), "no Desktop vial then");
        cfg.draw_desktop = true;
        assert!(cfg.ensure_inbox());
        assert!(!cfg.ensure_inbox(), "only one");
        let inbox = cfg.inbox().unwrap().clone();
        assert_eq!((inbox.name.as_str(), inbox.id.as_str()), ("Desktop", INBOX_ID));

        // Putting an item "into" the Desktop vial takes it out of the real vials.
        let mut cfg = config(vec![vial("a", &["x"])]);
        cfg.draw_desktop = true;
        cfg.ensure_inbox();
        let inbox_id = cfg.inbox().unwrap().id.clone();
        assert!(cfg.assign("x", Some(&inbox_id)));
        assert!(cfg.vial_of("x").is_none());
        assert!(cfg.inbox().unwrap().items.is_empty());
        assert_eq!(cfg.declined, ["x"]);
    }

    #[test]
    fn a_config_with_two_desktop_vials_keeps_the_first_and_empties_it() {
        let mut a = vial("desktop", &["x"]);
        a.inbox = true;
        let mut b = vial("desktop-2", &[]);
        b.inbox = true;
        let mut cfg = config(vec![a, b]);
        cfg.draw_desktop = true;
        let cfg = cfg.validated().unwrap();
        assert_eq!(cfg.vials.len(), 1);
        assert!(cfg.vials[0].items.is_empty());
    }

    #[test]
    fn the_desktop_vial_of_an_earlier_build_goes_away_when_vials_leaves_the_desktop_alone() {
        let mut a = vial("desktop", &[]);
        a.inbox = true;
        let cfg = config(vec![a, vial("keep", &["y"])]).validated().unwrap();
        assert_eq!(cfg.vials.len(), 1);
        assert_eq!(cfg.vials[0].id, "keep");
    }

    #[test]
    fn rules_never_target_the_desktop_vial() {
        let mut cfg = config(vec![vial("a", &[])]);
        cfg.draw_desktop = true;
        cfg.ensure_inbox();
        let inbox_id = cfg.inbox().unwrap().id.clone();
        let item = desqu_desktop::DesktopItem {
            id: r"C:\d.pdf".into(),
            name: "a".into(),
            kind: ItemKind::File,
            position: desqu_desktop::Point { x: 0, y: 0 },
            parked: false,
        };
        cfg.rules.push(SortRule {
            id: "r".into(),
            name: "r".into(),
            enabled: true,
            matcher: ItemMatcher::Extension("pdf".into()),
            target_vial: inbox_id,
        });
        assert!(super::super::rules::first_match(&cfg, &item).is_none());
    }

    #[test]
    fn a_fresh_config_has_no_vials() {
        assert!(VialsConfig::default().vials.is_empty());
    }

    #[test]
    fn the_untouched_starter_vial_of_early_builds_is_dropped_but_a_used_one_is_kept() {
        let dir = std::env::temp_dir().join(format!("desqu-vials-starter-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, starter_json("")).unwrap();
        assert!(VialsConfig::load(&path).unwrap().vials.is_empty());
        std::fs::write(&path, starter_json(r#""C:\\x\\a.txt""#)).unwrap();
        assert_eq!(VialsConfig::load(&path).unwrap().vials.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_is_default_corrupt_is_an_error_and_save_round_trips() {
        let dir = std::env::temp_dir().join(format!("vials-config-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("config.json");
        assert_eq!(VialsConfig::load(&path).unwrap(), VialsConfig::default());

        let cfg = config(vec![vial("a", &["x"])]);
        cfg.save(&path).unwrap();
        assert_eq!(VialsConfig::load(&path).unwrap().vials[0].items, ["x"]);

        std::fs::write(&path, "{ nope").unwrap();
        assert!(VialsConfig::load(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn assigning_moves_an_item_and_taking_it_out_records_it_as_declined() {
        let mut cfg = config(vec![vial("a", &["x"]), vial("b", &[])]);
        assert!(cfg.assign("x", Some("b")));
        assert_eq!(cfg.vial_of("x").unwrap().id, "b");
        assert!(cfg.vial("a").unwrap().items.is_empty());

        assert!(!cfg.assign("x", Some("b")), "already there: nothing changes");
        assert!(!cfg.assign("y", Some("nope")), "unknown vial is ignored");

        assert!(cfg.assign("x", None));
        assert!(cfg.vial_of("x").is_none());
        assert_eq!(cfg.declined, ["x"]);
        assert!(!cfg.assign("x", None), "already out");

        assert!(cfg.assign("x", Some("a")));
        assert!(cfg.declined.is_empty(), "putting it back clears the decline");
    }

    #[test]
    fn vial_ids_must_be_safe_for_window_labels_and_urls() {
        assert!(config(vec![vial("a/b", &[])]).validated().is_err());
        assert!(config(vec![vial("a b", &[])]).validated().is_err());
        assert!(config(vec![vial("vial-1_x", &[])]).validated().is_ok());
    }
}
