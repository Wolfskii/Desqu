//! Backing up and restoring the settings of all the apps as one zip, in a folder in Documents, and a copy in
//! OneDrive (when there is one) that another PC can restore. A backup holds the four settings files; a restore
//! checks each of them by loading it before it replaces anything.

use std::path::{Path, PathBuf};

use screenbound_module::core::config::AppConfig;
use vials_core::model::VialsConfig;
use workspaces_core::model::WorkspacesConfig;

/// Where the four settings files are.
#[derive(Debug, Clone)]
pub struct Paths {
    pub vials: PathBuf,
    pub workspaces: PathBuf,
    pub screenbound: PathBuf,
    pub desqu: PathBuf,
}

/// The names the files have in a backup.
const NAMES: [&str; 4] = ["vials.json", "workspaces.json", "screenbound.json", "desqu.json"];

impl Paths {
    pub fn real() -> Paths {
        let appdata = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default();
        Paths {
            vials: crate::app_settings::vials_path(),
            workspaces: crate::app_settings::workspaces_path(),
            screenbound: crate::screenbound_remote::config_path(),
            desqu: appdata.join("io.github.wolfskii.desqu").join("gpui-shell.json"),
        }
    }

    fn entries(&self) -> [(&'static str, &Path); 4] {
        [(NAMES[0], &self.vials), (NAMES[1], &self.workspaces), (NAMES[2], &self.screenbound), (NAMES[3], &self.desqu)]
    }
}

/// The folder backups go in.
pub fn folder() -> PathBuf {
    std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default().join("Documents").join("Desqu backups")
}

/// The OneDrive folder, if there is one: the copy that another PC can restore lives in `Desqu` there.
pub fn onedrive_folder() -> Option<PathBuf> {
    ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"].iter().find_map(|v| std::env::var_os(v).map(PathBuf::from)).filter(|p| p.is_dir()).map(|p| p.join("Desqu"))
}

fn run_powershell(script: &str, items: &str, target: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("DESQU_ITEMS", items)
        .env("DESQU_TARGET", target)
        .creation_flags(0x0800_0000)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// A backup named `desqu-settings-<stamp>.zip` in `into`. Files that do not exist yet are left out.
pub fn make(paths: &Paths, into: &Path, stamp: u64) -> Result<PathBuf, String> {
    std::fs::create_dir_all(into).map_err(|e| e.to_string())?;
    let work = std::env::temp_dir().join(format!("desqu-backup-{}-{stamp}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let mut copied = Vec::new();
    for (name, path) in paths.entries() {
        if path.is_file() {
            let to = work.join(name);
            std::fs::copy(path, &to).map_err(|e| format!("{name}: {e}"))?;
            copied.push(to.to_string_lossy().into_owned());
        }
    }
    if copied.is_empty() {
        let _ = std::fs::remove_dir_all(&work);
        return Err("there are no settings to save yet".into());
    }
    let zip = into.join(format!("desqu-settings-{stamp}.zip"));
    let result = run_powershell("Compress-Archive -LiteralPath ($env:DESQU_ITEMS -split '\\|') -DestinationPath $env:DESQU_TARGET -Force -ErrorAction Stop", &copied.join("|"), &zip);
    let _ = std::fs::remove_dir_all(&work);
    result.map(|()| zip)
}

/// Whether the text of a settings file is one the app can read.
fn readable(name: &str, file: &Path) -> Result<(), String> {
    match name {
        "vials.json" => VialsConfig::load(file).map(drop).map_err(|e| e.to_string()),
        "workspaces.json" => WorkspacesConfig::load(file).map(drop).map_err(|e| e.to_string()),
        "screenbound.json" => AppConfig::load(file).map(drop).map_err(|e| e.to_string()),
        _ => std::fs::read_to_string(file).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<serde_json::Value>(&t).map(drop).map_err(|e| e.to_string())),
    }
}

/// Puts the settings of a backup in place. Every file in it is read first; if one cannot be read nothing is
/// replaced. Returns the names of the files that were restored. The apps must not be running: they would write
/// their own settings over these.
pub fn restore(paths: &Paths, zip: &Path) -> Result<Vec<&'static str>, String> {
    let work = std::env::temp_dir().join(format!("desqu-restore-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let result = (|| {
        run_powershell("Expand-Archive -LiteralPath $env:DESQU_ITEMS -DestinationPath $env:DESQU_TARGET -Force -ErrorAction Stop", &zip.to_string_lossy(), &work)?;
        let mut found = Vec::new();
        for (name, target) in paths.entries() {
            let source = work.join(name);
            if source.is_file() {
                readable(name, &source).map_err(|e| format!("{name} in the backup cannot be read: {e}"))?;
                found.push((name, source, target));
            }
        }
        if found.is_empty() {
            return Err("this zip holds none of the settings files".to_string());
        }
        let mut done = Vec::new();
        for (name, source, target) in found {
            if let Some(dir) = target.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            // Beside the file first, so a failure half way leaves the old file whole.
            let tmp = target.with_extension("json.restoring");
            std::fs::copy(&source, &tmp).map_err(|e| format!("{name}: {e}"))?;
            std::fs::rename(&tmp, target).map_err(|e| format!("{name}: {e}"))?;
            done.push(name);
        }
        Ok(done)
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}

/// The backups in `folder`, newest first.
pub fn list(folder: &Path) -> Vec<PathBuf> {
    let mut found: Vec<(u64, PathBuf)> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let stamp = path.file_name()?.to_str()?.strip_prefix("desqu-settings-")?.strip_suffix(".zip")?.parse().ok()?;
            Some((stamp, path))
        })
        .collect();
    found.sort_by_key(|a| std::cmp::Reverse(a.0));
    found.into_iter().map(|f| f.1).collect()
}

/// When a backup was made, as seconds since the epoch (from its name).
pub fn stamp_of(zip: &Path) -> Option<u64> {
    zip.file_name()?.to_str()?.strip_prefix("desqu-settings-")?.strip_suffix(".zip")?.parse().ok()
}

/// Deletes all but the newest `keep` backups.
pub fn prune(folder: &Path, keep: usize) {
    for old in list(folder).into_iter().skip(keep) {
        let _ = std::fs::remove_file(old);
    }
}

/// A backup is due when there is none, or the newest is older than `every` seconds.
pub fn is_due(folder: &Path, now: u64, every: u64) -> bool {
    list(folder).first().and_then(|z| stamp_of(z)).is_none_or(|made| now.saturating_sub(made) >= every)
}

/// Makes a backup in the backup folder (and, when asked and there is OneDrive, a copy there), and keeps only the
/// newest few of each. Returns the backup.
pub fn back_up(paths: &Paths, also_onedrive: bool) -> Result<PathBuf, String> {
    let stamp = now_secs();
    let zip = make(paths, &folder(), stamp)?;
    prune(&folder(), 14);
    if also_onedrive {
        if let Some(cloud) = onedrive_folder() {
            if std::fs::create_dir_all(&cloud).is_ok() && std::fs::copy(&zip, cloud.join(zip.file_name().unwrap_or_default())).is_ok() {
                prune(&cloud, 5);
            }
        }
    }
    Ok(zip)
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(name: &str) -> (PathBuf, Paths) {
        let root = std::env::temp_dir().join(format!("desqu-backup-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let paths = Paths { vials: root.join("a/vials.json"), workspaces: root.join("b/workspaces.json"), screenbound: root.join("c/screenbound.json"), desqu: root.join("d/shell.json") };
        for (file, text) in [(&paths.vials, r#"{"version":1,"vials":[],"rules":[],"theme":"classic"}"#), (&paths.workspaces, r#"{"version":1,"workspaces":[]}"#), (&paths.desqu, r#"{"vials":true}"#)] {
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
        (root, paths)
    }

    #[test]
    fn a_backup_holds_the_settings_that_exist_and_a_restore_puts_them_back() {
        let (root, paths) = setup("roundtrip");
        let folder = root.join("backups");
        let zip = make(&paths, &folder, 1000).expect("a backup is made");
        assert!(zip.is_file());
        // The settings change, then the backup is restored.
        std::fs::write(&paths.vials, r#"{"version":1,"vials":[],"rules":[],"theme":"glass"}"#).unwrap();
        std::fs::remove_file(&paths.desqu).unwrap();
        let restored = restore(&paths, &zip).expect("it is restored");
        assert_eq!(restored, vec!["vials.json", "workspaces.json", "desqu.json"], "the Screenbound file did not exist, so it is not in the backup");
        assert!(std::fs::read_to_string(&paths.vials).unwrap().contains("classic"));
        assert!(paths.desqu.is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_backup_with_an_unreadable_file_replaces_nothing() {
        let (root, paths) = setup("broken");
        let zip = make(&paths, &root.join("backups"), 1).unwrap();
        // Break the file that is in the zip by making a zip of broken settings.
        std::fs::write(&paths.workspaces, "not json at all").unwrap();
        let broken = make(&paths, &root.join("backups"), 2).unwrap();
        let before = std::fs::read_to_string(&paths.vials).unwrap();
        std::fs::write(&paths.vials, r#"{"version":1,"vials":[],"rules":[],"theme":"fluent"}"#).unwrap();
        let error = restore(&paths, &broken).expect_err("the broken file is refused");
        assert!(error.contains("workspaces.json"), "{error}");
        assert!(std::fs::read_to_string(&paths.vials).unwrap().contains("fluent"), "nothing was replaced (before: {before})");
        assert!(restore(&paths, &zip).is_ok());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn backups_are_listed_newest_first_pruned_and_due_by_age() {
        let root = std::env::temp_dir().join(format!("desqu-backup-list-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for stamp in [100u64, 300, 200] {
            std::fs::write(root.join(format!("desqu-settings-{stamp}.zip")), "x").unwrap();
        }
        std::fs::write(root.join("other.zip"), "x").unwrap();
        let names = |v: Vec<PathBuf>| v.iter().map(|p| stamp_of(p).unwrap()).collect::<Vec<_>>();
        assert_eq!(names(list(&root)), vec![300, 200, 100]);
        assert!(!is_due(&root, 350, 86_400) && is_due(&root, 300 + 86_400, 86_400));
        prune(&root, 2);
        assert_eq!(names(list(&root)), vec![300, 200]);
        assert!(is_due(&root.join("nothing"), 5, 10), "no backup yet means it is due");
        let _ = std::fs::remove_dir_all(&root);
    }
}
