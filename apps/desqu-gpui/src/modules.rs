//! Which modules Desqu runs, and what each one is. Pure data and the small rules around it, tested.

use desqu_native_shell::Identity;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Vials,
    Workspaces,
    Screenbound,
}

/// In the order the window lists them.
pub const ALL: [Kind; 3] = [Kind::Screenbound, Kind::Vials, Kind::Workspaces];

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
            Kind::Vials => "Fences-style containers on your desktop: drag a box or right-click the desktop to create one, drag icons in and out, optional auto-sort rules.",
            Kind::Workspaces => "Tie desktop shortcuts, files and folders to a virtual desktop so each desktop shows only its own.",
            Kind::Screenbound => "Keep fullscreen windows inside a zone of your monitor instead of covering all of it.",
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

}

/// Saved in the user's config folder: which modules are on, and which of those start at sign-in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub vials: bool,
    #[serde(default)]
    pub workspaces: bool,
    #[serde(default)]
    pub screenbound: bool,
    #[serde(default)]
    pub vials_start_with_system: bool,
    #[serde(default)]
    pub workspaces_start_with_system: bool,
    #[serde(default)]
    pub screenbound_start_with_system: bool,
}

/// The "start all apps with system" switch: over the apps that are on, some may start and some not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriState {
    On,
    Off,
    Mixed,
}

impl Settings {
    pub fn enabled(&self, kind: Kind) -> bool {
        match kind {
            Kind::Vials => self.vials,
            Kind::Workspaces => self.workspaces,
            Kind::Screenbound => self.screenbound,
        }
    }

    /// Whether the module starts at sign-in. Only a module that is on can.
    pub fn start_with_system(&self, kind: Kind) -> bool {
        self.enabled(kind)
            && match kind {
                Kind::Vials => self.vials_start_with_system,
                Kind::Workspaces => self.workspaces_start_with_system,
                Kind::Screenbound => self.screenbound_start_with_system,
            }
    }

    fn start_flag(&mut self, kind: Kind) -> &mut bool {
        match kind {
            Kind::Vials => &mut self.vials_start_with_system,
            Kind::Workspaces => &mut self.workspaces_start_with_system,
            Kind::Screenbound => &mut self.screenbound_start_with_system,
        }
    }

    /// Turns a module on or off. Turning it off also clears its "start with system".
    pub fn set(&mut self, kind: Kind, on: bool) {
        match kind {
            Kind::Vials => self.vials = on,
            Kind::Workspaces => self.workspaces = on,
            Kind::Screenbound => self.screenbound = on,
        }
        *self.start_flag(kind) &= on;
    }

    /// Sets "start with system" for one module. Refused (false) for a module that is off.
    pub fn set_start_with_system(&mut self, kind: Kind, value: bool) -> bool {
        if value && !self.enabled(kind) {
            return false;
        }
        *self.start_flag(kind) = value;
        true
    }

    /// Sets "start with system" for every module that is on (turning it off applies to all).
    pub fn set_all_start_with_system(&mut self, value: bool) {
        for kind in ALL {
            let on = self.enabled(kind);
            *self.start_flag(kind) = value && on;
        }
    }

    pub fn all_start(&self) -> TriState {
        let on: Vec<Kind> = ALL.into_iter().filter(|k| self.enabled(*k)).collect();
        let starting = on.iter().filter(|k| self.start_with_system(**k)).count();
        if on.is_empty() || starting == 0 {
            TriState::Off
        } else if starting == on.len() {
            TriState::On
        } else {
            TriState::Mixed
        }
    }

    /// Desqu itself starts with Windows only when at least one module wants to.
    pub fn should_autostart(&self) -> bool {
        ALL.into_iter().any(|k| self.start_with_system(k))
    }

    /// Which modules start when Desqu launches: from sign-in only those marked "start with system";
    /// a launch by hand starts everything that is on.
    pub fn to_start(&self, autostart_launch: bool) -> Vec<Kind> {
        ALL.into_iter().filter(|k| self.enabled(*k) && (!autostart_launch || self.start_with_system(*k))).collect()
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
    }

    #[test]
    fn start_with_system_follows_the_rules_of_the_old_desqu() {
        let mut s = Settings::default();
        assert!(!s.set_start_with_system(Kind::Vials, true), "an app that is off cannot start with Windows");
        assert!(!s.should_autostart() && s.all_start() == TriState::Off);

        s.set(Kind::Vials, true);
        s.set(Kind::Screenbound, true);
        assert!(s.set_start_with_system(Kind::Vials, true));
        assert_eq!(s.all_start(), TriState::Mixed);
        assert!(s.should_autostart());
        assert_eq!(s.to_start(true), vec![Kind::Vials], "from sign-in: only the ones marked");
        assert_eq!(s.to_start(false), vec![Kind::Screenbound, Kind::Vials], "by hand: everything that is on");

        s.set_all_start_with_system(true);
        assert_eq!(s.all_start(), TriState::On);
        assert!(!s.start_with_system(Kind::Workspaces), "the master switch skips apps that are off");

        s.set(Kind::Vials, false);
        s.set(Kind::Vials, true);
        assert!(!s.start_with_system(Kind::Vials), "turning an app off clears its start with system");

        s.set_all_start_with_system(false);
        assert!(!s.should_autostart() && s.all_start() == TriState::Off);
    }

    #[test]
    fn an_older_settings_file_still_loads() {
        let old: Settings = serde_json::from_str(r#"{"vials":true,"workspaces":false,"screenbound":true}"#).unwrap();
        assert!(old.enabled(Kind::Vials) && old.enabled(Kind::Screenbound) && !old.should_autostart());
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
