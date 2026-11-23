//! The live part (`--live`): owns the desktop layer and the config file. It hides the items that are
//! in a vial (restored on exit, and from the journal after a crash) and remembers where vials are
//! moved to. Everything decidable from data lives in `vials-core`; this only connects it to the
//! desktop and the disk.

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
