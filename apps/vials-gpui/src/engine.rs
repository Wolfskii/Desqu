//! The live part (`--live`): owns the desktop layer and the config file. It hides the items that are
//! in a vial (restored on exit, and from the journal after a crash) and remembers where vials are
//! moved to. Everything decidable from data lives in `vials-core`; this only connects it to the
//! desktop and the disk.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use desqu_desktop::DesktopLayer;
use vials_core::layout::{self, MonitorRef};
use vials_core::model::{PixelRect, Vial, VialsConfig};
use vials_core::{clip, rules, sort};

pub struct Engine {
    layer: Arc<dyn DesktopLayer>,
    config: Mutex<VialsConfig>,
    path: PathBuf,
    monitors: Vec<MonitorRef>,
    watch: Mutex<Option<desqu_desktop::Subscription>>,
    /// The panel colour taken from the wallpaper (set by [`Engine::refresh_wallpaper_color`]).
    wallpaper_color: Mutex<Option<u32>>,
    /// The vials were put at other places (a layout came back): the windows have to follow.
    places_changed: std::sync::atomic::AtomicBool,
    /// What the vials looked like before the last few changes the user made: for undo.
    history: Mutex<vials_core::history::History<VialsConfig>>,
}

impl Engine {
    /// Opens the desktop layer (shared with Workspaces when that runs too) and hides the items of
    /// every vial.
    pub fn start(path: &Path, config: VialsConfig, monitors: Vec<MonitorRef>) -> Result<Arc<Self>, String> {
        let journal = path.with_file_name("desktop-journal.json");
        let layer = desqu_desktop::open("vials", &journal).map_err(|e| match e {
            desqu_desktop::DesktopError::OwnedElsewhere => "the desktop is held by an older Desqu, Vials or Workspaces that does not share it: quit that one first".to_string(),
            other => other.to_string(),
        })?;
        let engine = Arc::new(Self { layer, config: Mutex::new(config), path: path.to_path_buf(), monitors, watch: Mutex::new(None), places_changed: Default::default(), wallpaper_color: Mutex::new(None), history: Mutex::new(vials_core::history::History::new(30)) });
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
        if self.is_locked(vial_id) {
            return;
        }
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
    #[cfg(test)]
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
        self.apply_with(true, edit)
    }

    /// Like `apply`; `record` says whether undo can take the change back (what the user did, not what
    /// the desktop or a setting did).
    fn apply_with(&self, record: bool, edit: impl FnOnce(&mut VialsConfig) -> bool) -> bool {
        let mut cfg = self.config.lock().unwrap_or_else(|e| e.into_inner());
        let before: HashSet<String> = cfg.assigned_ids().into_iter().collect();
        let snapshot = record.then(|| cfg.clone());
        if !edit(&mut cfg) {
            return false;
        }
        if let Some(snapshot) = snapshot {
            self.history.lock().unwrap_or_else(|e| e.into_inner()).push(snapshot);
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

    /// A new vial in `rect` holding the desktop items `ids` (those that really are on the desktop).
    pub fn create_vial_with(&self, rect: PixelRect, ids: &[String]) -> Option<String> {
        let vial = self.create_vial(rect)?;
        self.drop_items(&vial, ids);
        Some(vial)
    }

    /// Takes an item out of its vial: it is back on the desktop.
    pub fn remove_item(&self, id: &str) -> bool {
        if self.vial_id_of(id).is_some_and(|v| self.is_locked(&v)) {
            return false;
        }
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

    /// A copy of the vials as they are now (the window manager compares it with the open windows).
    pub fn vials(&self) -> Vec<Vial> {
        self.config.lock().unwrap_or_else(|e| e.into_inner()).vials.iter().filter(|v| !v.inbox).cloned().collect()
    }

    /// Calls `notify` (on another thread) whenever the desktop's files change.
    pub fn watch(&self, notify: impl Fn() + Send + 'static) {
        match self.layer.subscribe(Box::new(move |_| notify())) {
            Ok(sub) => *self.watch.lock().unwrap_or_else(|e| e.into_inner()) = Some(sub),
            Err(e) => eprintln!("cannot watch the desktop: {e}"),
        }
    }

    /// Takes the desktop's current state into account: new items that match a rule go into their vial,
    /// vials with a sort keep their order. Returns whether anything changed.
    pub fn reconcile(&self) -> bool {
        let Ok(items) = self.layer.items() else { return false };
        // The desktop keeps listing a deleted file for a few seconds: never put such a ghost in a vial.
        let items: Vec<_> = items.into_iter().filter(|i| i.id.starts_with("::") || Path::new(&i.id).exists()).collect();
        let names: std::collections::HashMap<&str, &str> = items.iter().map(|i| (i.id.as_str(), i.name.as_str())).collect();
        let changed = self.apply_with(false, |cfg| {
            let assignments = rules::assign_new_items(cfg, &items);
            for (item, vial) in &assignments {
                cfg.assign(item, Some(vial));
            }
            let reordered = sort::apply_orders(cfg, |id| vials_platform::facts::item_facts(id, names.get(id).copied()));
            !assignments.is_empty() || reordered
        });
        // A file that has just appeared may not take the hidden attribute the first time: say it again
        // for everything that should be hidden and is not.
        self.ensure_hidden();
        changed
    }

    /// Hides the items of vials that are not hidden yet (idempotent).
    fn ensure_hidden(&self) {
        let cfg = self.config.lock().unwrap_or_else(|e| e.into_inner());
        if !cfg.hide_on_desktop {
            return;
        }
        let Ok(hidden) = self.layer.hidden() else { return };
        let hidden: HashSet<String> = hidden.into_iter().collect();
        let missing: Vec<String> = cfg.assigned_ids().into_iter().filter(|id| !hidden.contains(id)).collect();
        if !missing.is_empty() {
            let _ = self.layer.hide(&missing);
        }
    }

    /// Orders a vial's items by name, type, modified or size, and keeps that order as items change.
    pub fn sort_vial(&self, vial_id: &str, key: &str, descending: bool) -> Result<(), String> {
        let parsed = sort::SortKey::parse(key).ok_or("unknown sort order")?;
        let names: std::collections::HashMap<String, String> =
            self.layer.items().map_err(|e| e.to_string())?.into_iter().map(|i| (i.id, i.name)).collect();
        let mut result = Err("that vial no longer exists".to_string());
        self.apply(|cfg| {
            let Some(vial) = cfg.vials.iter_mut().find(|v| v.id == vial_id && !v.inbox) else { return false };
            let facts: Vec<_> =
                vial.items.iter().map(|id| vials_platform::facts::item_facts(id, names.get(id).map(String::as_str))).collect();
            vial.items = sort::sorted_with(&facts, parsed, descending, vial.folders_first);
            vial.sort_key = Some(key.to_string());
            vial.sort_desc = descending;
            result = Ok(());
            true
        });
        result
    }

    pub fn set_folders_first(&self, vial_id: &str, on: bool) {
        let names: std::collections::HashMap<String, String> =
            self.layer.items().map(|v| v.into_iter().map(|i| (i.id, i.name)).collect()).unwrap_or_default();
        self.apply(|cfg| {
            let Some(vial) = cfg.vials.iter_mut().find(|v| v.id == vial_id && !v.inbox) else { return false };
            vial.folders_first = on;
            sort::apply_orders(cfg, |id| vials_platform::facts::item_facts(id, names.get(id).map(String::as_str)));
            true
        });
    }

    /// Ctrl+V: the clipboard's files come to the Desktop (moved when cut, copied otherwise, by
    /// Explorer's own file operation) and go into the vial. Returns how many the vial received. Blocks
    /// until the file operation is done: call it off the UI thread.
    pub fn paste(&self, vial_id: &str) -> Result<usize, String> {
        if self.config.lock().unwrap_or_else(|e| e.into_inner()).vial(vial_id).is_none() {
            return Err("that vial no longer exists".into());
        }
        let folders: Vec<String> = desqu_desktop::desktop_folders().iter().map(|p| p.to_string_lossy().into_owned()).collect();
        let target = folders.first().cloned().ok_or("the Desktop folder was not found")?;
        let ids: Vec<String> = self.layer.items().map_err(|e| e.to_string())?.into_iter().map(|i| i.id).collect();
        // A thread of its own: the file operation needs a COM apartment and shows its own dialogs.
        let items = std::thread::spawn(move || -> Result<Vec<String>, String> {
            let clipboard = vials_platform::clipboard::files()?.ok_or("There are no files on the clipboard")?;
            let plan = clip::plan_paste(&clipboard.paths, clipboard.cut, &folders);
            let mut items = clip::matching_ids(&plan.adopt, &ids);
            items.extend(vials_platform::clipboard::transfer(&plan.copy_here, false, &target, true)?);
            items.extend(vials_platform::clipboard::transfer(&plan.transfer, clipboard.cut, &target, false)?);
            if clipboard.cut {
                vials_platform::clipboard::clear();
            }
            Ok(items)
        })
        .join()
        .map_err(|_| "the paste thread panicked".to_string())??;
        let count = items.len();
        if count > 0 {
            self.apply(|cfg| items.iter().fold(false, |c, id| cfg.assign(id, Some(vial_id)) | c));
        }
        Ok(count)
    }

    /// A new empty vial at a screen rectangle (physical pixels).
    pub fn create_vial(&self, rect: PixelRect) -> Option<String> {
        let monitor = layout::monitor_containing(&rect, &self.monitors)?;
        let normalized = layout::to_normalized(&rect, &monitor.bounds);
        let name = monitor.name.clone();
        let mut id = None;
        self.apply(|cfg| {
            id = Some(cfg.add_vial(normalized, Some(name)));
            true
        });
        id
    }

    /// A new empty vial of the default size (380 x 260 dip) with its corner at `point` (physical screen
    /// pixels), kept on its monitor. `None` point: the middle of the primary monitor.
    /// A folder portal: a vial that shows `folder`, in `rect`.
    pub fn create_portal(&self, rect: PixelRect, folder: &str) -> Option<String> {
        let monitor = layout::monitor_containing(&rect, &self.monitors)?;
        let normalized = layout::to_normalized(&rect, &monitor.bounds);
        let name = monitor.name.clone();
        let mut id = None;
        self.apply(|cfg| {
            id = Some(cfg.add_portal(normalized, Some(name), folder));
            true
        });
        id
    }

    /// A folder portal of the usual size at a point (the pointer, say).
    pub fn create_portal_at(&self, point: Option<(i32, i32)>, folder: &str) -> Option<String> {
        let rect = self.popup_rect(point)?;
        self.create_portal(rect, folder)
    }

    /// Where a new vial of the usual size goes when it is made at `point` (the middle of the primary monitor
    /// without one), kept on its monitor.
    fn popup_rect(&self, point: Option<(i32, i32)>) -> Option<PixelRect> {
        let primary = self.monitors.iter().find(|m| m.primary).or(self.monitors.first())?;
        let point = point.unwrap_or(((primary.bounds.left + primary.bounds.right) / 2, (primary.bounds.top + primary.bounds.bottom) / 2));
        let at = PixelRect { left: point.0, top: point.1, right: point.0 + 1, bottom: point.1 + 1 };
        let monitor = layout::monitor_containing(&at, &self.monitors).unwrap_or(primary);
        let size = ((380.0 * monitor.scale).round() as i32, (260.0 * monitor.scale).round() as i32);
        let (x, y) = vials_core::gesture::popup_position(point, size, &monitor.bounds);
        Some(PixelRect { left: x, top: y, right: x + size.0, bottom: y + size.1 })
    }

    pub fn create_vial_at(&self, point: Option<(i32, i32)>) -> Option<String> {
        let primary = self.monitors.iter().find(|m| m.primary).or(self.monitors.first())?;
        let point = point.unwrap_or(((primary.bounds.left + primary.bounds.right) / 2, (primary.bounds.top + primary.bounds.bottom) / 2));
        let at = PixelRect { left: point.0, top: point.1, right: point.0 + 1, bottom: point.1 + 1 };
        let monitor = layout::monitor_containing(&at, &self.monitors).unwrap_or(primary);
        let size = ((380.0 * monitor.scale).round() as i32, (260.0 * monitor.scale).round() as i32);
        let (x, y) = vials_core::gesture::popup_position(point, size, &monitor.bounds);
        self.create_vial(PixelRect { left: x, top: y, right: x + size.0, bottom: y + size.1 })
    }

    /// Changes the settings (anything but the vials themselves). Returns whether something changed. Turning
    /// "hide items on the desktop" on or off hides or shows the vials' items at once.
    pub fn edit_settings(&self, edit: impl FnOnce(&mut VialsConfig)) -> bool {
        let before = self.config();
        let changed = self.apply_with(false, |cfg| {
            edit(cfg);
            *cfg != before
        });
        if changed {
            let after = self.config();
            if after.hide_on_desktop {
                self.ensure_hidden();
            } else {
                let _ = self.layer.show(&after.assigned_ids());
            }
            if after.context_menu != before.context_menu {
                let exe = std::env::current_exe().ok();
                let _ = vials_platform::set_context_menu(if after.context_menu { exe.as_deref().and_then(|p| p.to_str()) } else { None });
            }
        }
        changed
    }

    /// Adds a rule (tried after the existing ones). Desktop items that already match it and sit in no vial
    /// are sorted at once.
    /// Looks at the wallpaper again (off the UI thread: it is a picture to decode) and keeps the panel colour it
    /// gives. Returns whether that colour changed.
    pub fn refresh_wallpaper_color(&self) -> bool {
        let color = if self.config().wallpaper_tint { crate::liquid::current().and_then(|w| w.average_color()).map(vials_core::chameleon::panel_color) } else { None };
        let mut slot = self.wallpaper_color.lock().unwrap_or_else(|e| e.into_inner());
        let changed = *slot != color;
        *slot = color;
        changed
    }

    /// A vial that shows the folder of notes and links, named Notes.
    pub fn create_notes_at(&self, point: Option<(i32, i32)>) -> Option<String> {
        let folder = vials_core::notes::folder();
        std::fs::create_dir_all(&folder).ok()?;
        let vial = self.create_portal_at(point, &folder.to_string_lossy())?;
        self.rename_vial(&vial, "Notes");
        Some(vial)
    }

    /// A vial named "Archive" with a rule that sends it whatever has not been changed for `days` days.
    pub fn create_archive_at(&self, point: Option<(i32, i32)>, days: u32) -> Option<String> {
        let vial = self.create_vial_at(point)?;
        self.rename_vial(&vial, "Archive");
        self.add_rule(&format!("not changed for {days} days"), vials_core::model::ItemMatcher::ModifiedOlderThanDays(days), &vial);
        Some(vial)
    }

    pub fn add_rule(&self, name: &str, matcher: vials_core::model::ItemMatcher, vial_id: &str) -> bool {
        let added = self.apply(|cfg| cfg.add_rule(name, matcher, vial_id).is_some());
        if added {
            self.reconcile();
        }
        added
    }

    pub fn delete_rule(&self, id: &str) -> bool {
        self.apply(|cfg| cfg.delete_rule(id))
    }

    pub fn set_rule_enabled(&self, id: &str, enabled: bool) -> bool {
        let changed = self.apply(|cfg| cfg.set_rule_enabled(id, enabled));
        if changed && enabled {
            self.reconcile();
        }
        changed
    }

    pub fn move_rule(&self, id: &str, delta: i32) -> bool {
        self.apply(|cfg| cfg.move_rule(id, delta))
    }

    /// The look of one vial: the common look with this vial's own tint and colour.
    pub fn look_for(&self, vial_id: &str) -> crate::theme::Theme {
        let cfg = self.config.lock().unwrap_or_else(|e| e.into_inner());
        let vial = cfg.vial(vial_id);
        let tint = vial.and_then(|v| v.tint).unwrap_or(cfg.glass_tint);
        let look = crate::theme::Theme::of(cfg.theme, tint);
        let wallpaper = cfg.wallpaper_tint.then(|| *self.wallpaper_color.lock().unwrap_or_else(|e| e.into_inner())).flatten();
        match vial.and_then(|v| v.color_rgb()).or(wallpaper) {
            Some(rgb) => look.with_color(rgb),
            None => look,
        }
    }

    /// Changes how one vial looks (colour, tint, icon size, title). Returns whether anything changed.
    pub fn set_look(&self, vial_id: &str, change: impl FnOnce(&mut Vial)) -> bool {
        self.apply(|cfg| match cfg.vials.iter_mut().find(|v| v.id == vial_id) {
            Some(v) => {
                let before = v.clone();
                change(v);
                *v != before
            }
            None => false,
        })
    }

    pub fn config(&self) -> VialsConfig {
        self.config.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Deletes a vial; its items are back on the desktop.
    pub fn delete_vial(&self, vial_id: &str) -> bool {
        self.apply(|cfg| cfg.delete_vial(vial_id))
    }

    pub fn rename_vial(&self, vial_id: &str, name: &str) -> bool {
        if self.is_locked(vial_id) {
            return false;
        }
        self.apply(|cfg| cfg.rename_vial(vial_id, name))
    }

    /// Saves where the vials are under a name (a new "Layout N" for an empty one). Returns the name.
    pub fn save_layout(&self, name: &str) -> String {
        let mut saved = String::new();
        self.apply_with(false, |cfg| {
            saved = vials_core::layouts::save_named(cfg, name);
            true
        });
        saved
    }

    /// Puts the vials where a named layout had them.
    pub fn restore_layout(&self, name: &str) -> bool {
        let moved = self.apply(|cfg| vials_core::layouts::restore_named(cfg, name));
        if moved {
            self.places_changed.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        moved
    }

    pub fn delete_layout(&self, name: &str) -> bool {
        self.apply_with(false, |cfg| vials_core::layouts::delete_named(cfg, name))
    }

    /// The names of the saved layouts.
    pub fn layout_names(&self) -> Vec<String> {
        let cfg = self.config.lock().unwrap_or_else(|e| e.into_inner());
        vials_core::layouts::named(&cfg).into_iter().map(|l| l.name.clone()).collect()
    }

    /// The screens changed from one setup to another: the vials go where they were in that setup.
    pub fn switch_setup(&self, old: &str, new: &str) -> bool {
        let moved = self.apply_with(false, |cfg| {
            let before = cfg.clone();
            vials_core::layouts::switch_setup(cfg, old, new);
            *cfg != before
        });
        self.places_changed.store(true, std::sync::atomic::Ordering::SeqCst);
        moved
    }

    /// Whether the vials were put at other places since this was last asked.
    pub fn take_places_changed(&self) -> bool {
        self.places_changed.swap(false, std::sync::atomic::Ordering::SeqCst)
    }

    /// Whether there is something to undo.
    pub fn can_undo(&self) -> bool {
        !self.history.lock().unwrap_or_else(|e| e.into_inner()).is_empty()
    }

    /// Takes back the last change the user made to the vials, their items or the rules.
    pub fn undo(&self) -> bool {
        let Some(before) = self.history.lock().unwrap_or_else(|e| e.into_inner()).pop() else { return false };
        self.apply_with(false, |cfg| {
            let mut restored = cfg.clone();
            restored.restore_content(before);
            let changed = restored != *cfg;
            *cfg = restored;
            changed
        })
    }

    /// Makes `vial` (with its tabs) a tab of `host`.
    pub fn merge_tab(&self, vial: &str, host: &str) -> bool {
        self.apply(|cfg| cfg.merge_tab(vial, host))
    }

    /// Takes a tab out into a vial of its own.
    pub fn split_tab(&self, vial: &str) -> bool {
        self.apply(|cfg| cfg.split_tab(vial))
    }

    /// Whether the vial is locked.
    pub fn is_locked(&self, vial_id: &str) -> bool {
        self.config.lock().unwrap_or_else(|e| e.into_inner()).is_locked(vial_id)
    }

    /// Locks or unlocks a vial.
    pub fn set_locked(&self, vial_id: &str, locked: bool) -> bool {
        self.apply(|cfg| cfg.set_locked(vial_id, locked))
    }

    pub fn set_collapsed(&self, vial_id: &str, collapsed: bool) {
        self.apply_with(false, |cfg| match cfg.vials.iter_mut().find(|v| v.id == vial_id) {
            Some(v) if v.collapsed != collapsed => {
                v.collapsed = collapsed;
                true
            }
            _ => false,
        });
    }

    /// Switches the desktop's own icons off (and back on): what quick-hide does besides hiding the vials.
    pub fn set_veil(&self, on: bool) {
        if let Err(e) = self.layer.veil(on) {
            eprintln!("could not {} the desktop icons: {e}", if on { "hide" } else { "show" });
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

/// Need the real desktop and no other Vials running, and create throw-away files on the Desktop
/// (removed again). The desktop layer is process-wide and a second one started right after the
/// first was torn down does not see new files, so run each test in its own process, and wait a little
/// between runs (right after another test deleted its files, Explorer can take a minute to list new ones):
/// `cargo test --release drop_remove -- --ignored` and `cargo test --release rules_sort -- --ignored`.
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
        let a = desktop.join(format!("zz-vials-engine-a-{}.txt", std::process::id()));
        let b = desktop.join(format!("zz-vials-engine-b-{}.txt", std::process::id()));
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
            locked: false,
            tint: None,
            color: None,
            icon_size: None,
            title_center: false,
            source: None,
            tab_host: None,
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

    #[test]
    #[ignore = "uses the real desktop and the clipboard"]
    fn rules_sort_paste_and_the_vial_lifecycle_work_together() {
        use vials_core::model::{ItemMatcher, SortRule};
        let desktop = PathBuf::from(std::env::var_os("USERPROFILE").unwrap()).join("Desktop");
        let rule_file = desktop.join(format!("zz-vials-rule-b-{}.zzvtest", std::process::id()));
        let rule_file2 = desktop.join(format!("zz-vials-rule-a-{}.zzvtest", std::process::id()));
        let source = std::env::temp_dir().join(format!("zz-vials-paste-{}.txt", std::process::id()));
        std::fs::write(&source, "pasted").unwrap();
        let dir = std::env::temp_dir().join(format!("vials-engine-test2-{}", std::process::id()));
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
            locked: false,
            tint: None,
            color: None,
            icon_size: None,
            title_center: false,
            source: None,
            tab_host: None,
        });
        cfg.rules.push(SortRule {
            id: "r1".into(),
            name: "test".into(),
            enabled: true,
            matcher: ItemMatcher::Extension("zzvtest".into()),
            target_vial: "vial-1".into(),
        });
        cfg.save(&cfg_path).unwrap();
        let pasted: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let engine = Engine::start(&cfg_path, cfg, crate::win::monitors()).expect("engine");
            std::fs::write(&rule_file, "b").unwrap();
            std::fs::write(&rule_file2, "a").unwrap();

            // Auto-sort: new desktop files that match the rule join the vial (the desktop needs a moment).
            let mut got = 0;
            for _ in 0..120 {
                engine.reconcile();
                got = engine.vial_items("vial-1").len();
                if got == 2 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            assert_eq!(got, 2, "both files match the rule");
            for _ in 0..20 {
                engine.reconcile();
                if hidden(&rule_file) && hidden(&rule_file2) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            assert!(hidden(&rule_file) && hidden(&rule_file2));

            // Sort by name: a before b, and the order is kept.
            engine.sort_vial("vial-1", "name", false).unwrap();
            let names: Vec<String> = engine.vial_items("vial-1").iter().map(|i| crate::view::display_name(i)).collect();
            assert!(names[0].contains("rule-a") && names[1].contains("rule-b"), "{names:?}");
            engine.sort_vial("vial-1", "name", true).unwrap();
            assert!(crate::view::display_name(&engine.vial_items("vial-1")[0]).contains("rule-b"));
            assert!(engine.sort_vial("vial-1", "nonsense", false).is_err());

            // Paste: a file copied to the clipboard arrives on the Desktop and in the vial.
            vials_platform::clipboard::put_files(&[source.to_string_lossy().into_owned()], false).unwrap();
            assert_eq!(engine.paste("vial-1").unwrap(), 1);
            let ids = engine.vial_items("vial-1");
            assert_eq!(ids.len(), 3, "{ids:?}");
            let new = ids.iter().find(|i| i.contains("zz-vials-paste")).expect("pasted item is in the vial");
            *pasted.lock().unwrap() = Some(PathBuf::from(new));
            assert!(Path::new(new).exists() && Path::new(new).starts_with(&desktop));

            // Vial lifecycle.
            let rect = PixelRect { left: 200, top: 200, right: 700, bottom: 600 };
            let id = engine.create_vial(rect).expect("a new vial on a monitor");
            assert_eq!(engine.vials().len(), 2);
            assert!(engine.rename_vial(&id, "Renamed"));
            engine.set_collapsed(&id, true);
            let saved = VialsConfig::load(&cfg_path).unwrap();
            let v = saved.vial(&id).unwrap();
            assert_eq!((v.name.as_str(), v.collapsed), ("Renamed", true));
            assert!(engine.delete_vial("vial-1"));
            assert!(!hidden(&rule_file), "deleting a vial gives its items back to the desktop");
            assert_eq!(engine.vials().len(), 1);
        }));
        for p in [&rule_file, &rule_file2] {
            unhide_and_delete(p);
        }
        let stem = source.file_stem().unwrap().to_string_lossy().into_owned();
        if let Ok(entries) = std::fs::read_dir(&desktop) {
            for entry in entries.flatten().filter(|e| e.file_name().to_string_lossy().starts_with(&stem)) {
                unhide_and_delete(&entry.path());
            }
        }
        if let Some(p) = pasted.lock().unwrap().take() {
            unhide_and_delete(&p);
        }
        let _ = std::fs::remove_file(&source);
        let _ = std::fs::remove_dir_all(&dir);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }
}
