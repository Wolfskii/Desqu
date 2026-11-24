//! Window layouts: where every window was, put back after the screens change (docking, undocking, a monitor
//! that woke up again). The positions are kept for each screen setup; when the setup changes, the windows go to
//! where they were the last time the screens were like this. What matches which window is pure and tested; the
//! windows themselves are read and moved through Screenbound's platform layer.

use std::collections::HashMap;
use std::path::PathBuf;

use screenbound_module::core::geometry::Rect;
use screenbound_module::core::monitor::MonitorInfo;
use screenbound_module::core::window::{ShowState, WindowId};
use screenbound_module::platform::{self, PlatformWindowManager};
use serde::{Deserialize, Serialize};

/// A window as it was.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    pub process: String,
    pub class: String,
    pub title: String,
    pub rect: [i32; 4],
}

/// A window as it is now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Live {
    pub id: u64,
    pub process: String,
    pub class: String,
    pub title: String,
    pub rect: [i32; 4],
}

/// Windows that are closer than this (px, on every edge) to where they were are left alone.
const CLOSE: i32 = 8;

/// Which window goes where. A window of the same program and kind is matched by title first (the same document
/// or page), then in the order the two lists have; each window is used once. Windows already in place are skipped.
pub fn plan_restore(saved: &[Saved], now: &[Live]) -> Vec<(u64, [i32; 4])> {
    let mut used = vec![false; now.len()];
    let mut moves = Vec::new();
    let take = |saved: &Saved, by_title: bool, used: &mut Vec<bool>| -> Option<usize> {
        now.iter().enumerate().position(|(i, w)| !used[i] && w.process == saved.process && w.class == saved.class && (!by_title || w.title == saved.title)).inspect(|i| used[*i] = true)
    };
    // Matches by title first, so the windows of one program that differ only by title find their own place.
    let mut leftover: Vec<&Saved> = Vec::new();
    for s in saved {
        match take(s, true, &mut used) {
            Some(i) => push_move(&mut moves, &now[i], s),
            None => leftover.push(s),
        }
    }
    for s in leftover {
        if let Some(i) = take(s, false, &mut used) {
            push_move(&mut moves, &now[i], s);
        }
    }
    moves
}

fn push_move(moves: &mut Vec<(u64, [i32; 4])>, window: &Live, saved: &Saved) {
    if window.rect.iter().zip(saved.rect).any(|(a, b)| (a - b).abs() > CLOSE) {
        moves.push((window.id, saved.rect));
    }
}

/// Everything that is kept: the windows for each screen setup.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Store {
    pub setups: HashMap<String, Vec<Saved>>,
}

pub fn store_path() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default().join("io.github.wolfskii.desqu").join("window-layouts.json")
}

impl Store {
    pub fn load() -> Store {
        std::fs::read_to_string(store_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(dir) = store_path().parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = store_path().with_extension("json.tmp");
        if let Ok(text) = serde_json::to_string(self) {
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(&tmp, store_path());
            }
        }
    }
}

/// A text for the screens as they are: each monitor's name, size and place.
pub fn setup_key(monitors: &[MonitorInfo]) -> String {
    let mut parts: Vec<String> = monitors
        .iter()
        .map(|m| format!("{}:{}x{}@{},{}", m.device_name, m.bounds.right - m.bounds.left, m.bounds.bottom - m.bounds.top, m.bounds.left, m.bounds.top))
        .collect();
    parts.sort();
    parts.join("|")
}

/// The windows now, read through the platform layer: only ordinary visible ones that are neither minimized nor
/// maximized (those follow their monitor by themselves), and none of Desqu's own.
pub fn windows_now(manager: &dyn PlatformWindowManager) -> Vec<Live> {
    let own = std::process::id();
    manager
        .enumerate_windows()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|id: WindowId| {
            let identity = manager.identity(id).ok()?;
            let state = manager.state(id).ok()?;
            if state.show_state != ShowState::Normal || !state.visible || state.cloaked || identity.process.pid == own || identity.process_name.is_empty() {
                return None;
            }
            let b = state.bounds;
            Some(Live { id: id.0, process: identity.process_name, class: identity.class_name, title: identity.title, rect: [b.left, b.top, b.right, b.bottom] })
        })
        .collect()
}

/// The running tool: it keeps the windows of the screen setup as they are, and puts them back when the setup returns.
pub struct WindowLayouts {
    manager: std::sync::Arc<dyn PlatformWindowManager>,
    store: Store,
    /// The setup the screens are in, as of the last look.
    screens: String,
    /// When the setup changed (the screens need a moment to settle before windows are put on them).
    changed_at: Option<std::time::Instant>,
    last_saved: Option<std::time::Instant>,
}

/// How often the windows of the current setup are written down.
const SAVE_EVERY: std::time::Duration = std::time::Duration::from_secs(20);
/// How long the screens are given to settle after a change.
const SETTLE: std::time::Duration = std::time::Duration::from_secs(3);

impl WindowLayouts {
    pub fn new() -> Self {
        platform::init_thread();
        Self { manager: platform::create(), store: Store::load(), screens: String::new(), changed_at: None, last_saved: None }
    }

    fn key(&self) -> String {
        setup_key(&self.manager.monitors().unwrap_or_default())
    }

    /// Writes down where the windows are now, for the setup the screens are in.
    pub fn save_now(&mut self) {
        let key = self.key();
        if key.is_empty() {
            return;
        }
        let saved: Vec<Saved> = windows_now(self.manager.as_ref()).into_iter().map(|w| Saved { process: w.process, class: w.class, title: w.title, rect: w.rect }).collect();
        self.store.setups.insert(key, saved);
        self.store.save();
        self.last_saved = Some(std::time::Instant::now());
    }

    /// Puts the windows where the current setup had them. Returns how many moved.
    pub fn restore_now(&self) -> usize {
        let Some(saved) = self.store.setups.get(&self.key()) else { return 0 };
        let moves = plan_restore(saved, &windows_now(self.manager.as_ref()));
        let mut moved = 0;
        for (id, r) in moves {
            if self.manager.set_bounds(WindowId(id), Rect::new(r[0], r[1], r[2], r[3])).is_ok() {
                moved += 1;
            }
        }
        moved
    }

    /// Called about every second while the tool is on.
    pub fn tick(&mut self) {
        let key = self.key();
        if key.is_empty() {
            return;
        }
        if self.screens.is_empty() {
            self.screens = key;
            return;
        }
        if key != self.screens {
            self.screens = key;
            self.changed_at = Some(std::time::Instant::now());
            return;
        }
        if let Some(at) = self.changed_at {
            if at.elapsed() >= SETTLE {
                self.changed_at = None;
                self.restore_now();
                self.last_saved = Some(std::time::Instant::now());
            }
            return;
        }
        if self.last_saved.is_none_or(|t| t.elapsed() >= SAVE_EVERY) {
            self.save_now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn was(process: &str, title: &str, rect: [i32; 4]) -> Saved {
        Saved { process: process.into(), class: "C".into(), title: title.into(), rect }
    }

    fn is(id: u64, process: &str, title: &str, rect: [i32; 4]) -> Live {
        Live { id, process: process.into(), class: "C".into(), title: title.into(), rect }
    }

    #[test]
    fn windows_go_back_to_where_they_were_and_the_ones_in_place_are_left_alone() {
        let saved = [was("code.exe", "a.rs", [0, 0, 800, 600]), was("chrome.exe", "Mail", [800, 0, 1600, 900])];
        let now = [is(1, "chrome.exe", "Mail", [10, 10, 500, 400]), is(2, "code.exe", "a.rs", [0, 3, 800, 598])];
        assert_eq!(plan_restore(&saved, &now), vec![(1, [800, 0, 1600, 900])], "the code window is within a few pixels already");
    }

    #[test]
    fn windows_of_one_program_find_their_own_place_by_title_and_each_is_used_once() {
        let saved = [was("chrome.exe", "Mail", [0, 0, 500, 500]), was("chrome.exe", "Docs", [600, 0, 1100, 500])];
        let now = [is(1, "chrome.exe", "Docs", [0, 0, 100, 100]), is(2, "chrome.exe", "Mail", [0, 0, 100, 100])];
        assert_eq!(plan_restore(&saved, &now), vec![(2, [0, 0, 500, 500]), (1, [600, 0, 1100, 500])]);
        // A third saved window with nothing left to match moves nothing.
        let more = [was("chrome.exe", "Mail", [0, 0, 500, 500]), was("chrome.exe", "Docs", [600, 0, 1100, 500]), was("chrome.exe", "Third", [0, 600, 500, 900])];
        assert_eq!(plan_restore(&more, &now).len(), 2);
    }

    #[test]
    fn a_window_whose_title_changed_is_matched_by_program_and_kind_in_order() {
        let saved = [was("notepad.exe", "old title", [0, 0, 300, 300])];
        let now = [is(7, "notepad.exe", "new title", [500, 500, 800, 800]), is(8, "other.exe", "x", [0, 0, 1, 1])];
        assert_eq!(plan_restore(&saved, &now), vec![(7, [0, 0, 300, 300])]);
        assert!(plan_restore(&[], &now).is_empty());
    }

    #[test]
    fn the_store_round_trips_as_text() {
        let mut store = Store::default();
        store.setups.insert("desk".into(), vec![was("a.exe", "t", [1, 2, 3, 4])]);
        let text = serde_json::to_string(&store).unwrap();
        let back: Store = serde_json::from_str(&text).unwrap();
        assert_eq!(back.setups["desk"][0].rect, [1, 2, 3, 4]);
    }

    /// Reads the real windows and screens (changes nothing): run by hand with `--ignored`.
    #[test]
    #[ignore = "reads the real windows"]
    fn the_real_windows_can_be_read() {
        platform::init_thread();
        let manager = platform::create();
        let windows = windows_now(manager.as_ref());
        let key = setup_key(&manager.monitors().unwrap());
        println!("{} windows, setup {key}", windows.len());
        assert!(!windows.is_empty() && !key.is_empty());
    }
}
