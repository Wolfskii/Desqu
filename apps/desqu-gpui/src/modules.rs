//! Which modules Desqu runs, and what each one is. Pure data and the small rules around it, tested.

use desqu_native_shell::Identity;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Vials,
    Workspaces,
    Screenbound,
}

pub const ALL: [Kind; 3] = [Kind::Vials, Kind::Workspaces, Kind::Screenbound];

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Vials => "Vials",
            Kind::Workspaces => "Workspaces",
            Kind::Screenbound => "Screenbound",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Kind::Vials => "Desktop containers for shortcuts, apps and folders",
            Kind::Workspaces => "Desktop icons that belong to a virtual desktop",
            Kind::Screenbound => "Keeps fullscreen apps inside a zone of your monitor",
        }
    }

    /// The module's own app, which has to sit next to Desqu.
    pub fn exe(self) -> &'static str {
        match self {
            Kind::Vials => "vials-gpui.exe",
            Kind::Workspaces => "workspaces-gpui.exe",
            Kind::Screenbound => "screenbound-gpui.exe",
        }
    }

    /// How the module's app names itself to Windows: Desqu talks to it through this.
    pub fn identity(self) -> Identity {
        match self {
            Kind::Vials => Identity { name: "Vials", key: "VialsGpui" },
            Kind::Workspaces => Identity { name: "Workspaces", key: "WorkspacesGpui" },
            Kind::Screenbound => Identity { name: "Screenbound", key: "ScreenboundGpui" },
        }
    }

    /// What it is started with: no tray icon of its own (Desqu has the icon), and no window (Desqu opens it).
    pub fn start_args(self) -> Vec<&'static str> {
        let mut args = vec!["--no-tray-icon", "--autostart"];
        if self == Kind::Vials {
            args.push("--live");
        }
        args
    }

    /// The action that brings the module's own window up.
    pub fn open_action(self) -> &'static str {
        match self {
            Kind::Vials => "settings",
            _ => "show",
        }
    }
}

/// Saved in the user's config folder: which modules are on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub vials: bool,
    #[serde(default)]
    pub workspaces: bool,
    #[serde(default)]
    pub screenbound: bool,
}

impl Settings {
    pub fn enabled(&self, kind: Kind) -> bool {
        match kind {
            Kind::Vials => self.vials,
            Kind::Workspaces => self.workspaces,
            Kind::Screenbound => self.screenbound,
        }
    }

    pub fn set(&mut self, kind: Kind, on: bool) {
        match kind {
            Kind::Vials => self.vials = on,
            Kind::Workspaces => self.workspaces = on,
            Kind::Screenbound => self.screenbound = on,
        }
    }

    pub fn load(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_module_has_its_own_names_and_a_no_tray_start() {
        let keys: Vec<_> = ALL.iter().map(|k| k.identity().key).collect();
        assert_eq!(keys.len(), keys.iter().collect::<std::collections::HashSet<_>>().len(), "no two modules share a key");
        for kind in ALL {
            assert!(kind.start_args().contains(&"--no-tray-icon"), "Desqu owns the icon");
            assert!(kind.exe().ends_with("-gpui.exe"));
        }
        assert!(Kind::Vials.start_args().contains(&"--live"), "Vials only works with the desktop");
        assert!(!Kind::Workspaces.start_args().contains(&"--live"));
        assert_eq!(Kind::Vials.open_action(), "settings");
        assert_eq!(Kind::Screenbound.open_action(), "show");
    }

    #[test]
    fn settings_round_trip_and_default_to_everything_off() {
        let dir = std::env::temp_dir().join(format!("desqu-modules-test-{}", std::process::id()));
        let path = dir.join("shell.json");
        assert_eq!(Settings::load(&path), Settings::default(), "a missing file means everything off");
        let mut s = Settings::default();
        s.set(Kind::Workspaces, true);
        s.save(&path).unwrap();
        let back = Settings::load(&path);
        assert!(back.enabled(Kind::Workspaces) && !back.enabled(Kind::Vials) && !back.enabled(Kind::Screenbound));
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default(), "a broken file is ignored, not a crash");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
