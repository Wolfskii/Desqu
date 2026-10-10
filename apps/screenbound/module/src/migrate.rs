//! One-time move from the pre-rebrand settings folder (`%APPDATA%\com.screenbound.app`) to the
//! current one. The legacy folder is only ever *read*: the config is copied, never moved, so the
//! old app keeps working and the user keeps a backup.

use std::path::{Path, PathBuf};

pub const LEGACY_IDENTIFIER: &str = "com.screenbound.app";
pub const MARKER_FILE: &str = "MIGRATED.txt";
pub const LEGACY_JOURNAL_FILE: &str = "managed-windows.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationPlan {
    Skip,
    CopyConfig,
}

/// Never migrates twice and never overwrites a config the user already has.
pub fn plan_migration(
    new_cfg_exists: bool,
    legacy_cfg_exists: bool,
    marker_exists: bool,
) -> MigrationPlan {
    if new_cfg_exists || marker_exists || !legacy_cfg_exists {
        MigrationPlan::Skip
    } else {
        MigrationPlan::CopyConfig
    }
}

/// `roaming` is the per-user Roaming AppData folder (`app.path().config_dir()`).
pub fn legacy_dir(roaming: &Path) -> PathBuf {
    roaming.join(LEGACY_IDENTIFIER)
}

/// Copies `legacy_dir/config.json` to `new_config` when [`plan_migration`] says so, then leaves a
/// marker in the legacy folder. Returns whether it copied.
pub fn migrate_config(legacy_dir: &Path, new_config: &Path) -> std::io::Result<bool> {
    let legacy_config = legacy_dir.join("config.json");
    let marker = legacy_dir.join(MARKER_FILE);
    let plan = plan_migration(new_config.exists(), legacy_config.exists(), marker.exists());
    if plan == MigrationPlan::Skip {
        return Ok(false);
    }
    if let Some(parent) = new_config.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(&legacy_config, new_config)?;
    let new_dir = new_config.parent().unwrap_or(Path::new(""));
    std::fs::write(&marker, marker_text(now_secs(), new_dir))?;
    Ok(true)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub fn marker_text(unix_secs: u64, new_dir: &Path) -> String {
    format!(
        "Screenbound copied config.json from this folder on {}.\r\n\
         New settings folder: {}\r\n\
         This folder is left untouched as a backup; it is safe to delete once you are happy with the new version.\r\n",
        iso_utc(unix_secs),
        new_dir.display()
    )
}

/// `YYYY-MM-DDTHH:MM:SSZ` without a date/time dependency (civil-from-days algorithm).
pub fn iso_utc(unix_secs: u64) -> String {
    let days = (unix_secs / 86_400) as i64;
    let rem = unix_secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sb-migrate-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn plan_truth_table() {
        use MigrationPlan::*;
        // (new, legacy, marker)
        assert_eq!(plan_migration(false, true, false), CopyConfig);
        assert_eq!(plan_migration(true, true, false), Skip);
        assert_eq!(plan_migration(false, true, true), Skip);
        assert_eq!(plan_migration(false, false, false), Skip);
        assert_eq!(plan_migration(true, false, true), Skip);
    }

    #[test]
    fn legacy_dir_uses_the_old_identifier() {
        assert!(legacy_dir(Path::new("C:/Roaming")).ends_with("com.screenbound.app"));
    }

    #[test]
    fn copies_config_writes_marker_and_leaves_legacy_untouched() {
        let root = temp("copy");
        let legacy = root.join("legacy");
        let new_dir = root.join("new");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("config.json"), r#"{"zones":[]}"#).unwrap();

        assert!(migrate_config(&legacy, &new_dir.join("config.json")).unwrap());
        assert_eq!(std::fs::read_to_string(new_dir.join("config.json")).unwrap(), r#"{"zones":[]}"#);
        assert_eq!(std::fs::read_to_string(legacy.join("config.json")).unwrap(), r#"{"zones":[]}"#);
        let marker = std::fs::read_to_string(legacy.join(MARKER_FILE)).unwrap();
        assert!(marker.contains("safe to delete"));

        // Second run: marker present -> nothing happens, even if the new config is removed.
        std::fs::remove_file(new_dir.join("config.json")).unwrap();
        assert!(!migrate_config(&legacy, &new_dir.join("config.json")).unwrap());
        assert!(!new_dir.join("config.json").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn never_overwrites_an_existing_new_config() {
        let root = temp("keep");
        let legacy = root.join("legacy");
        let new_dir = root.join("new");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::create_dir_all(&new_dir).unwrap();
        std::fs::write(legacy.join("config.json"), "old").unwrap();
        std::fs::write(new_dir.join("config.json"), "mine").unwrap();
        assert!(!migrate_config(&legacy, &new_dir.join("config.json")).unwrap());
        assert_eq!(std::fs::read_to_string(new_dir.join("config.json")).unwrap(), "mine");
        assert!(!legacy.join(MARKER_FILE).exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn missing_legacy_folder_is_a_no_op() {
        let root = temp("none");
        assert!(!migrate_config(&root.join("nope"), &root.join("new").join("config.json")).unwrap());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn iso_utc_known_values() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_utc(1_000_000_000), "2001-09-09T01:46:40Z");
        assert_eq!(iso_utc(1_791_637_078), "2026-10-10T12:57:58Z");
    }
}
