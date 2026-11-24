//! The options of the apps that Desqu shows on their own pages, so they are set without opening the
//! app's window. A running app is told (it applies the change the same way its own window does and
//! saves it); an app that is off has its config file changed directly.

use std::path::{Path, PathBuf};

use desqu_native_shell::tray;
use vials_core::model::VialsConfig;
use workspaces_core::model::WorkspacesConfig;

use crate::modules::Kind;

fn config_file(folder: &str) -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default().join(folder).join("config.json")
}

pub fn vials_path() -> PathBuf {
    config_file("io.github.wolfskii.vials")
}

pub fn workspaces_path() -> PathBuf {
    config_file("io.github.wolfskii.workspaces")
}

/// Vials' options as saved (the defaults when it never ran).
pub fn vials() -> VialsConfig {
    VialsConfig::load(&vials_path()).unwrap_or_default()
}

pub fn workspaces_auto_assign() -> bool {
    WorkspacesConfig::load(&workspaces_path()).map(|c| c.auto_assign_new).unwrap_or(true)
}

/// Changes one option of `kind`. `running`: the app is up and takes the change itself.
pub fn apply(kind: Kind, running: bool, key: &str, value: &str) -> Result<(), String> {
    if running && tray::forward_action(&kind.identity(), &format!("set-setting:{key}:{value}")) {
        return Ok(());
    }
    match kind {
        Kind::Vials => apply_to_vials_file(&vials_path(), key, value),
        Kind::Workspaces => apply_to_workspaces_file(&workspaces_path(), key, value),
        Kind::Screenbound => Err("Screenbound has no options here".into()),
    }
}

fn apply_to_vials_file(path: &Path, key: &str, value: &str) -> Result<(), String> {
    let mut cfg = VialsConfig::load(path).map_err(|e| e.to_string())?;
    cfg.set_setting(key, value)?;
    cfg.save(path).map_err(|e| e.to_string())
}

fn apply_to_workspaces_file(path: &Path, key: &str, value: &str) -> Result<(), String> {
    if key != "autoAssignNew" {
        return Err(format!("unknown setting {key}"));
    }
    let mut cfg = WorkspacesConfig::load(path).map_err(|e| e.to_string())?;
    cfg.auto_assign_new = value.parse::<bool>().map_err(|_| format!("{key}: expected true or false"))?;
    cfg.save(path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("desqu-app-settings-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("config.json")
    }

    #[test]
    fn a_vials_option_is_written_to_its_file_when_it_is_off() {
        let path = temp("vials");
        apply_to_vials_file(&path, "theme", "classic").unwrap();
        apply_to_vials_file(&path, "glassTint", "40").unwrap();
        let cfg = VialsConfig::load(&path).unwrap();
        assert_eq!(cfg.glass_tint, 40);
        assert!(apply_to_vials_file(&path, "glassTint", "400").is_err());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn the_workspaces_option_is_written_to_its_file_when_it_is_off() {
        let path = temp("workspaces");
        apply_to_workspaces_file(&path, "autoAssignNew", "false").unwrap();
        assert!(!WorkspacesConfig::load(&path).unwrap().auto_assign_new);
        assert!(apply_to_workspaces_file(&path, "other", "true").is_err());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
