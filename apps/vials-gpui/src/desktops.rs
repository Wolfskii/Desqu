//! Vials on virtual desktops. When the Workspaces app runs, it can tie a vial to a virtual desktop
//! (it keeps that in its own config, under `vial:<id>`); Vials reads that and puts the vial's window
//! on that desktop, so the vial only shows there. Without Workspaces running nothing is moved.
//!
//! Windows lets a program move its own windows between virtual desktops (`IVirtualDesktopManager`,
//! documented); no undocumented interface is used.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

use desqu_native_shell::{tray, Identity};
use windows::core::GUID;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
use windows::Win32::UI::Shell::{IVirtualDesktopManager, VirtualDesktopManager};
use workspaces_core::model::{desktop_label, vial_item, vial_of, WorkspacesConfig};

/// How the Workspaces app names itself to Windows.
const WORKSPACES: Identity = Identity { name: "Workspaces", key: "WorkspacesGpui" };

/// Where Workspaces keeps its config (`--workspaces-config <file>` overrides, for tests).
pub fn workspaces_config_path() -> PathBuf {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--workspaces-config" {
            if let Some(p) = args.next() {
                return PathBuf::from(p);
            }
        }
    }
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default();
    appdata.join("io.github.wolfskii.workspaces").join("config.json")
}

/// Whether the Workspaces app is running (its single-instance mutex is taken).
pub fn workspaces_running() -> bool {
    tray::single_instance(&WORKSPACES).is_none()
}

/// What a vial's right-click menu offers while Workspaces runs: (label, checked, target), where the
/// target is a virtual desktop id or `all`. `None` when Workspaces is not running or Windows has only
/// one virtual desktop.
pub fn menu_choices(vial_id: &str) -> Option<Vec<(String, bool, String)>> {
    if !workspaces_running() {
        return None;
    }
    let list = workspaces_platform::desktops().ok()?.list;
    if list.is_empty() {
        return None;
    }
    let cfg = WorkspacesConfig::load(&workspaces_config_path()).unwrap_or_default();
    let owner = cfg.owner_of(&vial_item(vial_id)).map(str::to_string);
    let mut out = vec![("Show on every desktop".to_string(), owner.is_none(), "all".to_string())];
    for (i, d) in list.iter().enumerate() {
        out.push((format!("Show on {}", desktop_label(i, &d.name)), owner.as_deref() == Some(d.id.as_str()), d.id.clone()));
    }
    Some(out)
}

/// Asks the running Workspaces to tie the vial to a virtual desktop (or `all`). It is Workspaces that
/// keeps the setting; the vial follows within a second.
pub fn assign(vial_id: &str, target: &str) -> bool {
    tray::forward_action(&WORKSPACES, &format!("assign-vial:{vial_id}:{target}"))
}

/// `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}` as a GUID.
pub fn parse_guid(text: &str) -> Option<GUID> {
    let hex: String = text.chars().filter(|c| !matches!(c, '{' | '}' | '-')).collect();
    (hex.len() == 32).then(|| u128::from_str_radix(&hex, 16).ok()).flatten().map(GUID::from_u128)
}

#[cfg(test)]
fn guid_text(guid: &GUID) -> String {
    format!("{{{guid:?}}}").to_uppercase()
}

/// Which virtual desktop each vial is tied to (vial id to desktop id).
pub fn assignments(cfg: &WorkspacesConfig) -> HashMap<String, String> {
    cfg.workspaces
        .iter()
        .flat_map(|w| w.items.iter().filter_map(move |i| vial_of(i).map(|v| (v.to_string(), w.desktop_id.clone()))))
        .collect()
}

/// Watches the Workspaces config and says what to do with the vials' windows when it (or whether
/// Workspaces runs) changed.
pub struct Follower {
    path: PathBuf,
    seen: Option<(Option<SystemTime>, bool)>,
    /// Vials this program moved to a desktop; they are moved back to the current one when released.
    moved: HashMap<String, String>,
}

/// What a vial's window should do.
#[derive(Debug, PartialEq, Eq)]
pub enum Move {
    /// To this virtual desktop.
    To(String),
    /// To the desktop you are on (it was tied to one, and is not any more).
    ToCurrent,
}

impl Follower {
    pub fn new(path: PathBuf) -> Self {
        Self { path, seen: None, moved: HashMap::new() }
    }

    /// `None` when nothing changed since the last call; else the moves to make for the given vials.
    pub fn poll(&mut self, vial_ids: &[String]) -> Option<Vec<(String, Move)>> {
        let modified = std::fs::metadata(&self.path).and_then(|m| m.modified()).ok();
        let running = workspaces_running();
        let now = (modified, running);
        if self.seen == Some(now) {
            return None;
        }
        self.seen = Some(now);
        let wanted = if running { WorkspacesConfig::load(&self.path).map(|c| assignments(&c)).unwrap_or_default() } else { HashMap::new() };
        Some(plan(&wanted, &mut self.moved, vial_ids))
    }
}

/// The moves that bring the windows in line with `wanted`. `moved` remembers what was moved by us.
pub fn plan(wanted: &HashMap<String, String>, moved: &mut HashMap<String, String>, vial_ids: &[String]) -> Vec<(String, Move)> {
    let mut out = Vec::new();
    for id in vial_ids {
        match (wanted.get(id), moved.get(id)) {
            (Some(to), Some(was)) if to == was => {}
            (Some(to), _) => {
                moved.insert(id.clone(), to.clone());
                out.push((id.clone(), Move::To(to.clone())));
            }
            (None, Some(_)) => {
                moved.remove(id);
                out.push((id.clone(), Move::ToCurrent));
            }
            (None, None) => {}
        }
    }
    moved.retain(|id, _| vial_ids.contains(id));
    out
}

/// Windows' virtual desktop manager. Needs the COM apartment of the calling thread (GPUI's UI thread
/// has one).
pub struct Manager(IVirtualDesktopManager);

impl Manager {
    pub fn new() -> Result<Self, String> {
        // SAFETY: plain COM creation on a thread with COM initialised.
        unsafe { CoCreateInstance::<_, IVirtualDesktopManager>(&VirtualDesktopManager, None, CLSCTX_ALL) }
            .map(Self)
            .map_err(|e| format!("no virtual desktop manager: {e}"))
    }

    /// Moves one of this program's own windows to a virtual desktop.
    pub fn move_to(&self, hwnd: HWND, desktop: &str) -> Result<(), String> {
        let guid = parse_guid(desktop).ok_or_else(|| format!("'{desktop}' is not a virtual desktop id"))?;
        // SAFETY: our own window.
        unsafe { self.0.MoveWindowToDesktop(hwnd, &guid) }.map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use workspaces_core::model::vial_item;

    const A: &str = "{AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA}";
    const B: &str = "{3FEBEE14-E0E2-4DCE-A63D-09646A16F375}";

    #[test]
    fn desktop_ids_round_trip_through_a_guid() {
        let guid = parse_guid(B).expect("a valid id");
        assert_eq!(guid_text(&guid), B);
        assert!(parse_guid("nonsense").is_none());
        assert!(parse_guid("{1234}").is_none());
    }

    #[test]
    fn only_vials_are_picked_out_of_the_workspaces_config() {
        let mut cfg = WorkspacesConfig::default();
        cfg.assign(&vial_item("vial-1"), Some((A, "Work")));
        cfg.assign("C:/Users/me/Desktop/a.txt", Some((A, "Work")));
        cfg.assign(&vial_item("vial-2"), Some((B, "Play")));
        let got = assignments(&cfg);
        assert_eq!(got.len(), 2);
        assert_eq!(got["vial-1"], A);
        assert_eq!(got["vial-2"], B);
    }

    #[test]
    fn windows_are_moved_once_and_released_to_the_current_desktop() {
        let ids = vec!["vial-1".to_string(), "vial-2".to_string()];
        let mut moved = HashMap::new();
        let mut wanted = HashMap::from([("vial-1".to_string(), A.to_string())]);
        assert_eq!(plan(&wanted, &mut moved, &ids), vec![("vial-1".to_string(), Move::To(A.into()))]);
        assert!(plan(&wanted, &mut moved, &ids).is_empty(), "nothing to do the second time");
        wanted.insert("vial-1".into(), B.into());
        assert_eq!(plan(&wanted, &mut moved, &ids), vec![("vial-1".to_string(), Move::To(B.into()))]);
        wanted.clear(); // Workspaces ended, or the vial was set to every desktop
        assert_eq!(plan(&wanted, &mut moved, &ids), vec![("vial-1".to_string(), Move::ToCurrent)]);
        assert!(plan(&wanted, &mut moved, &ids).is_empty());
        assert!(moved.is_empty());
    }
}
