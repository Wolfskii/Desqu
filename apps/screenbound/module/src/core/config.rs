use std::path::Path;

use serde::{Deserialize, Serialize};

use super::rules::{Action, BorderlessAction, BorderlessTarget, Matcher, RuleScope, WindowRule};
use super::transition::TransitionSpeed;
use super::window::ChromeMode;
use super::zone::{NormalizedRect, Zone, ZoneReference, ZoneUnit};

/// v2: `startAtBoot` moved out to the shell's settings (`shell.json`).
pub const CONFIG_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    /// Global kill switch for all automatic window management.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// How entering and leaving the zone is animated.
    #[serde(default)]
    pub transition: TransitionSpeed,
    /// v1 only: read so it can be handed to the shell once, never written back.
    #[serde(rename = "startAtBoot", default, skip_serializing)]
    pub legacy_start_at_boot: Option<bool>,
    pub zones: Vec<Zone>,
    pub rules: Vec<WindowRule>,
    /// Named groups apps can be placed in. A rule can target one or more of these.
    #[serde(default)]
    pub groups: Vec<AppGroup>,
    /// Apps Screenbound has seen. Stored with the rest of the config so the list survives
    /// reinstalls; the uninstaller deletes it only when "Delete app data" is checked.
    #[serde(default)]
    pub known_apps: Vec<KnownApp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppGroup {
    pub id: String,
    pub name: String,
    /// `#rrggbb`.
    pub color: String,
    /// Key into the fixed icon set shown in the Apps tab.
    pub icon: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownApp {
    pub process_name: String,
    pub title: String,
    /// Turned on in the Apps list. A rule only matches an app that is on.
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
}

fn default_version() -> u32 {
    CONFIG_VERSION
}

fn default_true() -> bool {
    true
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            enabled: true,
            transition: TransitionSpeed::default(),
            legacy_start_at_boot: None,
            zones: vec![Zone {
                id: "zone-left-75".into(),
                name: "75% Left".into(),
                rect: NormalizedRect {
                    x: 0.0,
                    y: 0.0,
                    width: 0.75,
                    height: 1.0,
                },
                unit: ZoneUnit::Percent,
                pixels: None,
                monitor: None,
                reference: ZoneReference::Monitor,
            }],
            rules: vec![WindowRule {
                id: "rule-all".into(),
                name: "All apps".into(),
                enabled: true,
                matcher: Matcher::Any(vec![]),
                scope: Some(RuleScope::All),
                actions: vec![Action::FullscreenZone {
                    zone_id: "zone-left-75".into(),
                    chrome: ChromeMode::Keep,
                    use_snapped_zone: true,
                }],
            }],
            groups: Vec::new(),
            known_apps: Vec::new(),
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

impl AppConfig {
    /// Takes the v1 `startAtBoot` value (once). The caller passes it to the shell settings and
    /// saves the config, which then no longer contains the key.
    pub fn take_legacy_start_at_boot(&mut self) -> Option<bool> {
        self.legacy_start_at_boot.take()
    }

    /// Reads `path`, and if it still holds a v1 `startAtBoot`, rewrites it as v2 without the key
    /// and returns the value. `Ok(None)` when there is no file or nothing to migrate.
    pub fn migrate_legacy_start_at_boot(path: &Path) -> Result<Option<bool>, ConfigError> {
        if !path.exists() {
            return Ok(None);
        }
        let mut config = Self::load(path)?;
        let legacy = config.take_legacy_start_at_boot();
        if legacy.is_some() {
            config.save(path)?;
        }
        Ok(legacy)
    }

    pub fn zone(&self, id: &str) -> Option<&Zone> {
        self.zones.iter().find(|z| z.id == id)
    }

    /// Normalizes zone geometry and rejects structurally broken configs (duplicate ids,
    /// rules pointing at missing zones).
    pub fn validated(mut self) -> Result<Self, ConfigError> {
        let mut seen = std::collections::HashSet::new();
        for zone in &mut self.zones {
            if zone.id.trim().is_empty() {
                return Err(ConfigError::Invalid("zone with empty id".into()));
            }
            if !seen.insert(zone.id.clone()) {
                return Err(ConfigError::Invalid(format!(
                    "duplicate zone id '{}'",
                    zone.id
                )));
            }
            zone.rect = zone.rect.sanitized();
            zone.pixels = zone.pixels.map(|p| p.sanitized());
        }
        let mut seen_groups = std::collections::HashSet::new();
        for group in &mut self.groups {
            if group.id.trim().is_empty() {
                return Err(ConfigError::Invalid("group with empty id".into()));
            }
            if !seen_groups.insert(group.id.clone()) {
                return Err(ConfigError::Invalid(format!(
                    "duplicate group id '{}'",
                    group.id
                )));
            }
            group.name = group.name.trim().to_string();
            if group.name.is_empty() {
                group.name = "Group".into();
            }
            if !is_hex_color(&group.color) {
                group.color = "#4fa3ff".into();
            }
            if !GROUP_ICONS.contains(&group.icon.as_str()) {
                group.icon = "apps".into();
            }
        }
        let mut seen = std::collections::HashSet::new();
        for rule in &self.rules {
            if !seen.insert(rule.id.clone()) {
                return Err(ConfigError::Invalid(format!(
                    "duplicate rule id '{}'",
                    rule.id
                )));
            }
            if let Some((zone_id, _)) = rule.fullscreen_zone() {
                if self.zone(zone_id).is_none() {
                    return Err(ConfigError::Invalid(format!(
                        "rule '{}' references unknown zone '{zone_id}'",
                        rule.name
                    )));
                }
            }
            if let Some(BorderlessAction {
                target: BorderlessTarget::Zone { zone_id },
                ..
            }) = rule.borderless_action()
            {
                if self.zone(zone_id).is_none() {
                    return Err(ConfigError::Invalid(format!(
                        "rule '{}' references unknown zone '{zone_id}'",
                        rule.name
                    )));
                }
            }
        }
        let mut seen_apps = std::collections::HashMap::<String, KnownApp>::new();
        for app in self.known_apps.drain(..) {
            let name = app.process_name.trim().to_string();
            if name.is_empty() {
                continue;
            }
            let group_id = app.group_id.filter(|id| seen_groups.contains(id));
            seen_apps.insert(
                name.to_lowercase(),
                KnownApp {
                    process_name: name,
                    title: app.title,
                    enabled: app.enabled,
                    group_id,
                },
            );
        }
        self.known_apps = seen_apps.into_values().collect();
        self.known_apps
            .sort_by_key(|app| app.process_name.to_lowercase());
        for rule in &mut self.rules {
            if let Some(RuleScope::Groups { group_ids }) = &mut rule.scope {
                group_ids.retain(|id| seen_groups.contains(id));
            }
        }
        self.version = CONFIG_VERSION;
        Ok(self)
    }

    pub fn match_rule(&self, id: &super::window::WindowIdentity) -> Option<&WindowRule> {
        self.match_rule_where(id, |_| true)
    }

    /// First rule with a fullscreen-zone action that matches the window.
    pub fn match_fullscreen_rule(&self, id: &super::window::WindowIdentity) -> Option<&WindowRule> {
        self.match_rule_where(id, |r| r.fullscreen_action().is_some())
    }

    /// First rule with a borderless action that matches the window.
    pub fn match_borderless_rule(&self, id: &super::window::WindowIdentity) -> Option<&WindowRule> {
        self.match_rule_where(id, |r| r.borderless_action().is_some())
    }

    fn match_rule_where(
        &self,
        id: &super::window::WindowIdentity,
        wanted: impl Fn(&WindowRule) -> bool,
    ) -> Option<&WindowRule> {
        let known = self
            .known_apps
            .iter()
            .find(|app| app.process_name.eq_ignore_ascii_case(&id.process_name));
        super::rules::first_match_where(
            &self.rules,
            id,
            known.map(|app| (app.enabled, app.group_id.as_deref())),
            wanted,
        )
    }

    /// Loads config; a missing file yields defaults. A corrupt file is an error so the caller
    /// can back it up instead of silently overwriting the user's settings.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str::<AppConfig>(&text)?.validated(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        write_json_atomic(path, self)
    }
}

const GROUP_ICONS: &[&str] = &[
    "apps", "browser", "video", "game", "music", "chat", "folder", "star",
];

fn is_hex_color(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next() == Some('#') && chars.clone().count() == 6 && chars.all(|c| c.is_ascii_hexdigit())
}

/// Writes via temp file + rename so a crash mid-write cannot leave a truncated file.
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), ConfigError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid_and_roundtrips() {
        let cfg = AppConfig::default().validated().unwrap();
        let json = serde_json::to_string(&cfg).unwrap();
        assert_eq!(serde_json::from_str::<AppConfig>(&json).unwrap(), cfg);
    }

    #[test]
    fn default_has_no_preset_apps() {
        let cfg = AppConfig::default();
        assert!(cfg.known_apps.is_empty());
        assert!(cfg.groups.is_empty());
        assert_eq!(cfg.rules.len(), 1);
        assert!(matches!(cfg.rules[0].scope, Some(RuleScope::All)));
    }

    #[test]
    fn rejects_dangling_zone_reference() {
        use crate::core::rules::{Action, Matcher};
        use crate::core::window::ChromeMode;
        let mut cfg = AppConfig::default();
        cfg.zones.clear();
        cfg.rules.push(WindowRule {
            id: "r".into(),
            name: "Apps".into(),
            enabled: true,
            matcher: Matcher::ProcessName("vlc.exe".into()),
            scope: None,
            actions: vec![Action::FullscreenZone {
                zone_id: "missing".into(),
                chrome: ChromeMode::Keep,
                use_snapped_zone: true,
            }],
        });
        assert!(matches!(cfg.validated(), Err(ConfigError::Invalid(_))));
    }

    #[test]
    fn rejects_duplicate_zone_ids() {
        let mut cfg = AppConfig::default();
        cfg.zones.push(cfg.zones[0].clone());
        assert!(cfg.validated().is_err());
    }

    #[test]
    fn save_and_load() {
        let dir = std::env::temp_dir().join(format!("screenbound-test-{}", std::process::id()));
        let path = dir.join("config.json");
        let cfg = AppConfig {
            enabled: false,
            ..Default::default()
        };
        cfg.save(&path).unwrap();
        assert_eq!(AppConfig::load(&path).unwrap(), cfg);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn missing_file_yields_default() {
        let path = std::env::temp_dir()
            .join("screenbound-does-not-exist")
            .join("config.json");
        assert_eq!(AppConfig::load(&path).unwrap(), AppConfig::default());
    }

    #[test]
    fn v1_start_at_boot_is_taken_once_and_not_written_back() {
        let mut cfg: AppConfig = serde_json::from_str(
            r#"{"version":1,"startAtBoot":true,"zones":[],"rules":[]}"#,
        )
        .unwrap();
        assert_eq!(cfg.take_legacy_start_at_boot(), Some(true));
        assert_eq!(cfg.take_legacy_start_at_boot(), None);
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(!json.contains("startAtBoot"), "{json}");
    }

    #[test]
    fn v2_config_has_nothing_to_migrate() {
        let mut cfg = AppConfig::default();
        assert_eq!(cfg.take_legacy_start_at_boot(), None);
        assert_eq!(cfg.version, CONFIG_VERSION);
    }

    #[test]
    fn migrating_a_file_rewrites_it_as_v2_and_keeps_everything_else() {
        let dir = std::env::temp_dir().join(format!("sb-config-migrate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");

        let mut original = AppConfig::default();
        original.groups.push(AppGroup {
            id: "g1".into(),
            name: "Browsers".into(),
            color: "#4fa3ff".into(),
            icon: "apps".into(),
        });
        let mut v1 = serde_json::to_value(&original).unwrap();
        v1["version"] = 1.into();
        v1["startAtBoot"] = true.into();
        std::fs::write(&path, serde_json::to_vec(&v1).unwrap()).unwrap();

        assert_eq!(AppConfig::migrate_legacy_start_at_boot(&path).unwrap(), Some(true));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("startAtBoot"));
        let migrated = AppConfig::load(&path).unwrap();
        assert_eq!(migrated.version, CONFIG_VERSION);
        assert_eq!(migrated.zones, original.zones);
        assert_eq!(migrated.groups, original.groups);
        // Second run is a no-op.
        assert_eq!(AppConfig::migrate_legacy_start_at_boot(&path).unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn migrating_a_missing_file_is_a_no_op() {
        let path = std::env::temp_dir().join("sb-config-does-not-exist.json");
        let _ = std::fs::remove_file(&path);
        assert_eq!(AppConfig::migrate_legacy_start_at_boot(&path).unwrap(), None);
    }

    fn borderless_rule(id: &str, target: BorderlessTarget) -> WindowRule {
        WindowRule {
            id: id.into(),
            name: id.into(),
            enabled: true,
            matcher: Matcher::ProcessName("game.exe".into()),
            scope: None,
            actions: vec![Action::Borderless { target, topmost: false }],
        }
    }

    #[test]
    fn borderless_rule_with_unknown_zone_is_rejected() {
        let mut cfg = AppConfig::default();
        cfg.rules.push(borderless_rule("b", BorderlessTarget::Zone { zone_id: "nope".into() }));
        assert!(matches!(cfg.validated(), Err(ConfigError::Invalid(m)) if m.contains("unknown zone")));
    }

    #[test]
    fn borderless_rule_on_a_known_zone_or_monitor_is_valid() {
        let mut cfg = AppConfig::default();
        let zone_id = cfg.zones[0].id.clone();
        cfg.rules.push(borderless_rule("m", BorderlessTarget::Monitor));
        cfg.rules.push(borderless_rule("z", BorderlessTarget::Zone { zone_id }));
        assert!(cfg.validated().is_ok());
    }

    #[test]
    fn fullscreen_and_borderless_rules_do_not_shadow_each_other() {
        use super::super::rules::test_identity;
        let mut cfg = AppConfig::default();
        // A borderless rule listed first must not hide the fullscreen rule below it.
        cfg.rules.clear();
        cfg.rules.push(borderless_rule("b", BorderlessTarget::Monitor));
        let zone_id = cfg.zones[0].id.clone();
        cfg.rules.push(WindowRule {
            id: "f".into(),
            name: "f".into(),
            enabled: true,
            matcher: Matcher::ProcessName("game.exe".into()),
            scope: None,
            actions: vec![Action::FullscreenZone {
                zone_id,
                chrome: ChromeMode::Keep,
                use_snapped_zone: true,
            }],
        });
        let id = test_identity("game.exe", "UnrealWindow", "Game");
        assert_eq!(cfg.match_borderless_rule(&id).map(|r| r.id.as_str()), Some("b"));
        assert_eq!(cfg.match_fullscreen_rule(&id).map(|r| r.id.as_str()), Some("f"));
    }
}
