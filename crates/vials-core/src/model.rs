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
    /// The sort last chosen for the vial's items ("name", "type", "modified", "size"), if any.
    #[serde(default)]
    pub sort_key: Option<String>,
    #[serde(default)]
    pub sort_desc: bool,
    /// Folders stay on top whatever the sort (also re-applied when items change).
    #[serde(default)]
    pub folders_first: bool,
    /// The vial stays where it is and keeps its items: no moving, resizing, renaming or dragging out.
    #[serde(default)]
    pub locked: bool,
    /// This vial's own glass tint (0 clear to 100 solid); `None` follows the setting for all vials.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<u8>,
    /// This vial's own panel colour as `#rrggbb`; `None` is the look's own colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Icon size in pixels (24, 32 or 48); `None` is 32.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_size: Option<u8>,
    /// The title sits in the middle of the title bar instead of at its left.
    #[serde(default)]
    pub title_center: bool,
}

/// The icon sizes a vial can use, in pixels.
pub const ICON_SIZES: [u8; 3] = [24, 32, 48];

/// The panel colours offered in the vial menu: (name, `#rrggbb`).
pub const PANEL_COLORS: [(&str, &str); 6] = [
    ("Graphite", "#2b2f36"),
    ("Blue", "#1f3b73"),
    ("Green", "#1f5a3d"),
    ("Orange", "#7a4a1a"),
    ("Pink", "#7a2a52"),
    ("Purple", "#4a2f7a"),
];

impl Vial {
    /// The icon size to draw: the chosen one, else 32.
    pub fn icon_px(&self) -> u8 {
        self.icon_size.filter(|s| ICON_SIZES.contains(s)).unwrap_or(32)
    }

    /// The panel colour as `0xRRGGBB`, if one is set and valid.
    pub fn color_rgb(&self) -> Option<u32> {
        let hex = self.color.as_deref()?.strip_prefix('#')?;
        (hex.len() == 6).then(|| u32::from_str_radix(hex, 16).ok()).flatten()
    }
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
    /// The label shown on the desktop contains / starts with / ends with this text, case-insensitive.
    NameContains(String),
    NameStartsWith(String),
    NameEndsWith(String),
    /// Last changed within this many days.
    ModifiedWithinDays(u32),
    /// Not changed for at least this many days.
    ModifiedOlderThanDays(u32),
    /// Created within this many days.
    CreatedWithinDays(u32),
    /// A file bigger than this many megabytes (folders have no size).
    SizeOverMb(u64),
    /// A file smaller than this many megabytes.
    SizeUnderMb(u64),
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
    /// The Fluent look (Windows 11 surfaces): acrylic tinted with the Fluent dark surface, 8 px corners,
    /// hairline edges and Fluent's text and selection colours.
    Fluent,
    /// Fluent with its light surface and colours.
    FluentLight,
    /// Fluent, dark or light by the Windows app setting (and changing with it).
    System,
    /// Liquid glass: the wallpaper is redrawn behind the vial with a refraction shader (bent edges,
    /// highlights), which Windows' own blur cannot do. Popups use the plain Glass look.
    Liquid,
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
    /// A collapsed vial opens while the pointer is over it and closes again when the pointer leaves.
    #[serde(default)]
    pub hover_open: bool,
    /// A double-click on the empty desktop hides every vial and the desktop icons; another shows them.
    #[serde(default = "yes")]
    pub quick_hide: bool,
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
            quick_hide: true,
            hover_open: false,
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
    /// Sets one option by the name the settings pages use (`theme`, `glassTint`, `autoSort`,
    /// `hideOnDesktop`, `createOnDrag`, `contextMenu`). Both Desqu and Vials go through this, so a
    /// change means the same thing wherever it is made.
    pub fn set_setting(&mut self, key: &str, value: &str) -> Result<(), String> {
        let flag = || match value {
            "true" => Ok(true),
            "false" => Ok(false),
            other => Err(format!("{key}: expected true or false, got {other}")),
        };
        match key {
            "theme" => {
                self.theme = serde_json::from_value(serde_json::Value::String(value.to_string())).map_err(|_| format!("theme: unknown look {value}"))?
            }
            "glassTint" => self.glass_tint = value.parse::<u8>().ok().filter(|v| *v <= 100).ok_or(format!("glassTint: expected 0 to 100, got {value}"))?,
            "autoSort" => self.auto_sort = flag()?,
            "hideOnDesktop" => self.hide_on_desktop = flag()?,
            "createOnDrag" => self.create_on_drag = flag()?,
            "contextMenu" => self.context_menu = flag()?,
            "quickHide" => self.quick_hide = flag()?,
            "hoverOpen" => self.hover_open = flag()?,
            other => return Err(format!("unknown setting {other}")),
        }
        Ok(())
    }

    pub fn vial(&self, id: &str) -> Option<&Vial> {
        self.vials.iter().find(|v| v.id == id)
    }

    /// Goes back to the vials, items and rules of `before`, but keeps what has changed since in how the
    /// vials sit on screen (place, size, collapsed) and in the settings: undo is about content.
    pub fn restore_content(&mut self, mut before: VialsConfig) {
        for vial in &mut before.vials {
            if let Some(now) = self.vials.iter().find(|v| v.id == vial.id) {
                vial.rect = now.rect;
                vial.monitor = now.monitor.clone();
                vial.collapsed = now.collapsed;
            }
        }
        self.vials = before.vials;
        self.rules = before.rules;
        self.declined = before.declined;
    }

    /// Whether the vial is locked (an unknown vial is not).
    pub fn is_locked(&self, id: &str) -> bool {
        self.vial(id).is_some_and(|v| v.locked)
    }

    /// Locks or unlocks a vial. Returns whether anything changed.
    pub fn set_locked(&mut self, id: &str, locked: bool) -> bool {
        match self.vials.iter_mut().find(|v| v.id == id) {
            Some(v) if v.locked != locked => {
                v.locked = locked;
                true
            }
            _ => false,
        }
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
                sort_key: None,
                sort_desc: false,
                folders_first: false,
                locked: false,
                tint: None,
                color: None,
                icon_size: None,
                title_center: false,
            },
        );
        true
    }

    /// Adds an empty vial at `rect` on `monitor` and returns its id. Names and ids are the smallest unused.
    pub fn add_vial(&mut self, rect: NormalizedRect, monitor: Option<String>) -> String {
        let id = crate::gesture::next_id(self.vials.iter().map(|v| v.id.as_str()));
        let name = crate::gesture::next_name(self.vials.iter().map(|v| v.name.as_str()));
        self.vials.push(Vial {
            id: id.clone(),
            name,
            monitor,
            rect: rect.sanitized(),
            items: Vec::new(),
            collapsed: false,
            inbox: false,
            sort_key: None,
            sort_desc: false,
            folders_first: false,
            locked: false,
            tint: None,
            color: None,
            icon_size: None,
            title_center: false,
        });
        id
    }

    /// Deletes a vial; its items go back on the desktop. The built-in Desktop vial cannot be deleted.
    pub fn delete_vial(&mut self, id: &str) -> bool {
        let before = self.vials.len();
        self.vials.retain(|v| v.id != id || v.inbox);
        self.vials.len() != before
    }

    /// Renames a vial (trimmed, 1..=80 characters). Returns whether the name changed.
    pub fn rename_vial(&mut self, id: &str, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 80 {
            return false;
        }
        match self.vials.iter_mut().find(|v| v.id == id && !v.inbox) {
            Some(v) if v.name != name => {
                v.name = name.to_string();
                true
            }
            _ => false,
        }
    }

    /// Adds a rule at the end of the list (rules are tried in order) and returns its id. `None` when the
    /// vial does not exist.
    pub fn add_rule(&mut self, name: &str, matcher: ItemMatcher, target_vial: &str) -> Option<String> {
        self.vial(target_vial)?;
        let taken: Vec<&str> = self.rules.iter().map(|r| r.id.as_str()).collect();
        let id = (1..).map(|n| format!("rule-{n}")).find(|id| !taken.contains(&id.as_str()))?;
        self.rules.push(SortRule { id: id.clone(), name: name.trim().to_string(), enabled: true, matcher, target_vial: target_vial.to_string() });
        Some(id)
    }

    pub fn delete_rule(&mut self, id: &str) -> bool {
        let before = self.rules.len();
        self.rules.retain(|r| r.id != id);
        self.rules.len() != before
    }

    pub fn set_rule_enabled(&mut self, id: &str, enabled: bool) -> bool {
        match self.rules.iter_mut().find(|r| r.id == id) {
            Some(r) if r.enabled != enabled => {
                r.enabled = enabled;
                true
            }
            _ => false,
        }
    }

    /// Moves a rule up (`-1`) or down (`1`) the list. Returns whether it moved.
    pub fn move_rule(&mut self, id: &str, delta: i32) -> bool {
        let Some(from) = self.rules.iter().position(|r| r.id == id) else { return false };
        let to = from as i32 + delta;
        if to < 0 || to as usize >= self.rules.len() || delta == 0 {
            return false;
        }
        self.rules.swap(from, to as usize);
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
    #[test]
    fn restoring_content_keeps_where_the_vials_are_and_the_settings() {
        let mut before = VialsConfig::default();
        before.vials.push(vial("a", &["x", "y"]));
        before.vials.push(vial("gone", &["z"]));
        before.theme = VialTheme::Classic;
        let mut now = before.clone();
        now.vials.remove(1);
        now.vials[0].items = vec!["x".into()];
        now.vials[0].rect = NormalizedRect { x: 0.5, y: 0.5, width: 0.2, height: 0.2 };
        now.vials[0].collapsed = true;
        now.theme = VialTheme::Glass;
        now.restore_content(before);
        assert_eq!(now.vials.len(), 2, "the deleted vial is back");
        assert_eq!(now.vials[0].items, vec!["x".to_string(), "y".to_string()], "with its items");
        assert_eq!(now.vials[0].rect.x, 0.5, "but where the window is now");
        assert!(now.vials[0].collapsed);
        assert_eq!(now.theme, VialTheme::Glass, "settings are not undone");
    }

    #[test]
    fn a_vial_look_has_defaults_and_ignores_values_it_does_not_know() {
        let mut v = vial("a", &[]);
        assert_eq!((v.icon_px(), v.color_rgb(), v.tint, v.title_center), (32, None, None, false));
        v.icon_size = Some(48);
        v.color = Some("#1f3b73".into());
        assert_eq!((v.icon_px(), v.color_rgb()), (48, Some(0x1f3b73)));
        v.icon_size = Some(40);
        v.color = Some("blue".into());
        assert_eq!((v.icon_px(), v.color_rgb()), (32, None), "unknown size and colour fall back");
        let json = r#"{"id":"v","name":"V","rect":{"x":0.0,"y":0.0,"width":0.2,"height":0.2},"items":[]}"#;
        let old: Vial = serde_json::from_str(json).unwrap();
        assert!(old.tint.is_none() && old.color.is_none() && old.icon_size.is_none() && !old.title_center);
        for (_, hex) in PANEL_COLORS {
            let mut p = vial("p", &[]);
            p.color = Some(hex.into());
            assert!(p.color_rgb().is_some(), "{hex}");
        }
    }

    #[test]
    fn a_vial_can_be_locked_and_an_old_config_without_the_field_loads_unlocked() {
        let mut cfg = VialsConfig::default();
        cfg.vials.push(vial("vial-1", &[]));
        assert!(!cfg.is_locked("vial-1"));
        assert!(cfg.set_locked("vial-1", true));
        assert!(cfg.is_locked("vial-1"));
        assert!(!cfg.set_locked("vial-1", true), "no change the second time");
        assert!(!cfg.set_locked("nope", true));
        let json = r#"{"id":"v","name":"V","rect":{"x":0.0,"y":0.0,"width":0.2,"height":0.2},"items":[]}"#;
        let old: Vial = serde_json::from_str(json).unwrap();
        assert!(!old.locked);
    }

    use super::*;

    #[test]
    fn settings_are_set_by_name_and_bad_values_are_refused() {
        let mut c = VialsConfig::default();
        c.set_setting("theme", "system").unwrap();
        assert_eq!(c.theme, VialTheme::System);
        c.set_setting("theme", "fluentLight").unwrap();
        c.set_setting("glassTint", "35").unwrap();
        c.set_setting("autoSort", "false").unwrap();
        c.set_setting("quickHide", "false").unwrap();
        c.set_setting("hoverOpen", "true").unwrap();
        assert!(c.hover_open);
        assert!(!c.quick_hide);
        assert_eq!((c.theme, c.glass_tint, c.auto_sort), (VialTheme::FluentLight, 35, false));
        assert!(c.set_setting("glassTint", "101").is_err());
        assert!(c.set_setting("theme", "neon").is_err());
        assert!(c.set_setting("autoSort", "maybe").is_err());
        assert!(c.set_setting("nope", "true").is_err());
    }

    fn vial(id: &str, items: &[&str]) -> Vial {
        Vial {
            id: id.into(),
            name: id.into(),
            monitor: None,
            rect: NormalizedRect { x: 0.0, y: 0.0, width: 0.2, height: 0.3 },
            items: items.iter().map(|s| s.to_string()).collect(),
            collapsed: false,
            inbox: false,
            sort_key: None,
            sort_desc: false,
            folders_first: false,
            locked: false,
            tint: None,
            color: None,
            icon_size: None,
            title_center: false,
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
    fn fluent_themes_round_trip_by_name() {
        let cfg: VialsConfig = serde_json::from_str(r#"{"theme":"fluentLight"}"#).unwrap();
        assert_eq!(cfg.theme, VialTheme::FluentLight);
        let cfg: VialsConfig = serde_json::from_str(r#"{"theme":"fluent"}"#).unwrap();
        assert_eq!(cfg.theme, VialTheme::Fluent);
        assert!(serde_json::to_string(&cfg).unwrap().contains("\"theme\":\"fluent\""));
    }

    #[test]
    fn vials_can_be_added_renamed_and_deleted() {
        let mut cfg = VialsConfig::default();
        let rect = NormalizedRect { x: 0.1, y: 0.1, width: 0.2, height: 0.2 };
        let a = cfg.add_vial(rect, Some("D1".into()));
        let b = cfg.add_vial(rect, None);
        assert_eq!((a.as_str(), b.as_str()), ("vial-1", "vial-2"));
        assert_eq!(cfg.vial("vial-2").unwrap().name, "Vial 2");
        assert!(cfg.rename_vial("vial-1", "  Work  "));
        assert!(!cfg.rename_vial("vial-1", "Work"), "same name: no change");
        assert!(!cfg.rename_vial("vial-1", "   "), "empty names are refused");
        cfg.assign("C:/f.txt", Some("vial-1"));
        assert!(cfg.delete_vial("vial-1"));
        assert!(cfg.vial_of("C:/f.txt").is_none(), "its items are free again");
        assert!(!cfg.delete_vial("vial-1"));
    }

    #[test]
    fn rules_can_be_added_toggled_moved_and_deleted() {
        let mut cfg = VialsConfig::default();
        cfg.add_vial(NormalizedRect { x: 0.1, y: 0.1, width: 0.2, height: 0.2 }, None);
        assert_eq!(cfg.add_rule("pdfs", ItemMatcher::Extension("pdf".into()), "nope"), None, "the vial must exist");
        let a = cfg.add_rule("pdfs", ItemMatcher::Extension("pdf".into()), "vial-1").unwrap();
        let b = cfg.add_rule("docs", ItemMatcher::Extension("docx".into()), "vial-1").unwrap();
        assert_eq!((a.as_str(), b.as_str()), ("rule-1", "rule-2"));
        assert!(cfg.set_rule_enabled(&a, false) && !cfg.set_rule_enabled(&a, false));
        assert!(cfg.move_rule(&b, -1) && cfg.rules[0].id == b);
        assert!(!cfg.move_rule(&b, -1), "already first");
        assert!(cfg.delete_rule(&a) && !cfg.delete_rule(&a));
        assert_eq!(cfg.add_rule("again", ItemMatcher::Extension("x".into()), "vial-1").as_deref(), Some("rule-1"), "ids are reused");
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
