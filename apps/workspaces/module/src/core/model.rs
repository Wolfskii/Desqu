//! The Workspaces data model: one workspace per virtual desktop, kept by the desktop's id.

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const CONFIG_VERSION: u32 = 1;
const MAX_NAME: usize = 80;

/// An app to start again when a workspace is restored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppLaunch {
    /// Absolute path of the executable.
    pub exe: String,
    /// Command-line arguments, as one string (what follows the executable in the command line).
    #[serde(default)]
    pub args: Option<String>,
    /// Shown in the UI; defaults to the file name.
    #[serde(default)]
    pub name: String,
}

impl AppLaunch {
    pub fn display_name(&self) -> String {
        if !self.name.trim().is_empty() {
            return self.name.clone();
        }
        Path::new(&self.exe)
            .file_stem()
            .map_or_else(|| self.exe.clone(), |s| s.to_string_lossy().into_owned())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    /// The virtual desktop's id, normalized to `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`.
    pub desktop_id: String,
    /// Remembered so a workspace is still recognisable after its desktop was renamed or removed.
    #[serde(default)]
    pub name: String,
    /// When on, the desktop icons in `hidden_items` are hidden while this desktop is current.
    #[serde(default)]
    pub manage_icons: bool,
    /// Desktop item ids hidden on this desktop. Items not listed (new files) stay visible.
    #[serde(default)]
    pub hidden_items: Vec<String>,
    #[serde(default)]
    pub apps: Vec<AppLaunch>,
    /// Folders opened in Explorer when the workspace is restored.
    #[serde(default)]
    pub folders: Vec<String>,
}

impl Workspace {
    pub fn new(desktop_id: &str, name: &str) -> Self {
        Self {
            desktop_id: desktop_id.to_string(),
            name: name.to_string(),
            manage_icons: false,
            hidden_items: Vec::new(),
            apps: Vec::new(),
            folders: Vec::new(),
        }
    }
}

fn default_version() -> u32 {
    CONFIG_VERSION
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacesConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub workspaces: Vec<Workspace>,
    /// Leaving a desktop refreshes its saved apps and folders from what is open there. Off by
    /// default: it silently replaces the lists.
    #[serde(default)]
    pub auto_save: bool,
}

impl Default for WorkspacesConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            workspaces: Vec::new(),
            auto_save: false,
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

    /// The workspace of `desktop_id`, created empty (named `name`) when there is none yet.
    pub fn workspace_mut_or_create(&mut self, desktop_id: &str, name: &str) -> &mut Workspace {
        if let Some(i) = self
            .workspaces
            .iter()
            .position(|w| w.desktop_id == desktop_id)
        {
            return &mut self.workspaces[i];
        }
        self.workspaces.push(Workspace::new(desktop_id, name));
        self.workspaces.last_mut().expect("just pushed")
    }

    /// Fixes what can be fixed (names, duplicates) and rejects what cannot (a bad desktop id, an
    /// app without an absolute executable path).
    pub fn validated(mut self) -> Result<Self, ConfigError> {
        let mut seen = HashSet::new();
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

            let mut items = HashSet::new();
            ws.hidden_items
                .retain(|i| !i.trim().is_empty() && items.insert(i.clone()));

            let mut exes = HashSet::new();
            for app in &mut ws.apps {
                if !Path::new(&app.exe).is_absolute() {
                    return Err(ConfigError::Invalid(format!(
                        "'{}' is not an absolute path",
                        app.exe
                    )));
                }
                app.args = app
                    .args
                    .take()
                    .map(|a| a.trim().to_string())
                    .filter(|a| !a.is_empty());
                app.name = app.name.trim().chars().take(MAX_NAME).collect();
            }
            // The same exe with the same arguments is one entry.
            ws.apps
                .retain(|a| exes.insert((a.exe.to_ascii_lowercase(), a.args.clone())));

            let mut folders = HashSet::new();
            ws.folders
                .retain(|f| !f.trim().is_empty() && folders.insert(f.to_ascii_lowercase()));
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

    fn app(exe: &str, args: Option<&str>) -> AppLaunch {
        AppLaunch {
            exe: exe.into(),
            args: args.map(str::to_string),
            name: String::new(),
        }
    }

    #[test]
    fn validation_normalizes_and_deduplicates() {
        let mut ws = Workspace::new("753c9b0d-edaa-45d2-ac5a-5740141905ea", "  Work  ");
        ws.hidden_items = vec!["a".into(), "a".into(), " ".into(), "b".into()];
        ws.apps = vec![
            app(r"C:\x\a.exe", Some(" -f ")),
            app(r"c:\x\A.EXE", Some("-f")),
            app(r"C:\x\a.exe", None),
        ];
        ws.folders = vec![r"C:\Work".into(), r"c:\work".into(), "".into()];
        let cfg = WorkspacesConfig {
            workspaces: vec![ws],
            ..Default::default()
        }
        .validated()
        .unwrap();
        let ws = &cfg.workspaces[0];
        assert_eq!(ws.desktop_id, ID);
        assert_eq!(ws.name, "Work");
        assert_eq!(ws.hidden_items, vec!["a", "b"]);
        assert_eq!(ws.apps.len(), 2);
        assert_eq!(ws.apps[0].args.as_deref(), Some("-f"));
        assert_eq!(ws.folders, vec![r"C:\Work"]);
    }

    #[test]
    fn validation_rejects_bad_desktop_ids_duplicates_and_relative_exes() {
        let bad_id = WorkspacesConfig {
            workspaces: vec![Workspace::new("nope", "x")],
            ..Default::default()
        };
        assert!(bad_id.validated().is_err());
        let dup = WorkspacesConfig {
            workspaces: vec![Workspace::new(ID, "a"), Workspace::new(ID, "b")],
            ..Default::default()
        };
        assert!(dup.validated().is_err());
        let mut ws = Workspace::new(ID, "a");
        ws.apps = vec![app("notepad.exe", None)];
        assert!(WorkspacesConfig {
            workspaces: vec![ws],
            ..Default::default()
        }
        .validated()
        .is_err());
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
        cfg.workspace_mut_or_create(ID, "Main").manage_icons = true;
        cfg.save(&path).unwrap();
        assert_eq!(WorkspacesConfig::load(&path).unwrap(), cfg);

        std::fs::write(&path, "{ not json").unwrap();
        assert!(WorkspacesConfig::load(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn display_name_falls_back_to_the_file_name() {
        assert_eq!(app(r"C:\Tools\code.exe", None).display_name(), "code");
        let mut a = app(r"C:\Tools\code.exe", None);
        a.name = "Editor".into();
        assert_eq!(a.display_name(), "Editor");
    }
}
