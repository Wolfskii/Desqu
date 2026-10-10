//! On-disk record of windows ScreenBound is currently modifying, so a crash or kill does not
//! leave applications stuck in a zone: the next start restores them.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::config::write_json_atomic;
use crate::core::window::{ProcessRef, WindowId, WindowState};

/// What kind of change the entry records. Journals written before borderless windows existed have
/// no `kind` and are fullscreen entries.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JournalKind {
    #[default]
    Fullscreen,
    Borderless,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntry {
    #[serde(default)]
    pub kind: JournalKind,
    pub window: WindowId,
    pub process: ProcessRef,
    pub process_name: String,
    /// The application's own fullscreen state, i.e. what to hand back.
    pub detected: WindowState,
    /// What ScreenBound set; only restore if the window still looks like this.
    pub applied: WindowState,
}

pub fn load(path: &Path) -> Vec<JournalEntry> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            tracing::warn!("ignoring unreadable journal {}: {e}", path.display());
            Vec::new()
        }),
        Err(_) => Vec::new(),
    }
}

pub fn save(path: &Path, entries: &[JournalEntry]) {
    if entries.is_empty() {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!("could not clear journal {}: {e}", path.display()),
        }
        return;
    }
    if let Err(e) = write_json_atomic(path, &entries) {
        tracing::error!("could not write journal {}: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::fullscreen::test_state;
    use crate::core::geometry::Rect;

    #[test]
    fn roundtrip_and_clear() {
        let dir = std::env::temp_dir().join(format!("screenbound-journal-{}", std::process::id()));
        let path = dir.join("managed.json");
        let entry = JournalEntry {
            kind: JournalKind::Fullscreen,
            window: WindowId(0x1234),
            process: ProcessRef {
                pid: 42,
                start_time: 7,
            },
            process_name: "chrome.exe".into(),
            detected: test_state(Rect::new(0, 0, 5120, 1440)),
            applied: test_state(Rect::new(0, 0, 3840, 1440)),
        };
        save(&path, std::slice::from_ref(&entry));
        assert_eq!(load(&path), vec![entry]);
        save(&path, &[]);
        assert!(!path.exists());
        assert!(load(&path).is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn old_entries_without_a_kind_are_fullscreen_and_borderless_round_trips() {
        let entry = JournalEntry {
            kind: JournalKind::Borderless,
            window: WindowId(7),
            process: ProcessRef { pid: 1, start_time: 2 },
            process_name: "game.exe".into(),
            detected: test_state(Rect::new(100, 100, 1380, 820)),
            applied: test_state(Rect::new(0, 0, 2560, 1440)),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert_eq!(serde_json::from_str::<JournalEntry>(&json).unwrap(), entry);

        let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
        value.as_object_mut().unwrap().remove("kind");
        let old: JournalEntry = serde_json::from_value(value).unwrap();
        assert_eq!(old.kind, JournalKind::Fullscreen);
    }
}
