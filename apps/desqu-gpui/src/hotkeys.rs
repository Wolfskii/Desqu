//! Every global hotkey the apps use, in one list, with a warning when two of them are the same key or when
//! Windows (or a common program) already uses the key. Pure: it works on the settings of the apps.

use desqu_native_shell::hotkey::{parse, Combo};
use screenbound_module::core::config::AppConfig;
use vials_core::model::VialsConfig;
use workspaces_core::model::WorkspacesConfig;

/// One hotkey: which app, what it does and the keys as the setting has them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub app: &'static str,
    pub what: &'static str,
    pub keys: String,
}

/// What is wrong with a hotkey, if anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// The text is not a key combination.
    NotAKey,
    /// Another hotkey of ours is the same.
    Twice(String),
    /// Windows or a common program uses it.
    Taken(&'static str),
}

const ALT: u32 = 1;
const CTRL: u32 = 2;
const SHIFT: u32 = 4;
const WIN: u32 = 8;

/// Combinations Windows (or a very common program) uses, and by what.
const TAKEN: &[(u32, u32, &str)] = &[
    (WIN, 0x20, "Windows: switch keyboard layout (Win+Space)"),
    (WIN | SHIFT, b'S' as u32, "Windows: screenshot (Win+Shift+S)"),
    (WIN, b'L' as u32, "Windows: lock the PC (Win+L)"),
    (WIN, b'D' as u32, "Windows: show the desktop (Win+D)"),
    (WIN, b'E' as u32, "Windows: Explorer (Win+E)"),
    (WIN, b'V' as u32, "Windows: clipboard history (Win+V)"),
    (WIN, b'Z' as u32, "Windows: snap layouts (Win+Z)"),
    (WIN | CTRL, 0x25, "Windows: previous virtual desktop (Win+Ctrl+Left)"),
    (WIN | CTRL, 0x27, "Windows: next virtual desktop (Win+Ctrl+Right)"),
    (WIN | CTRL, 0x44, "Windows: new virtual desktop (Win+Ctrl+D)"),
    (WIN | CTRL, 0x73, "Windows: close the virtual desktop (Win+Ctrl+F4)"),
    (CTRL | ALT, 0x2E, "Windows: security screen (Ctrl+Alt+Del)"),
    (ALT, 0x09, "Windows: switch windows (Alt+Tab)"),
    (ALT, 0x73, "Windows: close the window (Alt+F4)"),
    (CTRL | SHIFT, 0x1B, "Windows: Task Manager (Ctrl+Shift+Esc)"),
    (ALT, 0x20, "Windows: window menu (Alt+Space)"),
];

/// The hotkeys the apps have now.
pub fn collect(palette: &str, vials: &VialsConfig, workspaces: &WorkspacesConfig, screenbound: &AppConfig) -> Vec<Entry> {
    let mut out = Vec::new();
    if !palette.is_empty() {
        out.push(Entry { app: "Desqu", what: "Open the command palette", keys: palette.to_string() });
    }
    if !vials.peek_hotkey.is_empty() {
        out.push(Entry { app: "Vials", what: "Bring the vials above the windows (peek)", keys: vials.peek_hotkey.clone() });
    }
    if workspaces.jump_hotkeys {
        out.push(Entry { app: "Workspaces", what: "Go to virtual desktop 1 to 9", keys: "win+ctrl+1".into() });
    }
    if !screenbound.zone_hotkey.is_empty() {
        out.push(Entry { app: "Screenbound", what: "Put the window in front into the zone", keys: screenbound.zone_hotkey.clone() });
    }
    out
}

/// Every combination an entry takes: one, or nine for the desktop jump.
fn combos(entry: &Entry) -> Vec<Combo> {
    let Some(first) = parse(&entry.keys) else { return Vec::new() };
    if entry.app == "Workspaces" {
        return (b'1'..=b'9').map(|d| Combo { modifiers: first.modifiers, key: u32::from(d) }).collect();
    }
    vec![first]
}

/// The problem with each entry (in the order of `entries`), `None` where it is fine.
pub fn check(entries: &[Entry]) -> Vec<Option<Problem>> {
    entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let mine = combos(entry);
            if mine.is_empty() {
                return Some(Problem::NotAKey);
            }
            let twin = entries.iter().enumerate().find(|(j, other)| *j != i && combos(other).iter().any(|c| mine.contains(c)));
            if let Some((_, other)) = twin {
                return Some(Problem::Twice(format!("{}: {}", other.app, other.what)));
            }
            mine.iter().find_map(|c| TAKEN.iter().find(|(m, k, _)| *m == c.modifiers && *k == c.key).map(|t| Problem::Taken(t.2)))
        })
        .collect()
}

/// The keys written for people: `win+shift+space` as `Win+Shift+Space`.
pub fn pretty(keys: &str) -> String {
    keys.split('+')
        .map(|p| {
            let p = p.trim();
            let mut c = p.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(app: &'static str, keys: &str) -> Entry {
        Entry { app, what: "x", keys: keys.into() }
    }

    #[test]
    fn only_the_hotkeys_that_are_on_are_listed() {
        let mut v = VialsConfig::default();
        let mut w = WorkspacesConfig::default();
        let mut s = AppConfig::default();
        assert!(collect("", &v, &w, &s).is_empty());
        assert_eq!(collect("ctrl+alt+space", &v, &w, &s).len(), 1);
        v.peek_hotkey = "win+shift+space".into();
        w.jump_hotkeys = true;
        s.zone_hotkey = "ctrl+alt+z".into();
        assert_eq!(collect("", &v, &w, &s).iter().map(|e| e.app).collect::<Vec<_>>(), vec!["Vials", "Workspaces", "Screenbound"]);
    }

    #[test]
    fn two_apps_on_one_key_are_each_told_about_the_other() {
        let list = [entry("Vials", "ctrl+alt+z"), entry("Screenbound", "Ctrl + Alt + Z"), entry("Workspaces", "win+ctrl+1")];
        let problems = check(&list);
        assert!(matches!(&problems[0], Some(Problem::Twice(who)) if who.starts_with("Screenbound")));
        assert!(matches!(&problems[1], Some(Problem::Twice(who)) if who.starts_with("Vials")));
        assert_eq!(problems[2], None);
    }

    #[test]
    fn the_nine_desktop_keys_clash_with_a_key_of_another_app_among_them() {
        let list = [entry("Workspaces", "win+ctrl+1"), entry("Vials", "win+ctrl+5")];
        let problems = check(&list);
        assert!(matches!(&problems[0], Some(Problem::Twice(_))) && matches!(&problems[1], Some(Problem::Twice(_))));
    }

    #[test]
    fn windows_own_keys_are_flagged_and_text_that_is_not_a_key_too() {
        let problems = check(&[entry("Vials", "win+space"), entry("Screenbound", "ctrl+alt+z"), entry("Vials", "banana")]);
        assert!(matches!(problems[0], Some(Problem::Taken(why)) if why.contains("keyboard layout")));
        assert_eq!(problems[1], None);
        assert_eq!(problems[2], Some(Problem::NotAKey));
    }

    #[test]
    fn keys_are_written_for_people() {
        assert_eq!(pretty("win+shift+space"), "Win+Shift+Space");
        assert_eq!(pretty("ctrl+alt+z"), "Ctrl+Alt+Z");
    }
}
