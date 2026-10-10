//! Shell settings model and store. One file per app: `shell.json` (standalone) or the shell part
//! of `settings.json` (Desqu). Policy matches Screenbound's config: missing file means defaults,
//! a corrupt file is an error (never silently overwritten).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::json::{write_json_atomic, JsonError};

pub const SETTINGS_VERSION: u32 = 1;

fn default_version() -> u32 {
    SETTINGS_VERSION
}

fn yes() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettings {
    #[serde(default = "yes")]
    pub check_automatically: bool,
    #[serde(default)]
    pub install_automatically: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            check_automatically: true,
            install_automatically: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShellSettings {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub start_with_system: bool,
    #[serde(default)]
    pub start_minimized: bool,
    /// `true` keeps today's behaviour: the close button hides the window to the tray.
    #[serde(default = "yes")]
    pub close_to_tray: bool,
    #[serde(default)]
    pub updates: UpdateSettings,
}

impl Default for ShellSettings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            start_with_system: false,
            start_minimized: false,
            close_to_tray: true,
            updates: UpdateSettings::default(),
        }
    }
}

pub fn load(path: &Path) -> Result<ShellSettings, JsonError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(serde_json::from_str(&text)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ShellSettings::default()),
        Err(e) => Err(e.into()),
    }
}

pub fn save(path: &Path, settings: &ShellSettings) -> Result<(), JsonError> {
    write_json_atomic(path, settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("desqu-shell-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("shell.json")
    }

    #[test]
    fn defaults_keep_current_behaviour() {
        let s = ShellSettings::default();
        assert!(!s.start_with_system);
        assert!(!s.start_minimized);
        assert!(s.close_to_tray);
        assert!(s.updates.check_automatically);
        assert!(!s.updates.install_automatically);
    }

    #[test]
    fn empty_object_yields_defaults() {
        let s: ShellSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, ShellSettings::default());
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let s: ShellSettings =
            serde_json::from_str(r#"{"startWithSystem":true,"somethingNew":42}"#).unwrap();
        assert!(s.start_with_system);
        assert!(s.close_to_tray);
    }

    #[test]
    fn serializes_camel_case() {
        let json = serde_json::to_value(ShellSettings::default()).unwrap();
        assert_eq!(json["closeToTray"], true);
        assert_eq!(json["updates"]["checkAutomatically"], true);
    }

    #[test]
    fn missing_file_is_default_and_round_trips() {
        let path = temp_path("roundtrip");
        assert_eq!(load(&path).unwrap(), ShellSettings::default());

        let settings = ShellSettings {
            start_with_system: true,
            close_to_tray: false,
            ..ShellSettings::default()
        };
        save(&path, &settings).unwrap();
        assert_eq!(load(&path).unwrap(), settings);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let path = temp_path("corrupt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ not json").unwrap();
        assert!(load(&path).is_err());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
