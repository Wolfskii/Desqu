//! Pure model of the desktop layer: items, the park-and-restore journal, and the rules around
//! them. Nothing here touches the shell, so it is fully unit tested.
//!
//! Hiding works in one of two ways. The native way keeps the desktop as it is and gives the hidden
//! items the file system's *hidden* attribute (Explorer does not draw those unless "show hidden
//! files" is on), telling the shell right away. The journal is written **before** an item is
//! touched, so a crash can never leave one hidden for good. The other way switches the desktop's
//! own icon layer off and lets the owners draw the icons themselves; the journal then records that
//! switch. (Items used to be *parked* off-screen; Windows 11 build 26300 stopped allowing that, see
//! `apps/vials/docs/desktop-research.md`. Journals written back then are still understood.)

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Where hidden items are parked. Negative and far outside any realistic virtual screen.
pub const PARK_POINT: Point = Point { x: -6000, y: -6000 };

/// A parking spot of its own for the n-th hidden item. Hundreds of icons stacked on one point can be
/// nudged apart by Explorer (and end up on screen again), so each gets a distinct off-screen cell.
pub fn park_slot(n: usize) -> Point {
    Point { x: PARK_POINT.x - (n % 250) as i32 * 100, y: PARK_POINT.y - (n / 250) as i32 * 100 }
}

/// Fallback original position for an item found parked with no journal entry (e.g. a lost
/// journal): the top-left cell, where the shell then snaps it to a free grid slot.
pub const FALLBACK_ORIGIN: Point = Point { x: 16, y: 2 };

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// True for any position that can only be a parked one.
pub fn is_parked(p: Point) -> bool {
    p.x <= -2000 && p.y <= -2000
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
    Shortcut,
    Folder,
    App,
    File,
    /// Virtual shell items such as This PC and the Recycle Bin.
    Special,
}

/// Classifies an item by its parsing name. `is_dir` comes from the caller (it needs the file
/// system), so this stays pure.
pub fn classify(id: &str, is_dir: bool) -> ItemKind {
    if id.starts_with("::") {
        return ItemKind::Special;
    }
    if is_dir {
        return ItemKind::Folder;
    }
    let ext = Path::new(id)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "lnk" | "url" | "appref-ms" => ItemKind::Shortcut,
        "exe" | "msi" | "bat" | "cmd" => ItemKind::App,
        _ => ItemKind::File,
    }
}

/// One thing shown on the desktop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopItem {
    /// Absolute parsing name: a file path, or `::{CLSID}` for a virtual item. Stable and unique.
    pub id: String,
    /// What the desktop labels it (extensions of known types are hidden).
    pub name: String,
    pub kind: ItemKind,
    pub position: Point,
    /// Hidden: parked off-screen (by us, or found that way), or, while an owner draws the desktop
    /// itself, claimed as hidden by some owner. A surface must not draw these.
    pub parked: bool,
}

/// An item we parked, with who asked for it. It is shown again only when *no* owner still wants
/// it hidden, so Vials and Workspaces can both hide the same item without fighting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParkedItem {
    pub id: String,
    /// Where the desktop showed it when it was hidden. Parked items are put back here.
    pub original: Point,
    pub owners: Vec<String>,
    /// Hidden with the hidden attribute (the current way). `false`: parked off-screen by an older
    /// build, which is put back by position.
    #[serde(default)]
    pub by_attribute: bool,
    /// The file was marked hidden before we touched it, so showing it must leave the attribute alone.
    #[serde(default)]
    pub attr_was_set: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Journal {
    #[serde(default)]
    pub items: Vec<ParkedItem>,
    /// This process switched off the desktop's own icons ("Show desktop icons") and must switch
    /// them back on, also after a crash. Not set when the user had them off already.
    #[serde(default)]
    pub icons_hidden: bool,
}

impl Journal {
    /// A missing or unreadable journal is an empty one (and unreadable ones are logged).
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                tracing::warn!("ignoring unreadable desktop journal {}: {e}", path.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// Writes atomically (temp file + rename). An empty journal removes the file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if self.items.is_empty() && !self.icons_hidden {
            return match std::fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e),
            };
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?)?;
        std::fs::rename(&tmp, path)
    }

    pub fn contains(&self, id: &str) -> bool {
        self.items.iter().any(|i| i.id == id)
    }

    /// Records that `owner` wants `id` hidden. `original` is where it is now. Returns `true` when
    /// the item must be physically parked (nobody had hidden it yet).
    pub fn park(&mut self, owner: &str, id: &str, original: Point) -> bool {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            if !item.owners.iter().any(|o| o == owner) {
                item.owners.push(owner.to_string());
            }
            return false;
        }
        let original = if is_parked(original) { FALLBACK_ORIGIN } else { original };
        self.items.push(ParkedItem {
            id: id.to_string(),
            original,
            owners: vec![owner.to_string()],
            by_attribute: false,
            attr_was_set: false,
        });
        true
    }

    /// Records that `owner` wants the file `id` hidden with the hidden attribute. `original` is where
    /// the desktop shows it, `attr_was_set` whether it already was hidden. Returns `true` when the
    /// attribute must be set (nobody had hidden it yet).
    pub fn claim_attr(&mut self, owner: &str, id: &str, original: Point, attr_was_set: bool) -> bool {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            if !item.owners.iter().any(|o| o == owner) {
                item.owners.push(owner.to_string());
            }
            return false;
        }
        self.items.push(ParkedItem {
            id: id.to_string(),
            original,
            owners: vec![owner.to_string()],
            by_attribute: true,
            attr_was_set,
        });
        !attr_was_set
    }

    /// `owner` no longer wants `id` hidden. Returns the position to restore when that was the
    /// last owner (the item must be physically moved back), else `None`.
    pub fn unpark(&mut self, owner: &str, id: &str) -> Option<Point> {
        self.unpark_item(owner, id).map(|item| item.original)
    }

    /// Like [`Journal::unpark`], but returns the whole entry (to know how it was hidden).
    pub fn unpark_item(&mut self, owner: &str, id: &str) -> Option<ParkedItem> {
        let pos = self.items.iter().position(|i| i.id == id)?;
        self.items[pos].owners.retain(|o| o != owner);
        if self.items[pos].owners.is_empty() {
            Some(self.items.remove(pos))
        } else {
            None
        }
    }

    /// Ids `owner` currently wants hidden.
    pub fn hidden_by(&self, owner: &str) -> Vec<String> {
        self.items
            .iter()
            .filter(|i| i.owners.iter().any(|o| o == owner))
            .map(|i| i.id.clone())
            .collect()
    }

    /// Drops `owner` from every item; returns the items that now need restoring.
    pub fn release_owner(&mut self, owner: &str) -> Vec<ParkedItem> {
        let mut restore = Vec::new();
        self.items.retain_mut(|item| {
            item.owners.retain(|o| o != owner);
            if item.owners.is_empty() {
                restore.push(item.clone());
                false
            } else {
                true
            }
        });
        restore
    }
}

/// What to do with a journal found at startup (crash recovery).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryPlan {
    /// Still on the desktop: move back to `original`.
    pub restore: Vec<ParkedItem>,
    /// Gone from the desktop (deleted or renamed meanwhile): nothing to restore.
    pub forget: Vec<String>,
}

pub fn plan_recovery(journal: &Journal, present: &HashSet<String>) -> RecoveryPlan {
    let (restore, gone): (Vec<_>, Vec<_>) = journal.items.iter().cloned().partition(|i| present.contains(&i.id));
    RecoveryPlan {
        restore,
        forget: gone.into_iter().map(|i| i.id).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: i32, y: i32) -> Point {
        Point { x, y }
    }

    #[test]
    fn park_slots_are_distinct_parked_and_inside_the_coordinate_range() {
        let slots: Vec<Point> = (0..2000).map(park_slot).collect();
        let unique: std::collections::HashSet<(i32, i32)> = slots.iter().map(|p| (p.x, p.y)).collect();
        assert_eq!(unique.len(), 2000);
        assert!(slots.iter().all(|p| is_parked(*p) && p.x > i32::from(i16::MIN) && p.y > i32::from(i16::MIN)));
        assert_eq!(park_slot(0), PARK_POINT);
    }

    #[test]
    fn parked_positions_are_recognised_but_ordinary_negative_ones_are_not() {
        assert!(is_parked(PARK_POINT));
        assert!(is_parked(p(-20000, -20000)));
        // A monitor to the left of the primary one has negative coordinates but is on-screen.
        assert!(!is_parked(p(-1920, 100)));
        assert!(!is_parked(p(16, 2)));
    }

    #[test]
    fn classifies_by_parsing_name() {
        assert_eq!(classify(r"C:\Users\me\Desktop\Steam.lnk", false), ItemKind::Shortcut);
        assert_eq!(classify(r"C:\Users\me\Desktop\Game.URL", false), ItemKind::Shortcut);
        assert_eq!(classify(r"C:\Users\me\Desktop\tool.exe", false), ItemKind::App);
        assert_eq!(classify(r"C:\Users\me\Desktop\notes.txt", false), ItemKind::File);
        assert_eq!(classify(r"C:\Users\me\Desktop\Projects", true), ItemKind::Folder);
        assert_eq!(classify("::{20D04FE0-3AEA-1069-A2D8-08002B30309D}", false), ItemKind::Special);
        assert_eq!(classify(r"C:\Users\me\Desktop\noextension", false), ItemKind::File);
    }

    #[test]
    fn the_first_owner_parks_and_a_second_owner_only_shares() {
        let mut j = Journal::default();
        assert!(j.park("vials", "a", p(16, 2)), "first owner must physically park it");
        assert!(!j.park("workspaces", "a", p(-6000, -6000)), "already parked: just add an owner");
        assert_eq!(j.items.len(), 1);
        assert_eq!(j.items[0].original, p(16, 2), "the real original is kept");
        assert_eq!(j.items[0].owners, ["vials", "workspaces"]);
        // Asking twice does not duplicate the owner.
        assert!(!j.park("vials", "a", p(16, 2)));
        assert_eq!(j.items[0].owners.len(), 2);
    }

    #[test]
    fn an_item_comes_back_only_when_the_last_owner_lets_go() {
        let mut j = Journal::default();
        j.park("vials", "a", p(100, 200));
        j.park("workspaces", "a", p(100, 200));
        assert_eq!(j.unpark("vials", "a"), None, "workspaces still wants it hidden");
        assert!(j.contains("a"));
        assert_eq!(j.unpark("workspaces", "a"), Some(p(100, 200)));
        assert!(!j.contains("a"));
        assert_eq!(j.unpark("workspaces", "a"), None, "unknown item is a no-op");
    }

    #[test]
    fn an_item_found_parked_without_a_journal_entry_gets_a_sane_origin() {
        let mut j = Journal::default();
        assert!(j.park("vials", "a", PARK_POINT));
        assert_eq!(j.items[0].original, FALLBACK_ORIGIN);
    }

    #[test]
    fn hidden_by_and_release_owner() {
        let mut j = Journal::default();
        j.park("vials", "a", p(1, 1));
        j.park("vials", "b", p(2, 2));
        j.park("workspaces", "b", p(2, 2));
        assert_eq!(j.hidden_by("vials"), ["a", "b"]);
        assert_eq!(j.hidden_by("workspaces"), ["b"]);

        let restore = j.release_owner("vials");
        assert_eq!(restore.len(), 1, "only a is free again; b is still wanted by workspaces");
        assert_eq!(restore[0].id, "a");
        assert_eq!(j.hidden_by("workspaces"), ["b"]);
        assert!(j.hidden_by("vials").is_empty());
    }

    #[test]
    fn recovery_restores_what_is_still_there_and_forgets_what_is_gone() {
        let mut j = Journal::default();
        j.park("vials", "kept", p(10, 20));
        j.park("vials", "deleted", p(30, 40));
        let present: HashSet<String> = ["kept".to_string(), "other".to_string()].into();
        let plan = plan_recovery(&j, &present);
        assert_eq!(plan.restore.len(), 1);
        assert_eq!(plan.restore[0].id, "kept");
        assert_eq!(plan.forget, ["deleted"]);
    }

    #[test]
    fn hiding_by_attribute_is_shared_between_owners_and_remembers_a_preset_attribute() {
        let mut j = Journal::default();
        assert!(j.claim_attr("vials", "a", p(14, 2), false), "the first owner sets the attribute");
        assert!(!j.claim_attr("workspaces", "a", p(14, 2), false), "a second owner only shares");
        assert!(!j.claim_attr("vials", "b", p(14, 100), true), "already hidden by the user: nothing to set");
        assert!(j.items.iter().all(|i| i.by_attribute));

        assert!(j.unpark_item("vials", "a").is_none(), "still wanted by workspaces");
        let last = j.unpark_item("workspaces", "a").unwrap();
        assert!(last.by_attribute && !last.attr_was_set);
        let preset = j.unpark_item("vials", "b").unwrap();
        assert!(preset.attr_was_set, "the restore must leave a user's own hidden attribute alone");
    }

    #[test]
    fn a_journal_from_the_parking_days_still_loads_as_parked() {
        let old = r#"{"items":[{"id":"parked.lnk","original":{"x":242,"y":2},"owners":["vials"]}]}"#;
        let j: Journal = serde_json::from_str(old).unwrap();
        assert!(!j.items[0].by_attribute && !j.items[0].attr_was_set && !j.icons_hidden);
    }

    #[test]
    fn journal_round_trips_clears_the_file_when_empty_and_survives_corruption() {
        let dir = std::env::temp_dir().join(format!("desqu-desktop-journal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("desktop-journal.json");

        assert_eq!(Journal::load(&path), Journal::default());
        let mut j = Journal::default();
        j.park("vials", "a", p(16, 2));
        j.save(&path).unwrap();
        assert_eq!(Journal::load(&path), j);

        let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(json["items"][0]["owners"][0], "vials");
        assert_eq!(json["items"][0]["original"]["x"], 16);

        j.unpark("vials", "a");
        j.save(&path).unwrap();
        assert!(!path.exists(), "an empty journal must not leave a file");

        // Having switched the desktop's own icons off is worth a file even with nothing parked.
        j.icons_hidden = true;
        j.save(&path).unwrap();
        assert!(Journal::load(&path).icons_hidden);
        j.icons_hidden = false;
        j.save(&path).unwrap();
        assert!(!path.exists());

        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "{ nope").unwrap();
        assert_eq!(Journal::load(&path), Journal::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
