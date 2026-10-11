//! Desqu's per-module settings and the pure rules built on them.
//!
//! Shell settings (start minimized, close to tray, updates) live in `shell.json`, owned by the
//! `desqu-shell` plugin. This file only holds what the collection adds: which modules are on and
//! which of them start with the system. They are kept apart because the shell plugin rewrites its
//! file wholesale and would otherwise drop this data.

use std::collections::BTreeMap;
use std::path::Path;

use desqu_shell::json::{write_json_atomic, JsonError};
use serde::{Deserialize, Serialize};

pub const MODULES_FILE: &str = "modules.json";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModuleSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub start_with_system: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModulesFile {
    #[serde(default)]
    pub modules: BTreeMap<String, ModuleSettings>,
}

impl ModulesFile {
    pub fn get(&self, id: &str) -> ModuleSettings {
        self.modules.get(id).copied().unwrap_or_default()
    }

    pub fn set(&mut self, id: &str, settings: ModuleSettings) {
        self.modules.insert(id.to_string(), settings);
    }
}

pub fn load(path: &Path) -> ModulesFile {
    let mut file = match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            tracing::warn!("ignoring unreadable {}: {e}", path.display());
            ModulesFile::default()
        }),
        Err(_) => ModulesFile::default(),
    };
    normalize(&mut file);
    file
}

/// "Start with system" only makes sense for an app that is turned on: clear it everywhere else.
pub fn normalize(file: &mut ModulesFile) {
    for settings in file.modules.values_mut() {
        settings.start_with_system &= settings.enabled;
    }
}

/// Turns a module on or off. Turning it off also clears its "start with system".
pub fn set_enabled(file: &mut ModulesFile, id: &str, enabled: bool) {
    let mut settings = file.get(id);
    settings.enabled = enabled;
    settings.start_with_system &= enabled;
    file.set(id, settings);
}

/// Sets "start with system" for one module. Refused (false) when asking for it on a module that is off.
pub fn set_start_with_system(file: &mut ModulesFile, id: &str, value: bool) -> bool {
    let mut settings = file.get(id);
    if value && !settings.enabled {
        return false;
    }
    settings.start_with_system = value;
    file.set(id, settings);
    true
}

pub fn save(path: &Path, file: &ModulesFile) -> Result<(), JsonError> {
    write_json_atomic(path, file)
}

/// Desqu itself needs a Run entry only when at least one module wants to start at sign-in.
pub fn should_autostart(file: &ModulesFile) -> bool {
    file.modules
        .values()
        .any(|m| m.enabled && m.start_with_system)
}

/// Which modules start when Desqu launches: from sign-in only those marked "start with system";
/// a manual launch starts everything that is on.
pub fn modules_to_start(file: &ModulesFile, autostart_launch: bool) -> Vec<String> {
    file.modules
        .iter()
        .filter(|(_, m)| m.enabled && (!autostart_launch || m.start_with_system))
        .map(|(id, _)| id.clone())
        .collect()
}

/// Sets "start with system" for every listed module that is on; turning it off applies to all.
pub fn set_all_start_with_system(file: &mut ModulesFile, ids: &[&str], value: bool) {
    for id in ids {
        let mut settings = file.get(id);
        settings.start_with_system = value && settings.enabled;
        file.set(id, settings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(entries: &[(&str, bool, bool)]) -> ModulesFile {
        let mut f = ModulesFile::default();
        for (id, enabled, sws) in entries {
            f.set(
                id,
                ModuleSettings {
                    enabled: *enabled,
                    start_with_system: *sws,
                },
            );
        }
        f
    }

    #[test]
    fn nothing_configured_means_no_autostart_and_nothing_to_start() {
        let f = ModulesFile::default();
        assert!(!should_autostart(&f));
        assert!(modules_to_start(&f, false).is_empty());
        assert!(modules_to_start(&f, true).is_empty());
    }

    #[test]
    fn autostart_needs_a_module_that_is_both_on_and_marked() {
        assert!(!should_autostart(&file(&[("a", true, false)])));
        assert!(!should_autostart(&file(&[("a", false, true)])));
        assert!(should_autostart(&file(&[
            ("a", true, true),
            ("b", false, false)
        ])));
    }

    #[test]
    fn manual_launch_starts_everything_that_is_on() {
        let f = file(&[("a", true, false), ("b", true, true), ("c", false, true)]);
        assert_eq!(modules_to_start(&f, false), ["a", "b"]);
    }

    #[test]
    fn sign_in_launch_starts_only_marked_modules_that_are_on() {
        let f = file(&[("a", true, false), ("b", true, true), ("c", false, true)]);
        assert_eq!(modules_to_start(&f, true), ["b"]);
    }

    #[test]
    fn set_all_start_with_system_touches_only_the_listed_modules_that_are_on() {
        let mut f = file(&[("a", true, false), ("x", true, false)]);
        set_all_start_with_system(&mut f, &["a", "b"], true);
        assert!(f.get("a").start_with_system && f.get("a").enabled);
        assert!(!f.get("b").start_with_system, "an app that is off never starts with the system");
        assert!(!f.get("x").start_with_system);
    }

    #[test]
    fn start_with_system_is_refused_for_an_app_that_is_off_and_cleared_when_it_is_turned_off() {
        let mut f = file(&[("a", false, false), ("b", true, true)]);
        assert!(!set_start_with_system(&mut f, "a", true));
        assert!(!f.get("a").start_with_system);
        assert!(set_start_with_system(&mut f, "b", false));
        set_enabled(&mut f, "b", true);
        assert!(set_start_with_system(&mut f, "b", true));
        set_enabled(&mut f, "b", false);
        assert!(!f.get("b").start_with_system && !f.get("b").enabled);
    }

    #[test]
    fn old_files_with_start_with_system_on_an_app_that_is_off_are_normalized() {
        let mut f = file(&[("a", false, true), ("b", true, true)]);
        normalize(&mut f);
        assert!(!f.get("a").start_with_system && f.get("b").start_with_system);
    }

    #[test]
    fn serializes_camel_case_and_ignores_unknown_fields() {
        let json = serde_json::to_value(file(&[("a", true, true)])).unwrap();
        assert_eq!(json["modules"]["a"]["startWithSystem"], true);
        let parsed: ModulesFile =
            serde_json::from_str(r#"{"modules":{"a":{"enabled":true,"extra":1}},"other":2}"#)
                .unwrap();
        assert!(parsed.get("a").enabled);
    }

    #[test]
    fn round_trips_through_a_file_and_survives_corruption() {
        let dir = std::env::temp_dir().join(format!("desqu-modules-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join(MODULES_FILE);
        assert_eq!(load(&path), ModulesFile::default());
        let f = file(&[("a", true, true)]);
        save(&path, &f).unwrap();
        assert_eq!(load(&path), f);
        std::fs::write(&path, "{ nope").unwrap();
        assert_eq!(load(&path), ModulesFile::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
