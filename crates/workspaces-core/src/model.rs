//! The Workspaces data model: each virtual desktop can own desktop items (shortcuts, files,
//! folders). An owned item is shown only on its desktop and hidden on the others; items nobody
//! owns stay visible everywhere. Items never move out of the Desktop folders.

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const CONFIG_VERSION: u32 = 1;
const MAX_NAME: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    /// The virtual desktop's id, normalized to `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`.
    pub desktop_id: String,
    /// Remembered so a workspace is still recognisable after its desktop was renamed or removed.
    #[serde(default)]
    pub name: String,
    /// Desktop item ids (absolute parsing names) that belong to this desktop.
    #[serde(default)]
    pub items: Vec<String>,
}

impl Workspace {
    pub fn new(desktop_id: &str, name: &str) -> Self {
        Self {
            desktop_id: desktop_id.to_string(),
            name: name.to_string(),
            items: Vec::new(),
        }
    }
}

fn default_version() -> u32 {
    CONFIG_VERSION
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacesConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub workspaces: Vec<Workspace>,
    /// A file that appears on the desktop while Workspaces runs belongs to the desktop you are on.
    #[serde(default = "yes")]
    pub auto_assign_new: bool,
    /// The first-run question ("which desktop do your current items belong to?") was answered.
    #[serde(default)]
    pub initialized: bool,
}

impl Default for WorkspacesConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            workspaces: Vec::new(),
            auto_assign_new: true,
            initialized: false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("config is not valid JSON: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid config: {0}")]
    Invalid(String),
}

/// Normalizes a desktop id to upper-case braces form, or `None` if it is not a GUID.
pub fn normalize_guid(text: &str) -> Option<String> {
    let t = text.trim().trim_start_matches('{').trim_end_matches('}');
    let groups: Vec<&str> = t.split('-').collect();
    let lens = [8usize, 4, 4, 4, 12];
    if groups.len() != 5
        || groups
            .iter()
            .zip(lens)
            .any(|(g, n)| g.len() != n || !g.chars().all(|c| c.is_ascii_hexdigit()))
    {
        return None;
    }
    Some(format!("{{{}}}", t.to_ascii_uppercase()))
}

/// Formats 16 registry bytes (a `GUID` in memory layout) as a normalized desktop id.
pub fn guid_from_registry_bytes(b: &[u8]) -> Option<String> {
    if b.len() < 16 {
        return None;
    }
    let d1 = u32::from_le_bytes(b[0..4].try_into().ok()?);
    let d2 = u16::from_le_bytes(b[4..6].try_into().ok()?);
    let d3 = u16::from_le_bytes(b[6..8].try_into().ok()?);
    Some(format!(
        "{{{d1:08X}-{d2:04X}-{d3:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    ))
}

impl WorkspacesConfig {
    pub fn workspace(&self, desktop_id: &str) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.desktop_id == desktop_id)
    }

    /// The desktop that owns `item_id`, if any.
    pub fn owner_of(&self, item_id: &str) -> Option<&str> {
        self.workspaces
            .iter()
            .find(|w| w.items.iter().any(|i| i == item_id))
            .map(|w| w.desktop_id.as_str())
    }

    /// Gives `item_id` to `desktop` (id, name; the workspace is created when it has none yet), or
    /// with `None` makes it visible everywhere again. Returns whether anything changed.
    pub fn assign(&mut self, item_id: &str, desktop: Option<(&str, &str)>) -> bool {
        let already = self.owner_of(item_id).map(str::to_string);
        if already.as_deref() == desktop.map(|(id, _)| id) {
            return false;
        }
        for ws in &mut self.workspaces {
            ws.items.retain(|i| i != item_id);
        }
        if let Some((id, name)) = desktop {
            if !self.workspaces.iter().any(|w| w.desktop_id == id) {
                self.workspaces.push(Workspace::new(id, name));
            }
            if let Some(ws) = self.workspaces.iter_mut().find(|w| w.desktop_id == id) {
                ws.items.push(item_id.to_string());
            }
        }
        true
    }

    /// Fixes what can be fixed (names, an item listed twice) and rejects what cannot (a bad or
    /// duplicated desktop id).
    pub fn validated(mut self) -> Result<Self, ConfigError> {
        let mut seen = HashSet::new();
        let mut owned = HashSet::new();
        for ws in &mut self.workspaces {
            let id = normalize_guid(&ws.desktop_id).ok_or_else(|| {
                ConfigError::Invalid(format!("'{}' is not a virtual desktop id", ws.desktop_id))
            })?;
            if !seen.insert(id.clone()) {
                return Err(ConfigError::Invalid(format!(
                    "two workspaces for desktop {id}"
                )));
            }
            ws.desktop_id = id;
            ws.name = ws.name.trim().chars().take(MAX_NAME).collect();
            // An item belongs to one desktop: the first workspace that lists it wins.
            ws.items
                .retain(|i| !i.trim().is_empty() && owned.insert(i.clone()));
        }
        self.version = CONFIG_VERSION;
        Ok(self)
    }

    /// A missing file yields defaults; a corrupt one is an error so the caller can back it up
    /// instead of silently overwriting the user's workspaces.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str::<WorkspacesConfig>(&text)?.validated(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Temp file + rename, so a crash mid-write cannot leave a truncated file.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "{753C9B0D-EDAA-45D2-AC5A-5740141905EA}";
    const ID2: &str = "{3FEBEE14-E0E2-4DCE-A63D-09646A16F375}";

    #[test]
    fn guids_are_normalized_and_bad_ones_rejected() {
        assert_eq!(
            normalize_guid("753c9b0d-edaa-45d2-ac5a-5740141905ea").as_deref(),
            Some(ID)
        );
        assert_eq!(normalize_guid(&format!(" {ID} ")).as_deref(), Some(ID));
        assert!(normalize_guid("not-a-guid").is_none());
        assert!(normalize_guid("753C9B0D-EDAA-45D2-AC5A-5740141905E").is_none());
        assert!(normalize_guid("753C9B0D-EDAA-45D2-AC5A-5740141905EG").is_none());
    }

    #[test]
    fn registry_bytes_match_the_text_form_seen_on_a_real_machine() {
        // {753C9B0D-EDAA-45D2-AC5A-5740141905EA} as stored in VirtualDesktopIDs.
        let bytes = [
            0x0D, 0x9B, 0x3C, 0x75, 0xAA, 0xED, 0xD2, 0x45, 0xAC, 0x5A, 0x57, 0x40, 0x14, 0x19,
            0x05, 0xEA,
        ];
        assert_eq!(guid_from_registry_bytes(&bytes).as_deref(), Some(ID));
        assert!(guid_from_registry_bytes(&bytes[..10]).is_none());
    }

    #[test]
    fn assigning_moves_an_item_between_desktops_and_releases_it() {
        let mut cfg = WorkspacesConfig::default();
        assert!(cfg.assign("a.txt", Some((ID, "Work"))));
        assert!(!cfg.assign("a.txt", Some((ID, "Work"))));
        assert_eq!(cfg.owner_of("a.txt"), Some(ID));
        assert!(cfg.assign("a.txt", Some((ID2, "Play"))));
        assert_eq!(cfg.owner_of("a.txt"), Some(ID2));
        assert!(cfg.workspace(ID).unwrap().items.is_empty());
        assert!(cfg.assign("a.txt", None));
        assert_eq!(cfg.owner_of("a.txt"), None);
        assert!(!cfg.assign("a.txt", None));
    }

    #[test]
    fn validation_normalizes_and_gives_an_item_to_one_desktop() {
        let mut a = Workspace::new("753c9b0d-edaa-45d2-ac5a-5740141905ea", "  Work  ");
        a.items = vec!["x".into(), "x".into(), " ".into(), "y".into()];
        let mut b = Workspace::new(ID2, "Play");
        b.items = vec!["y".into(), "z".into()];
        let cfg = WorkspacesConfig {
            workspaces: vec![a, b],
            ..Default::default()
        }
        .validated()
        .unwrap();
        assert_eq!(cfg.workspaces[0].desktop_id, ID);
        assert_eq!(cfg.workspaces[0].name, "Work");
        assert_eq!(cfg.workspaces[0].items, vec!["x", "y"]);
        assert_eq!(cfg.workspaces[1].items, vec!["z"]);
    }

    #[test]
    fn validation_rejects_bad_desktop_ids_and_duplicates() {
        let bad = WorkspacesConfig {
            workspaces: vec![Workspace::new("nope", "x")],
            ..Default::default()
        };
        assert!(bad.validated().is_err());
        let dup = WorkspacesConfig {
            workspaces: vec![Workspace::new(ID, "a"), Workspace::new(ID, "b")],
            ..Default::default()
        };
        assert!(dup.validated().is_err());
    }

    #[test]
    fn missing_file_is_default_corrupt_is_an_error_and_save_round_trips() {
        let dir = std::env::temp_dir().join(format!("desqu-ws-model-{}", std::process::id()));
        let path = dir.join("config.json");
        assert_eq!(
            WorkspacesConfig::load(&path).unwrap(),
            WorkspacesConfig::default()
        );

        let mut cfg = WorkspacesConfig::default();
        cfg.assign("a.txt", Some((ID, "Main")));
        cfg.save(&path).unwrap();
        assert_eq!(WorkspacesConfig::load(&path).unwrap(), cfg);

        std::fs::write(&path, "{ not json").unwrap();
        assert!(WorkspacesConfig::load(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn configs_from_the_first_draft_still_load() {
        let text = format!(
            r#"{{"version":1,"autoSave":true,"workspaces":[{{"desktopId":"{ID}","manageIcons":true,"hiddenItems":["q"],"apps":[],"folders":[]}}]}}"#
        );
        let cfg: WorkspacesConfig = serde_json::from_str(&text).unwrap();
        assert!(cfg.validated().unwrap().workspaces[0].items.is_empty());
    }
}
