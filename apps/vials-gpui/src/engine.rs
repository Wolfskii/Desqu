//! The live part (`--live`): owns the desktop layer and the config file. It hides the items that are
//! in a vial (restored on exit, and from the journal after a crash) and remembers where vials are
//! moved to. Everything decidable from data lives in `vials-core`; this only connects it to the
//! desktop and the disk.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use desqu_desktop::DesktopLayer;
use vials_core::layout::{self, MonitorRef};
use vials_core::model::{PixelRect, VialsConfig};

pub struct Engine {
    layer: Arc<dyn DesktopLayer>,
    config: Mutex<VialsConfig>,
    path: PathBuf,
    monitors: Vec<MonitorRef>,
}

impl Engine {
    /// Takes the desktop layer (only one process may own it: not while another Vials is running) and
    /// hides the items of every vial.
    pub fn start(path: &Path, config: VialsConfig, monitors: Vec<MonitorRef>) -> Result<Arc<Self>, String> {
        let journal = path.with_file_name("desktop-journal.json");
        let layer = desqu_desktop::open("vials", &journal).map_err(|e| match e {
            desqu_desktop::DesktopError::OwnedElsewhere => "another Vials or Desqu already owns the desktop".to_string(),
            other => other.to_string(),
        })?;
        let engine = Arc::new(Self { layer, config: Mutex::new(config), path: path.to_path_buf(), monitors });
        engine.hide_vial_items();
        Ok(engine)
    }

    fn hide_vial_items(&self) {
        let cfg = self.config.lock().unwrap_or_else(|e| e.into_inner());
        if !cfg.hide_on_desktop {
            return;
        }
        match self.layer.can_hide() {
            Ok(true) => {}
            _ => {
                eprintln!("Windows shows hidden files, so items cannot be hidden on the desktop");
                return;
            }
        }
        let ids: Vec<String> = cfg.vials.iter().filter(|v| !v.inbox).flat_map(|v| v.items.iter().cloned()).collect();
        if let Err(e) = self.layer.hide(&ids) {
            eprintln!("could not hide the vials' items: {e}");
        }
    }

    /// The user moved or resized a vial window: remember it as fractions of its monitor.
    pub fn persist_rect(&self, vial_id: &str, pixels: PixelRect) {
        let Some(monitor) = layout::monitor_containing(&pixels, &self.monitors) else { return };
        let mut cfg = self.config.lock().unwrap_or_else(|e| e.into_inner());
        let Some(vial) = cfg.vials.iter_mut().find(|v| v.id == vial_id) else { return };
        let mut rect = layout::to_normalized(&pixels, &monitor.bounds);
        if vial.collapsed {
            // The window is only a header: keep the height chosen for the open vial.
            rect.height = vial.rect.height;
        }
        let same = (rect.x - vial.rect.x).abs() < 0.002
            && (rect.y - vial.rect.y).abs() < 0.002
            && (rect.width - vial.rect.width).abs() < 0.002
            && (rect.height - vial.rect.height).abs() < 0.002;
        if same && vial.monitor.as_deref() == Some(monitor.name.as_str()) {
            return;
        }
        vial.rect = rect;
        vial.monitor = Some(monitor.name.clone());
        if let Err(e) = cfg.save(&self.path) {
            eprintln!("could not save the config: {e}");
        }
    }

    /// The items of one vial, in order.
    pub fn vial_items(&self, vial_id: &str) -> Vec<String> {
        let cfg = self.config.lock().unwrap_or_else(|e| e.into_inner());
        cfg.vial(vial_id).map(|v| v.items.clone()).unwrap_or_default()
    }

    /// Whether `id` belongs to a vial: only those may be dragged, opened or given a menu.
    pub fn is_assigned(&self, id: &str) -> bool {
        self.config.lock().unwrap_or_else(|e| e.into_inner()).vial_of(id).is_some()
    }

    /// Changes the config with `edit`, saves it, and hides the items that entered a vial / shows the
    /// ones that left. Returns whether anything changed.
    fn apply(&self, edit: impl FnOnce(&mut VialsConfig) -> bool) -> bool {
        let mut cfg = self.config.lock().unwrap_or_else(|e| e.into_inner());
        let before: HashSet<String> = cfg.assigned_ids().into_iter().collect();
        if !edit(&mut cfg) {
            return false;
        }
        let after: HashSet<String> = cfg.assigned_ids().into_iter().collect();
        if let Err(e) = cfg.save(&self.path) {
            eprintln!("could not save the config: {e}");
        }
        if cfg.hide_on_desktop {
            let entered: Vec<String> = after.difference(&before).cloned().collect();
            if !entered.is_empty() {
                let _ = self.layer.hide(&entered);
            }
        }
        let left: Vec<String> = before.difference(&after).cloned().collect();
        if !left.is_empty() {
            let _ = self.layer.show(&left);
        }
        true
    }

    /// Files dropped on a vial. Only items that really are on the desktop are taken, so a drop
    /// cannot make a vial refer to arbitrary files.
    pub fn drop_items(&self, vial_id: &str, paths: &[String]) -> bool {
        let wanted: HashSet<String> = paths.iter().map(|p| p.replace('/', "\\").to_lowercase()).collect();
        let Ok(on_desktop) = self.layer.items() else { return false };
        let ids: Vec<String> =
            on_desktop.into_iter().filter(|i| wanted.contains(&i.id.replace('/', "\\").to_lowercase())).map(|i| i.id).collect();
        self.apply(|cfg| ids.iter().fold(false, |changed, id| cfg.assign(id, Some(vial_id)) | changed))
    }

    /// Takes an item out of its vial: it is back on the desktop.
    pub fn remove_item(&self, id: &str) -> bool {
        self.apply(|cfg| cfg.assign(id, None))
    }

    /// An item dragged out of vial `source` has landed: on the empty desktop it is placed there as a
    /// block and leaves the vial; moved by another target (a folder, Explorer) it leaves the vial
    /// too, but only if it is still in the vial it was dragged from.
    pub fn settle_drag(&self, ids: &[String], source: Option<&str>, end: vials_platform::DragEnd) -> bool {
        use vials_platform::DragEnd;
        match end {
            DragEnd::OnDesktop((x, y)) => {
                if let Err(e) = self.layer.show_block(ids, desqu_desktop::Point { x, y }) {
                    eprintln!("could not place the items on the desktop: {e}");
                }
                self.apply(|cfg| ids.iter().fold(false, |c, id| cfg.assign(id, None) | c))
            }
            DragEnd::Dropped { moved: true, .. } => self.apply(|cfg| {
                let still: Vec<String> = ids
                    .iter()
                    .filter(|id| source.is_some() && cfg.vial_of(id).map(|v| v.id.as_str()) == source)
                    .cloned()
                    .collect();
                still.iter().fold(false, |c, id| cfg.assign(id, None) | c)
            }),
            _ => false,
        }
    }

    /// Which vial holds `id` (the source of a drag).
    pub fn vial_id_of(&self, id: &str) -> Option<String> {
        self.config.lock().unwrap_or_else(|e| e.into_inner()).vial_of(id).map(|v| v.id.clone())
    }

    /// Puts every hidden item back on the desktop.
    pub fn restore(&self) {
        let _ = self.layer.show_all();
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.restore();
    }
}

/// Needs the real desktop and no other Vials running, and creates two throw-away files on the
/// Desktop (removed again): `cargo test --release -- --ignored --test-threads=1`.
#[cfg(test)]
mod tests {
    use super::*;
    use vials_core::model::{NormalizedRect, Vial};

    fn hidden(p: &Path) -> bool {
        use std::os::windows::fs::MetadataExt;
        std::fs::metadata(p).map(|m| m.file_attributes() & 0x2 != 0).unwrap_or(false)
    }

    fn unhide_and_delete(p: &Path) {
        // Restores the attribute through the same path a user would (attrib), then deletes.
        let _ = std::process::Command::new("attrib").arg("-h").arg(p).status();
        let _ = std::fs::remove_file(p);
    }

    #[test]
    #[ignore = "uses the real desktop"]
    fn drop_remove_and_drag_settle_move_items_between_a_vial_and_the_desktop() {
        let desktop = PathBuf::from(std::env::var_os("USERPROFILE").unwrap()).join("Desktop");
        let a = desktop.join("zz-vials-engine-a.txt");
        let b = desktop.join("zz-vials-engine-b.txt");
        std::fs::write(&a, "a").unwrap();
        std::fs::write(&b, "b").unwrap();
        let dir = std::env::temp_dir().join(format!("vials-engine-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg_path = dir.join("config.json");
        let mut cfg = VialsConfig::default();
        cfg.vials.push(Vial {
            id: "vial-1".into(),
            name: "t".into(),
            monitor: None,
            rect: NormalizedRect { x: 0.1, y: 0.1, width: 0.1, height: 0.1 },
            items: vec![],
            collapsed: false,
            inbox: false,
            sort_key: None,
            sort_desc: false,
            folders_first: false,
        });
        cfg.save(&cfg_path).unwrap();

        let result = std::panic::catch_unwind(|| {
            let engine = Engine::start(&cfg_path, cfg, Vec::new()).expect("engine");
            let (ida, idb) = (a.to_string_lossy().into_owned(), b.to_string_lossy().into_owned());

            // The desktop notices new files after a few seconds.
            let mut taken = false;
            for _ in 0..40 {
                // A path with forward slashes, as a drop can deliver it.
                taken = engine.drop_items("vial-1", &[ida.replace('\\', "/"), idb.clone()]);
                if taken {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            assert!(taken, "the dropped desktop files should have joined the vial");
            assert_eq!(engine.vial_items("vial-1").len(), 2);
            assert!(hidden(&a) && hidden(&b), "items in a vial are hidden on the desktop");

            // Something that is not on the desktop is ignored.
            assert!(!engine.drop_items("vial-1", &[r"C:\Windows\notepad.exe".to_string()]));

            assert!(engine.remove_item(&ida));
            assert!(!hidden(&a), "an item taken out is back on the desktop");
            assert_eq!(engine.vial_items("vial-1"), vec![idb.clone()]);

            // Dropped on the empty desktop: placed there and out of the vial.
            let changed = engine.settle_drag(std::slice::from_ref(&idb), Some("vial-1"), vials_platform::DragEnd::OnDesktop((300, 300)));
            assert!(changed && engine.vial_items("vial-1").is_empty());
            assert!(!hidden(&b));

            // The saved config reflects it.
            let saved = VialsConfig::load(&cfg_path).unwrap();
            assert!(saved.vial("vial-1").unwrap().items.is_empty());
        });
        unhide_and_delete(&a);
        unhide_and_delete(&b);
        let _ = std::fs::remove_dir_all(&dir);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }
}
