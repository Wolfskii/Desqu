//! Things left over from the pre-rebrand Screenbound (0.1.0): a still-running old process, a
//! separate old install, and the old settings folder that migration keeps as a backup.
//!
//! The old app has a different single-instance identifier and knows nothing about the module
//! lock, so a running copy would fight the new engine over the same windows. We never kill it:
//! a plain close only hides it to the tray and a force-kill would skip its window restore.

use std::path::{Path, PathBuf};

use desqu_shell::platform::{self, ProcessInfo};
use serde::Serialize;

use crate::migrate::{LEGACY_IDENTIFIER, MARKER_FILE};

pub const EXE_NAME: &str = "screenbound.exe";
/// Uninstall key of 0.1.0 (its product name). It differs from the current one only by case,
/// which Windows ignores, so after an in-place upgrade this *is* the current install.
pub const OLD_UNINSTALL_KEY: &str = "ScreenBound";
pub const OLDER_RUNNING_MESSAGE: &str =
    "An older Screenbound is running. Right-click its tray icon and choose Exit, then try again.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OldInstall {
    pub version: Option<String>,
    pub uninstall_string: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyStatus {
    /// Another `screenbound.exe` (not this process) is running.
    pub older_running: bool,
    /// An install other than the one this process runs from.
    pub old_install: Option<OldInstall>,
    /// The old settings folder, kept as a backup after migration.
    pub settings_backup: Option<String>,
}

/// Processes with the Screenbound exe name other than `own_pid`.
pub fn other_instances(own_pid: u32, procs: &[ProcessInfo]) -> Vec<&ProcessInfo> {
    procs.iter().filter(|p| p.pid != own_pid).collect()
}

fn normalize(path: &str) -> String {
    path.trim()
        .trim_matches('"')
        .trim_end_matches(['\\', '/'])
        .replace('/', "\\")
        .to_lowercase()
}

/// `true` when `install_location` is not the folder `own_exe` lives in.
pub fn is_other_install(install_location: &str, own_exe: &Path) -> bool {
    let Some(own_dir) = own_exe.parent() else {
        return true;
    };
    normalize(install_location) != normalize(&own_dir.to_string_lossy())
}

pub fn older_instance_running() -> bool {
    let procs = platform::find_processes(EXE_NAME);
    !other_instances(std::process::id(), &procs).is_empty()
}

fn old_install() -> Option<OldInstall> {
    let entry = platform::read_uninstall_entry(OLD_UNINSTALL_KEY)?;
    let uninstall_string = entry.uninstall_string?;
    let own_exe = std::env::current_exe().ok()?;
    match entry.install_location {
        Some(location) if !is_other_install(&location, &own_exe) => None,
        _ => Some(OldInstall { version: entry.display_version, uninstall_string }),
    }
}

/// The folder only counts as a backup once migration left its marker there.
pub fn settings_backup(legacy_dir: &Path) -> Option<PathBuf> {
    legacy_dir.join(MARKER_FILE).exists().then(|| legacy_dir.to_path_buf())
}

pub fn status(legacy_dir: Option<&Path>) -> LegacyStatus {
    LegacyStatus {
        older_running: older_instance_running(),
        old_install: old_install(),
        settings_backup: legacy_dir
            .and_then(settings_backup)
            .map(|p| p.to_string_lossy().into_owned()),
    }
}

pub fn uninstall_old() -> Result<(), String> {
    let install = old_install().ok_or("no other Screenbound installation found")?;
    platform::launch_uninstaller(&install.uninstall_string)
}

/// Deletes the old settings folder. Refuses anything that is not the legacy folder with a
/// migration marker, so a wrong path can never delete unrelated data.
pub fn delete_settings_backup(legacy_dir: &Path) -> Result<(), String> {
    let is_legacy_name = legacy_dir
        .file_name()
        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(LEGACY_IDENTIFIER));
    if !is_legacy_name {
        return Err(format!("{} is not the legacy settings folder", legacy_dir.display()));
    }
    if settings_backup(legacy_dir).is_none() {
        return Err("settings were not migrated from this folder; refusing to delete it".into());
    }
    std::fs::remove_dir_all(legacy_dir).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32) -> ProcessInfo {
        ProcessInfo { pid, exe: None }
    }

    #[test]
    fn own_process_is_not_an_older_instance() {
        let procs = [proc(10), proc(20)];
        assert!(other_instances(10, &procs[..1]).is_empty());
        assert_eq!(other_instances(10, &procs).len(), 1);
        assert_eq!(other_instances(10, &procs)[0].pid, 20);
    }

    #[test]
    fn install_location_comparison_ignores_case_quotes_and_slashes() {
        let exe = Path::new(r"C:\Users\me\AppData\Local\Screenbound\screenbound.exe");
        assert!(!is_other_install(r#""C:\Users\me\AppData\Local\ScreenBound""#, exe));
        assert!(!is_other_install(r"c:/users/me/appdata/local/screenbound/", exe));
        assert!(is_other_install(r"D:\Apps\Screenbound", exe));
    }

    #[test]
    fn settings_backup_needs_the_marker() {
        let dir = std::env::temp_dir().join(format!("sb-legacy-{}-marker", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(settings_backup(&dir), None);
        std::fs::write(dir.join(MARKER_FILE), "x").unwrap();
        assert_eq!(settings_backup(&dir), Some(dir.clone()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn delete_refuses_wrong_folders_and_unmigrated_ones() {
        let root = std::env::temp_dir().join(format!("sb-legacy-{}-delete", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let wrong = root.join("some-other-folder");
        let right = root.join(LEGACY_IDENTIFIER);
        std::fs::create_dir_all(&wrong).unwrap();
        std::fs::create_dir_all(&right).unwrap();
        std::fs::write(wrong.join(MARKER_FILE), "x").unwrap();

        assert!(delete_settings_backup(&wrong).is_err());
        assert!(wrong.exists());
        // Right name but no marker: settings were never migrated, keep them.
        assert!(delete_settings_backup(&right).is_err());
        assert!(right.exists());

        std::fs::write(right.join(MARKER_FILE), "x").unwrap();
        assert!(delete_settings_backup(&right).is_ok());
        assert!(!right.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn status_serializes_camel_case() {
        let json = serde_json::to_value(LegacyStatus::default()).unwrap();
        assert_eq!(json["olderRunning"], false);
        assert!(json["oldInstall"].is_null());
        assert!(json["settingsBackup"].is_null());
    }
}
