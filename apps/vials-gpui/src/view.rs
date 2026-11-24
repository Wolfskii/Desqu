//! One vial's window content: header (name, collapse), then a grid of its items.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    div, img, RenderImage, prelude::*, px, rgba, App, Bounds, ClickEvent, Context, Entity, ExternalPaths, FocusHandle, FontWeight, Image,
    ImageFormat, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    Render, SharedString, Window,
};
use vials_core::model::PixelRect;
use vials_core::nav;

use crate::engine::Engine;
use crate::liquid;
use crate::menu::{self, MenuEntry};
use crate::theme::Theme;
use crate::win;

/// Bumped whenever the config changed (an item came or left a vial, a vial was added...), so every
/// vial window reloads and the set of windows is brought in line.
pub struct Revision(pub u64);

pub fn bump(revision: &Entity<Revision>, cx: &mut App) {
    revision.update(cx, |r, cx| {
        r.0 += 1;
        cx.notify();
    });
}

/// What a vial window needs to change things; absent in the read-only viewer.
#[derive(Clone)]
pub struct Live {
    pub engine: Arc<Engine>,
    pub revision: Entity<Revision>,
    pub vial_id: String,
}

#[derive(Clone)]
pub struct Item {
    pub id: String,
    pub name: SharedString,
    pub missing: bool,
    pub is_dir: bool,
}

impl Item {
    fn of(id: &str) -> Item {
        let meta = std::fs::metadata(id).ok();
        Item { name: display_name(id).into(), missing: meta.is_none(), is_dir: meta.is_some_and(|m| m.is_dir()), id: id.to_string() }
    }
}

/// Where the window sits, so collapsing can resize it in place.
pub struct Placement {
    pub rect: PixelRect,
    pub collapsed_height: i32,
}

/// Pointer movement (px) before a press turns into a drag.
const DRAG_THRESHOLD: f32 = 5.0;

/// What a folder portal shows: the folder it was made for, the one it is in now, and what that held.
struct PortalState {
    root: PathBuf,
    dir: PathBuf,
    listing: Vec<String>,
}

/// "New folder portal here...": asks for a folder, then makes a portal at `at` showing it.
pub fn new_portal(cx: &mut App, live: Live, at: Option<(i32, i32)>, rect: Option<PixelRect>) {
    cx.spawn(async move |cx| {
        let folder = cx.background_executor().spawn(async { vials_platform::actions::pick_folder("Choose the folder the portal shows").ok().flatten() }).await;
        let Some(folder) = folder else { return };
        let folder = folder.to_string_lossy().into_owned();
        let made = match rect {
            Some(rect) => live.engine.create_portal(rect, &folder),
            None => live.engine.create_portal_at(at, &folder),
        };
        if made.is_some() {
            let _ = cx.update(|cx| bump(&live.revision, cx));
        }
    })
    .detach();
}

/// The box drawn on empty space to select several items at once.
struct Marquee {
    start: Point<Pixels>,
    now: Point<Pixels>,
    /// What was selected before (kept when Ctrl is held).
    base: BTreeSet<usize>,
}

/// A press on the title: a click renames, a drag moves the window.
struct HeaderPress {
    cursor: (i32, i32),
    window: PixelRect,
    placement: PixelRect,
    moved: bool,
}

pub struct VialView {
    name: SharedString,
    items: Vec<Item>,
    icons: HashMap<String, Arc<Image>>,
    icons_requested: HashSet<String>,
    /// The item the keyboard is on, and where a Shift range starts.
    cursor: Option<usize>,
    anchor: Option<usize>,
    /// Everything the vial holds (or the portal's folder), and what the search lets through is `items`.
    all_items: Vec<Item>,
    /// The text typed into the search box; `None` while the box is closed.
    search: Option<String>,
    /// Which deep search of a portal is current, and what it found (`None`: look through `all_items`).
    search_gen: std::sync::Arc<std::sync::atomic::AtomicU64>,
    search_results: Option<Vec<String>>,
    /// A folder portal: the folder it was made for, the one it shows now, and what that held when last looked at.
    portal: Option<PortalState>,
    /// Icon size in pixels, and whether the title is centred: from the vial's own look.
    icon_size: u8,
    title_center: bool,
    /// Privacy mode is on: names and pictures are hidden until the pointer is over the vial.
    privacy_mask: bool,
    /// A collapsed vial is open for the moment because the pointer is over it.
    peeking: bool,
    /// Where the pointer is now (over the window or not), and which "left" a delayed close belongs to.
    hovered: bool,
    hover_gen: u64,
    /// The vial is locked: it cannot be moved, resized, renamed or emptied by dragging.
    locked: bool,
    selected: BTreeSet<usize>,
    marquee: Option<Marquee>,
    /// Where each item was last drawn (window coordinates): what the box is held against.
    bounds: Rc<RefCell<HashMap<usize, Bounds<Pixels>>>>,
    press: Option<(usize, Point<Pixels>)>,
    header_press: Option<HeaderPress>,
    /// The name being typed while the vial is renamed.
    editing: Option<String>,
    /// Nothing typed yet: the old name is shown selected, and typing replaces it.
    rename_fresh: bool,
    wants_keyboard: bool,
    collapsed: bool,
    placement: Placement,
    save_gen: u64,
    live: Option<Live>,
    focus: FocusHandle,
    theme: Theme,
    /// The window material has to be re-applied on the next render (it needs the window).
    material_dirty: bool,
    /// The Liquid backdrop for the window as it is now, and the bookkeeping to redraw it.
    liquid: Option<Arc<RenderImage>>,
    liquid_gen: u64,
    hwnd: Option<isize>,
}

/// Everything a vial window starts from.
pub struct ViewInit {
    pub name: String,
    pub ids: Vec<String>,
    pub collapsed: bool,
    pub placement: Placement,
    pub live: Option<Live>,
    pub theme: Theme,
}

/// What the desktop calls an item: the file name, without the extension of shortcut-like types.
pub fn display_name(id: &str) -> String {
    let p = Path::new(id);
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| id.to_string());
    match p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("lnk" | "url" | "exe") => p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or(name),
        _ => name,
    }
}

impl VialView {
    pub fn new(window: &mut Window, init: ViewInit, cx: &mut Context<Self>) -> Self {
        let ViewInit { name, ids, collapsed, placement, live, theme } = init;
        let name = name.as_str();
        // Keeps the full-size rectangle current when the user resizes the window, and (live) saves a moved
        // or resized window once it has stayed still for a moment.
        let saver = live.clone();
        cx.observe_window_bounds(window, move |this, window, cx| {
            let Some(hwnd) = win::hwnd(window) else { return };
            this.hwnd = Some(hwnd.0 as isize);
            if let Some(live) = &this.live {
                this.apply_capture(live.engine.config().hide_from_capture);
            }
            this.refresh_liquid(cx);
            // A locked vial goes back to where it was put.
            if this.locked && !this.shown_collapsed() {
                win::place_at_bottom(hwnd, &this.placement.rect);
                return;
            }
            if let Some(now) = win::window_rect(hwnd) {
                if this.shown_collapsed() {
                    this.placement.rect.left = now.left;
                    this.placement.rect.top = now.top;
                    this.placement.rect.right = now.right;
                } else {
                    this.placement.rect = now;
                }
            }
            let Some(live) = saver.clone() else { return };
            this.save_gen += 1;
            let generation = this.save_gen;
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(std::time::Duration::from_millis(500)).await;
                if this.update(cx, |view, _| view.save_gen == generation).unwrap_or(false) {
                    if let Some(rect) = win::window_rect(hwnd) {
                        live.engine.persist_rect(&live.vial_id, rect);
                    }
                }
            })
            .detach();
        })
        .detach();
        if let Some(live) = live.clone() {
            // Items that came or left (any vial window, or the desktop) show up here too.
            cx.observe(&live.revision, |this, _, cx| this.reload(cx)).detach();
            // The window only takes the keyboard while it is being used.
            cx.observe_window_activation(window, |this, window, cx| {
                if !window.is_window_active() {
                    if let Some(hwnd) = win::hwnd(window) {
                        win::release_keyboard(hwnd);
                    }
                    if this.editing.take().is_some() {
                        cx.notify();
                    }
                }
            })
            .detach();
        }
        let mut view = Self {
            name: name.to_string().into(),
            items: Vec::new(),
            icons: HashMap::new(),
            icons_requested: HashSet::new(),
            icon_size: live.as_ref().and_then(|l| l.engine.vials().into_iter().find(|v| v.id == l.vial_id)).map_or(32, |v| v.icon_px()),
            title_center: live.as_ref().and_then(|l| l.engine.vials().into_iter().find(|v| v.id == l.vial_id)).is_some_and(|v| v.title_center),
            all_items: Vec::new(),
            search: None,
            search_gen: Default::default(),
            search_results: None,
            privacy_mask: live.as_ref().is_some_and(|l| l.engine.config().privacy_mask),
            portal: None,
            cursor: None,
            anchor: None,
            peeking: false,
            hovered: false,
            hover_gen: 0,
            locked: live.as_ref().is_some_and(|l| l.engine.is_locked(&l.vial_id)),
            selected: BTreeSet::new(),
            marquee: None,
            bounds: Rc::default(),
            press: None,
            header_press: None,
            editing: None,
            rename_fresh: false,
            wants_keyboard: false,
            collapsed,
            placement,
            save_gen: 0,
            live,
            focus: cx.focus_handle(),
            theme,
            material_dirty: false,
            liquid: None,
            liquid_gen: 0,
            hwnd: None,
        };
        view.set_items(ids, cx);
        view.init_portal(cx);
        view.start_portal_watch(cx);
        view
    }

    /// A vial that shows a folder starts at that folder.
    fn init_portal(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.clone() else { return };
        let source = live.engine.vials().into_iter().find(|v| v.id == live.vial_id).and_then(|v| v.source);
        self.set_portal_source(source, cx);
    }

    /// Makes the view a portal of `source` (or an ordinary vial with `None`); nothing changes when it already is.
    fn set_portal_source(&mut self, source: Option<String>, cx: &mut Context<Self>) {
        match source {
            Some(folder) => {
                let root = PathBuf::from(folder);
                if self.portal.as_ref().map(|p| &p.root) != Some(&root) {
                    self.portal = Some(PortalState { dir: root.clone(), root, listing: Vec::new() });
                    self.selected.clear();
                    self.refresh_portal(cx);
                }
            }
            None => self.portal = None,
        }
    }

    /// Reads the folder the portal shows now (off the UI thread: a network folder can be slow).
    fn refresh_portal(&mut self, cx: &mut Context<Self>) {
        let Some(dir) = self.portal.as_ref().map(|p| p.dir.clone()) else { return };
        cx.spawn(async move |this, cx| {
            let wanted = dir.clone();
            let listing = cx.background_executor().spawn(async move { vials_core::portal::list(&dir) }).await;
            let _ = this.update(cx, |view, cx| {
                let Some(portal) = view.portal.as_mut().filter(|p| p.dir == wanted) else { return };
                match listing {
                    Ok(ids) => {
                        portal.listing = ids.clone();
                        view.set_items(ids, cx);
                    }
                    // A folder that cannot be read shows as empty.
                    Err(_) => {
                        portal.listing.clear();
                        view.set_items(Vec::new(), cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// A portal looks at its folder every couple of seconds, so files that come and go show up.
    fn start_portal_watch(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(std::time::Duration::from_secs(2)).await;
            let Ok(dir) = this.update(cx, |view, _| view.portal.as_ref().map(|p| p.dir.clone())) else { return };
            let Some(dir) = dir else { continue };
            let wanted = dir.clone();
            let listing = cx.background_executor().spawn(async move { vials_core::portal::list(&dir) }).await;
            let alive = this.update(cx, |view, cx| {
                let Some(portal) = view.portal.as_mut().filter(|p| p.dir == wanted) else { return };
                let ids = listing.unwrap_or_default();
                if portal.listing != ids {
                    portal.listing = ids.clone();
                    view.set_items(ids, cx);
                    cx.notify();
                }
            });
            if alive.is_err() {
                return;
            }
        })
        .detach();
    }

    /// Goes into a folder of the portal.
    fn portal_go(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        let Some(portal) = self.portal.as_mut() else { return };
        if !vials_core::portal::is_inside(&portal.root, &dir) {
            return;
        }
        portal.dir = dir;
        portal.listing.clear();
        self.selected.clear();
        self.cursor = None;
        self.refresh_portal(cx);
        cx.notify();
    }

    /// One folder up, but never above the folder the portal was made for.
    fn portal_up(&mut self, cx: &mut Context<Self>) {
        let Some(up) = self.portal.as_ref().and_then(|p| vials_core::portal::up(&p.root, &p.dir)) else { return };
        self.portal_go(up, cx);
    }

    /// Opens an item the way a double-click does: a folder of a portal is entered, anything else is opened.
    fn open_item(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.portal.is_some() && std::path::Path::new(id).is_dir() {
            self.portal_go(PathBuf::from(id), cx);
        } else {
            win::open_path(id);
        }
    }

    fn set_items(&mut self, ids: Vec<String>, cx: &mut Context<Self>) {
        self.all_items = ids.iter().map(|id| Item::of(id)).collect();
        self.refilter();
        let count = self.items.len();
        self.selected.retain(|i| *i < count);
        let wanted: Vec<String> = self
            .items
            .iter()
            .filter(|it| !it.missing && !self.icons.contains_key(&it.id) && !self.icons_requested.contains(&it.id))
            .map(|it| it.id.clone())
            .collect();
        self.request_icons(wanted, cx);
    }

    /// What the search lets through: the portal's deep results while there are some, else the items that fit.
    fn refilter(&mut self) {
        let query = vials_core::search::Query::parse(self.search.as_deref().unwrap_or(""));
        self.items = match (&self.search_results, query.is_empty()) {
            (_, true) => self.all_items.clone(),
            (Some(found), false) => found.iter().map(|id| Item::of(id)).collect(),
            (None, false) => self.all_items.iter().filter(|i| query.matches(&i.name, &i.id, i.is_dir)).cloned().collect(),
        };
    }

    /// The text of the search changed: show what fits, and in a portal look in the folders below too.
    fn search_changed(&mut self, cx: &mut Context<Self>) {
        self.selected.clear();
        self.cursor = None;
        let text = self.search.clone().unwrap_or_default();
        let query = vials_core::search::Query::parse(&text);
        // Any search that is still running is out of date now.
        let generation = self.search_gen.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        self.search_results = None;
        self.refilter();
        // (The recent files are only the few the list holds: they are filtered, not searched for.)
        let Some(dir) = self.portal.as_ref().filter(|p| p.root != std::path::Path::new(vials_core::portal::RECENT)).map(|p| p.dir.clone()).filter(|_| !query.is_empty()) else {
            cx.notify();
            return;
        };
        let counter = self.search_gen.clone();
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_executor()
                .spawn(async move {
                    let current = || counter.load(std::sync::atomic::Ordering::SeqCst) != generation;
                    vials_core::search::search_dir(&dir, &query, 300, 6, &current)
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if view.search_gen.load(std::sync::atomic::Ordering::SeqCst) != generation {
                    return;
                }
                view.search_results = Some(found);
                view.refilter();
                let wanted: Vec<String> = view.items.iter().filter(|it| !it.missing && !view.icons.contains_key(&it.id) && !view.icons_requested.contains(&it.id)).map(|it| it.id.clone()).collect();
                view.request_icons(wanted, cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Opens the search box (Ctrl+F).
    fn open_search(&mut self, cx: &mut Context<Self>) {
        if self.search.is_none() {
            self.search = Some(String::new());
        }
        self.wants_keyboard = true;
        cx.notify();
    }

    /// Closes the search box and shows everything again.
    fn close_search(&mut self, cx: &mut Context<Self>) {
        self.search = None;
        self.search_changed(cx);
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.clone() else { return };
        let look = live.engine.look_for(&live.vial_id);
        if look != self.theme {
            self.theme = look;
            self.material_dirty = true;
            self.refresh_liquid(cx);
        }
        if let Some(vial) = live.engine.vials().into_iter().find(|v| v.id == live.vial_id) {
            self.name = vial.name.clone().into();
            self.locked = vial.locked;
            self.icon_size = vial.icon_px();
            self.title_center = vial.title_center;
            let config = live.engine.config();
            self.privacy_mask = config.privacy_mask;
            self.apply_capture(config.hide_from_capture);
            self.set_portal_source(vial.source.clone(), cx);
            if self.portal.is_none() {
                self.set_items(vial.items, cx);
            }
        }
        cx.notify();
    }

    /// Icons come from the shell (about 6 ms each) and some handlers (.url shortcuts) only work in a
    /// single-threaded COM apartment, which GPUI's pool threads are not: one plain thread of our own
    /// fetches them, and the UI picks them up as they arrive.
    fn request_icons(&mut self, wanted: Vec<String>, cx: &mut Context<Self>) {
        if wanted.is_empty() {
            return;
        }
        self.icons_requested.extend(wanted.iter().cloned());
        let (tx, rx) = std::sync::mpsc::channel::<(String, Vec<u8>)>();
        std::thread::spawn(move || {
            for id in wanted {
                // A picture of the contents where there is one (images, videos, documents), else the icon.
                if let Ok(png) = desqu_desktop::thumbnail_png(&id, 64).or_else(|_| desqu_desktop::icon_png(&id, 64)) {
                    if tx.send((id, png)).is_err() {
                        return;
                    }
                }
            }
        });
        cx.spawn(async move |this, cx| loop {
            let mut got = Vec::new();
            let alive = loop {
                match rx.try_recv() {
                    Ok(v) => got.push(v),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break true,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => break false,
                }
            };
            if !got.is_empty() {
                let updated = this.update(cx, |view, cx| {
                    for (id, png) in got {
                        view.icons.insert(id, Arc::new(Image::from_bytes(ImageFormat::Png, png)));
                    }
                    cx.notify();
                });
                if updated.is_err() {
                    return;
                }
            }
            if !alive {
                return;
            }
            cx.background_executor().timer(std::time::Duration::from_millis(40)).await;
        })
        .detach();
    }

    /// Redraws the Liquid backdrop for where the window is now. Several moves in a row (a drag) draw
    /// once, a moment after the last one; the drawing itself runs off the UI thread.
    fn refresh_liquid(&mut self, cx: &mut Context<Self>) {
        if !self.theme.liquid {
            self.liquid = None;
            return;
        }
        let Some(hwnd) = self.hwnd else { return };
        self.liquid_gen += 1;
        let generation = self.liquid_gen;
        let tint = self.theme.tint;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(45)).await;
            if !this.update(cx, |view, _| view.liquid_gen == generation).unwrap_or(false) {
                return;
            }
            let Some(rect) = win::window_rect_of(hwnd) else { return };
            let monitors = win::monitors();
            let Some(monitor) = vials_core::layout::monitor_containing(&rect, &monitors) else { return };
            let scale = monitor.scale as f32;
            let params = liquid::Params {
                width: rect.width().max(1) as u32,
                height: rect.height().max(1) as u32,
                window: (rect.left - monitor.bounds.left, rect.top - monitor.bounds.top),
                monitor: (monitor.bounds.width().max(1) as u32, monitor.bounds.height().max(1) as u32),
                radius: 12.0 * scale,
                bezel: 26.0 * scale,
                tint,
                dpr: scale,
            };
            let rendered = cx.background_executor().spawn(async move { liquid::current().map(|wallpaper| liquid::render(&wallpaper, &params)) }).await;
            if let Some(image) = rendered {
                let data = Arc::new(RenderImage::new(vec![image::Frame::new(image)]));
                let _ = this.update(cx, |view, cx| {
                    if view.liquid_gen == generation {
                        view.liquid = Some(data);
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }

    /// Tells every vial window that the config changed.
    fn announce(&self, cx: &mut Context<Self>) {
        if let Some(live) = &self.live {
            bump(&live.revision, cx);
        }
    }

    /// Names and pictures are hidden right now: privacy mode is on and the pointer is not over the vial.
    fn masked(&self) -> bool {
        self.privacy_mask && !self.hovered
    }

    /// Keeps the window out of (or lets it into) screen captures, as the setting says.
    fn apply_capture(&self, exclude: bool) {
        if let Some(hwnd) = self.hwnd {
            win::exclude_from_capture(windows::Win32::Foundation::HWND(hwnd as *mut std::ffi::c_void), exclude);
        }
    }

    /// Whether the window is only its title bar right now.
    fn shown_collapsed(&self) -> bool {
        self.collapsed && !self.peeking
    }

    /// Sizes the window for what it shows now: just the title bar, or the whole vial.
    fn resize_window(&self, window: &Window) {
        if let Some(hwnd) = win::hwnd(window) {
            let mut r = self.placement.rect;
            if self.shown_collapsed() {
                r.bottom = r.top + self.placement.collapsed_height;
            }
            win::place_at_bottom(hwnd, &r);
        }
    }

    fn toggle_collapsed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.collapsed = !self.collapsed;
        self.peeking = false;
        self.resize_window(window);
        if let Some(live) = &self.live {
            live.engine.set_collapsed(&live.vial_id, self.collapsed);
        }
        cx.notify();
    }

    /// The pointer came over or left the window: with "open on hover" a collapsed vial opens, and
    /// closes a moment after the pointer is gone.
    fn on_hover_change(&mut self, over: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.hovered = over;
        self.hover_gen += 1;
        if self.privacy_mask {
            cx.notify();
        }
        let wanted = self.live.as_ref().is_some_and(|l| l.engine.config().hover_open);
        if !wanted || !self.collapsed {
            return;
        }
        if over {
            if !self.peeking {
                self.peeking = true;
                self.resize_window(window);
                cx.notify();
            }
            return;
        }
        let generation = self.hover_gen;
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(350)).await;
            let _ = this.update_in(cx, |view, window, cx| {
                // Only if the pointer has not come back (or the vial was opened for good) meanwhile.
                if view.hover_gen == generation && !view.hovered && view.peeking {
                    view.peeking = false;
                    view.resize_window(window);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// A click in the vial gives it the keyboard (for typing a name, Delete, Ctrl+C); it hands it back
    /// when it loses focus.
    fn take_keyboard(&mut self, window: &mut Window) {
        if self.live.is_none() {
            return;
        }
        if let Some(hwnd) = win::hwnd(window) {
            win::take_keyboard(hwnd);
        }
        window.activate_window();
        window.focus(&self.focus);
    }

    fn drop_paths(&mut self, paths: &ExternalPaths, cx: &mut Context<Self>) {
        let Some(live) = &self.live else { return };
        let paths: Vec<String> = paths.paths().iter().map(|p| p.to_string_lossy().into_owned()).collect();
        // Never work inside the drop itself: Explorer's drag loop waits for this call to return, and
        // asking Explorer for its items (or marking files) from in here deadlocks it with several
        // items. Return at once and do the work afterwards, off the UI thread.
        // A portal is a folder: what is dropped is copied into the folder it shows (Explorer's own copy, with
        // its progress and its questions).
        if let Some(dir) = self.portal.as_ref().map(|p| p.dir.to_string_lossy().into_owned()) {
            // Nothing is dropped into the list of recent files.
            if dir == vials_core::portal::RECENT {
                return;
            }
            cx.spawn(async move |this, cx| {
                let _ = cx.background_executor().spawn(async move { vials_platform::clipboard::transfer(&paths, false, &dir, false) }).await;
                let _ = this.update(cx, |view, cx| view.refresh_portal(cx));
            })
            .detach();
            return;
        }
        let (engine, vial_id) = (live.engine.clone(), live.vial_id.clone());
        cx.spawn(async move |this, cx| {
            let changed = cx.background_executor().spawn(async move { engine.drop_items(&vial_id, &paths) }).await;
            if changed {
                let _ = this.update(cx, |view, cx| view.announce(cx));
            }
        })
        .detach();
    }

    fn remove(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(live) = &self.live else { return };
        if live.engine.remove_item(id) {
            self.announce(cx);
        }
    }

    /// The native Windows drag of an item out of the vial. It is modal, so it runs from a task, never
    /// from inside an event handler (GPUI is borrowed there and the drag pumps messages).
    fn start_drag(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(live) = self.live.clone() else { return };
        if self.locked {
            return;
        }
        let Some(item) = self.items.get(index) else { return };
        if item.missing {
            return;
        }
        // Dragging one of several selected items takes them all.
        let ids: Vec<String> = if self.selected.contains(&index) && self.selected.len() > 1 {
            self.selected.iter().filter_map(|i| self.items.get(*i)).filter(|it| !it.missing).map(|it| it.id.clone()).collect()
        } else {
            vec![item.id.clone()]
        };
        let source = live.engine.vial_id_of(&item.id);
        let portal = self.portal.is_some();
        cx.spawn(async move |this, cx| {
            let end = vials_platform::drag_items(&ids);
            if portal {
                // Files of a folder stay in it; let go on the empty desktop and they are copied there.
                if let Ok(vials_platform::DragEnd::OnDesktop(_)) = end {
                    if let Some(desktop) = desqu_desktop::desktop_folders().first().map(|p| p.to_string_lossy().into_owned()) {
                        let _ = cx.background_executor().spawn(async move { vials_platform::clipboard::transfer(&ids, false, &desktop, true) }).await;
                    }
                }
                let _ = this.update(cx, |view, cx| view.refresh_portal(cx));
                return;
            }
            if let Ok(end) = end {
                let changed = live.engine.settle_drag(&ids, source.as_deref(), end);
                if changed {
                    let _ = this.update(cx, |view, cx| view.announce(cx));
                }
            }
        })
        .detach();
    }

    /// Explorer's own right-click menu for the item (modal, so from a task like the drag).
    fn item_menu(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(live) = &self.live else { return };
        let Some(item) = self.items.get(index) else { return };
        if item.missing || (self.portal.is_none() && !live.engine.is_assigned(&item.id)) {
            return;
        }
        let id = item.id.clone();
        cx.spawn(async move |_, _| {
            let _ = vials_platform::show_item_menu(&id);
        })
        .detach();
    }

    /// Ctrl+V: files from the clipboard come to the Desktop and into this vial. The file operation
    /// blocks, so it runs off the UI thread.
    fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.clone() else { return };
        cx.spawn(async move |this, cx| {
            let engine = live.engine.clone();
            let vial = live.vial_id.clone();
            let result = cx.background_executor().spawn(async move { engine.paste(&vial) }).await;
            if matches!(result, Ok(n) if n > 0) {
                let _ = this.update(cx, |view, cx| view.announce(cx));
            }
        })
        .detach();
    }

    fn begin_rename(&mut self, cx: &mut Context<Self>) {
        if self.live.is_none() || self.locked {
            return;
        }
        self.editing = Some(self.name.to_string());
        self.rename_fresh = true;
        self.wants_keyboard = true;
        cx.notify();
    }

    fn finish_rename(&mut self, save: bool, cx: &mut Context<Self>) {
        let Some(text) = self.editing.take() else { return };
        if save {
            if let Some(live) = &self.live {
                if live.engine.rename_vial(&live.vial_id, &text) {
                    self.announce(cx);
                }
            }
        }
        cx.notify();
    }

    /// Saved layouts: where all the vials sit, to bring back later.
    fn open_layouts_menu(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.clone() else { return };
        let Some(at) = win::cursor_position() else { return };
        let mut entries = Vec::new();
        {
            let live = live.clone();
            entries.push(MenuEntry::new("Save the layout now", move |cx| {
                live.engine.save_layout("");
                bump(&live.revision, cx);
            }));
        }
        let names = live.engine.layout_names();
        if !names.is_empty() {
            entries.push(MenuEntry::separator());
        }
        for name in &names {
            let (live, name) = (live.clone(), name.clone());
            entries.push(MenuEntry::new(format!("Bring back {name}"), move |cx| {
                live.engine.restore_layout(&name);
                bump(&live.revision, cx);
            }));
        }
        if !names.is_empty() {
            entries.push(MenuEntry::separator());
        }
        for name in names {
            let live = live.clone();
            entries.push(MenuEntry::new(format!("Delete {name}"), move |cx| {
                live.engine.delete_layout(&name);
                bump(&live.revision, cx);
            }));
        }
        menu::open(cx, at, entries);
    }

    /// The vial's own look: icon size, colour, how see-through, where the title sits.
    fn open_look_menu(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.clone() else { return };
        let Some(vial) = live.engine.vials().into_iter().find(|v| v.id == live.vial_id) else { return };
        let Some(at) = win::cursor_position() else { return };
        // One entry that changes the vial and tells every window.
        let entry = |label: String, checked: bool, change: Box<dyn Fn(&mut vials_core::model::Vial)>| {
            let live = live.clone();
            MenuEntry::new(label, move |cx| {
                live.engine.set_look(&live.vial_id, |v| change(v));
                bump(&live.revision, cx);
            })
            .checked(checked)
        };
        let mut entries = Vec::new();
        for (size, label) in [(24u8, "Small icons"), (32, "Medium icons"), (48, "Large icons")] {
            entries.push(entry(label.to_string(), vial.icon_px() == size, Box::new(move |v| v.icon_size = Some(size))));
        }
        entries.push(MenuEntry::separator());
        entries.push(entry("Default colour".into(), vial.color.is_none(), Box::new(|v| v.color = None)));
        for (name, hex) in vials_core::model::PANEL_COLORS {
            entries.push(entry(name.to_string(), vial.color.as_deref() == Some(hex), Box::new(move |v| v.color = Some(hex.to_string()))));
        }
        entries.push(MenuEntry::separator());
        for (label, tint) in [("Default see-through", None), ("Very clear", Some(5u8)), ("Frosted", Some(45)), ("Solid", Some(90))] {
            entries.push(entry(label.to_string(), vial.tint == tint, Box::new(move |v| v.tint = tint)));
        }
        entries.push(MenuEntry::separator());
        entries.push(entry("Title in the middle".into(), vial.title_center, Box::new({
            let on = !vial.title_center;
            move |v| v.title_center = on
        })));
        menu::open(cx, at, entries);
    }

    /// The vial's own menu: rename, sort, folders first, new vial, delete.
    fn open_vial_menu(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.clone() else { return };
        let Some(vial) = live.engine.vials().into_iter().find(|v| v.id == live.vial_id) else { return };
        let Some(at) = win::cursor_position() else { return };
        let weak = cx.weak_entity();

        let rename = {
            let weak = weak.clone();
            MenuEntry::new("Rename", move |cx| {
                let _ = weak.update(cx, |view, cx| view.begin_rename(cx));
            })
        };
        let mut entries = vec![rename, MenuEntry::separator()];
        for (key, label) in [("name", "Sort by name"), ("type", "Sort by type"), ("modified", "Sort by date modified"), ("size", "Sort by size")].into_iter().filter(|_| vial.source.is_none()) {
            let on = vial.sort_key.as_deref() == Some(key);
            let descending = on && !vial.sort_desc; // choosing the active sort again reverses it
            let live = live.clone();
            let label = if on { format!("{label} ({})", if vial.sort_desc { "descending" } else { "ascending" }) } else { label.to_string() };
            entries.push(
                MenuEntry::new(label, move |cx| {
                    let _ = live.engine.sort_vial(&live.vial_id, key, descending);
                    bump(&live.revision, cx);
                })
                .checked(on),
            );
        }
        {
            let w = weak.clone();
            entries.push(MenuEntry::new("Search...", move |cx| {
                let _ = w.update(cx, |view, cx| view.open_search(cx));
            }));
        }
        if vial.source.is_none() {
            let live = live.clone();
            let on = !vial.folders_first;
            entries.push(
                MenuEntry::new("Folders first", move |cx| {
                    live.engine.set_folders_first(&live.vial_id, on);
                    bump(&live.revision, cx);
                })
                .checked(vial.folders_first),
            );
        }
        {
            let live = live.clone();
            let on = !vial.locked;
            entries.push(
                MenuEntry::new("Lock", move |cx| {
                    live.engine.set_locked(&live.vial_id, on);
                    bump(&live.revision, cx);
                })
                .checked(vial.locked),
            );
        }
        {
            let weak = weak.clone();
            entries.push(MenuEntry::new("Look...", move |cx| {
                let _ = weak.update(cx, |view, cx| view.open_look_menu(cx));
            }));
        }
        {
            let weak = weak.clone();
            entries.push(MenuEntry::new("Layouts...", move |cx| {
                let _ = weak.update(cx, |view, cx| view.open_layouts_menu(cx));
            }));
        }
        if !self.items.is_empty() {
            let w = weak.clone();
            entries.push(MenuEntry::new("Open all", move |cx| {
                let _ = w.update(cx, |view, _| view.open_all());
            }));
            let w = weak.clone();
            entries.push(MenuEntry::new("Copy paths", move |cx| {
                let _ = w.update(cx, |view, _| view.copy_paths());
            }));
            let w = weak.clone();
            entries.push(MenuEntry::new("Zip to the desktop", move |cx| {
                let _ = w.update(cx, |view, cx| view.zip_items(cx));
            }));
            if !vial.locked {
                let w = weak.clone();
                entries.push(MenuEntry::new("Move to folder...", move |cx| {
                    let _ = w.update(cx, |view, cx| view.move_items(cx));
                }));
            }
            entries.push(MenuEntry::separator());
        }
        if live.engine.can_undo() {
            let weak = weak.clone();
            entries.push(MenuEntry::new("Undo", move |cx| {
                let _ = weak.update(cx, |view, cx| view.undo(cx));
            }));
        }
        entries.push(MenuEntry::separator());
        // While Workspaces runs: which virtual desktop the vial shows on (the current one is checked).
        if let Some(choices) = crate::desktops::menu_choices(&live.vial_id) {
            for (label, on, target) in choices {
                let vial = live.vial_id.clone();
                entries.push(
                    MenuEntry::new(label, move |_| {
                        if !crate::desktops::assign(&vial, &target) {
                            eprintln!("Workspaces did not take the change");
                        }
                    })
                    .checked(on),
                );
            }
            entries.push(MenuEntry::separator());
        }
        if let Some(source) = vial.source.clone() {
            if vials_core::notes::is_notes(&source) {
                let w = weak.clone();
                entries.push(MenuEntry::new("New note", move |cx| {
                    let _ = w.update(cx, |view, cx| view.new_note(cx));
                }));
                if vials_platform::actions::get_text().is_some_and(|t| vials_core::notes::looks_like_url(&t)) {
                    let w = weak.clone();
                    entries.push(MenuEntry::new("New link from the clipboard", move |cx| {
                        let _ = w.update(cx, |view, cx| view.new_link(cx));
                    }));
                }
            }
            entries.push(MenuEntry::new("Open the folder in Explorer", move |_| win::open_path(&source)));
        }
        {
            let live = live.clone();
            entries.push(MenuEntry::new("New notes vial here", move |cx| {
                if live.engine.create_notes_at(Some(at)).is_some() {
                    bump(&live.revision, cx);
                }
            }));
        }
        {
            let live = live.clone();
            entries.push(MenuEntry::new("New folder portal here...", move |cx| new_portal(cx, live.clone(), Some(at), None)));
        }
        {
            let live = live.clone();
            entries.push(MenuEntry::new("New archive vial here (not changed for 30 days)", move |cx| {
                if live.engine.create_archive_at(Some(at), 30).is_some() {
                    bump(&live.revision, cx);
                }
            }));
        }
        {
            let live = live.clone();
            entries.push(MenuEntry::new("New recent files vial here", move |cx| {
                if live.engine.create_portal_at(Some(at), vials_core::portal::RECENT).is_some() {
                    bump(&live.revision, cx);
                }
            }));
        }
        {
            let live = live.clone();
            entries.push(MenuEntry::new("New vial here", move |cx| {
                if live.engine.create_vial_at(Some(at)).is_some() {
                    bump(&live.revision, cx);
                }
            }));
        }
        {
            let live = live.clone();
            entries.push(MenuEntry::new("Rules", move |cx| crate::rules_window::open(cx, live.engine.clone(), live.revision.clone())));
        }
        {
            let live = live.clone();
            entries.push(MenuEntry::new("Settings", move |cx| crate::settings::open(cx, live.engine.clone(), live.revision.clone())));
        }
        {
            let live = live.clone();
            entries.push(MenuEntry::new("Delete vial", move |cx| {
                if live.engine.delete_vial(&live.vial_id) {
                    bump(&live.revision, cx);
                }
            }));
        }
        menu::open(cx, at, entries);
    }

    fn on_key(&mut self, e: &KeyDownEvent, cx: &mut Context<Self>) {
        let k = &e.keystroke;
        if let Some(text) = self.editing.as_mut() {
            match k.key.as_str() {
                "enter" => self.finish_rename(true, cx),
                "escape" => self.finish_rename(false, cx),
                "backspace" => {
                    if std::mem::take(&mut self.rename_fresh) {
                        text.clear();
                    } else {
                        text.pop();
                    }
                    cx.notify();
                }
                _ => {
                    if let Some(typed) = k.key_char.as_ref().filter(|_| !k.modifiers.control && !k.modifiers.alt) {
                        if std::mem::take(&mut self.rename_fresh) {
                            text.clear();
                        }
                        if text.chars().count() < 80 {
                            text.push_str(typed);
                            cx.notify();
                        }
                    }
                }
            }
            return;
        }
        if k.modifiers.control && k.key == "f" {
            self.open_search(cx);
            return;
        }
        // While the search box is open the keys that make text go into it.
        if let Some(text) = self.search.as_mut() {
            match k.key.as_str() {
                "escape" => {
                    self.close_search(cx);
                    return;
                }
                "backspace" => {
                    text.pop();
                    self.search_changed(cx);
                    return;
                }
                _ => {
                    if let Some(typed) = k.key_char.as_ref().filter(|_| !k.modifiers.control && !k.modifiers.alt) {
                        if text.chars().count() < 80 {
                            text.push_str(typed);
                            self.search_changed(cx);
                        }
                        return;
                    }
                }
            }
        }
        if k.modifiers.control && k.key == "v" {
            self.paste(cx);
            return;
        }
        if k.modifiers.control && k.key == "z" {
            self.undo(cx);
            return;
        }
        if k.modifiers.control && k.key == "a" {
            self.selected = (0..self.items.len()).collect();
            cx.notify();
            return;
        }
        let dir = match k.key.as_str() {
            "left" => Some(nav::Dir::Left),
            "right" => Some(nav::Dir::Right),
            "up" => Some(nav::Dir::Up),
            "down" => Some(nav::Dir::Down),
            "home" => Some(nav::Dir::Home),
            "end" => Some(nav::Dir::End),
            _ => None,
        };
        if let Some(dir) = dir {
            self.move_cursor(dir, k.modifiers.shift, cx);
            return;
        }
        match k.key.as_str() {
            "escape" => {
                self.selected.clear();
                self.cursor = None;
                cx.notify();
                return;
            }
            "f2" => {
                self.begin_rename(cx);
                return;
            }
            // Backspace goes up a folder in a portal, like Explorer.
            "backspace" if self.portal.is_some() => {
                self.portal_up(cx);
                return;
            }
            // Enter opens what is selected, like Explorer.
            "enter" => {
                for id in self.selected_ids() {
                    self.open_item(&id, cx);
                }
                return;
            }
            // The menu key, or Shift+F10: the item menu of the item the keyboard is on.
            "menu" | "f10" if k.key == "menu" || k.modifiers.shift => {
                if let Some(i) = self.cursor.filter(|i| *i < self.items.len()) {
                    self.item_menu(i, cx);
                }
                return;
            }
            _ => {}
        }
        let ids = self.selected_ids();
        if ids.is_empty() {
            return;
        }
        if k.key == "delete" {
            for id in &ids {
                self.remove(id, cx);
            }
        } else if k.modifiers.control && (k.key == "c" || k.key == "x") {
            let _ = vials_platform::clipboard::put_files(&ids, k.key == "x");
        }
    }

    /// The vial's items that exist (a path that is gone cannot be opened, copied or packed).
    fn existing_items(&self) -> Vec<String> {
        self.items.iter().filter(|i| !i.missing && !i.id.starts_with("::")).map(|i| i.id.clone()).collect()
    }

    /// A new note in the notes vial: an empty text file, opened to write in.
    fn new_note(&mut self, cx: &mut Context<Self>) {
        let Some(dir) = self.portal.as_ref().map(|p| p.dir.clone()) else { return };
        let name = vials_core::clip::unique_name(|n| dir.join(n).exists(), "Note", "txt");
        let path = dir.join(name);
        if std::fs::write(&path, "").is_ok() {
            #[cfg(not(test))]
            win::open_path(&path.to_string_lossy());
            self.refresh_portal(cx);
        }
    }

    /// A new link in the notes vial: a `.url` file for the web address on the clipboard.
    fn new_link(&mut self, cx: &mut Context<Self>) {
        let Some(dir) = self.portal.as_ref().map(|p| p.dir.clone()) else { return };
        let Some(url) = vials_platform::actions::get_text().filter(|t| vials_core::notes::looks_like_url(t)) else { return };
        let name = vials_core::clip::unique_name(|n| dir.join(n).exists(), &vials_core::notes::link_name(&url), "url");
        if std::fs::write(dir.join(name), vials_core::notes::url_file_text(&url)).is_ok() {
            self.refresh_portal(cx);
        }
    }

    /// "Open all": every item, like double-clicking each (at most 20 at once).
    fn open_all(&self) {
        for id in self.existing_items().into_iter().take(20) {
            win::open_path(&id);
        }
    }

    /// "Copy paths": one path per line, as text.
    fn copy_paths(&self) {
        if let Err(e) = vials_platform::actions::put_text(&self.existing_items().join("\r\n")) {
            eprintln!("could not copy the paths: {e}");
        }
    }

    /// "Zip": the items packed into `<vial name>.zip` on the desktop.
    fn zip_items(&mut self, cx: &mut Context<Self>) {
        let items = self.existing_items();
        let Some(folder) = desqu_desktop::desktop_folders().into_iter().next() else { return };
        let stem = vials_core::clip::file_safe(&self.name);
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let name = vials_core::clip::unique_name(|n| folder.join(n).exists(), &stem, "zip");
                    vials_platform::actions::zip(&items, &folder.join(name))
                })
                .await;
            if let Err(e) = result {
                eprintln!("could not zip the items: {e}");
            }
        })
        .detach();
    }

    /// "Move to folder...": pick a folder, move the files there with Explorer's own file operation (its
    /// progress and conflict questions), and take what moved out of the vial.
    fn move_items(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.clone() else { return };
        let items = self.existing_items();
        let title = format!("Move the items of {} to", self.name);
        cx.spawn(async move |this, cx| {
            let moved = cx
                .background_executor()
                .spawn({
                    let items = items.clone();
                    async move {
                        let Some(folder) = vials_platform::actions::pick_folder(&title).ok().flatten() else { return false };
                        let target = folder.to_string_lossy().into_owned();
                        vials_platform::clipboard::transfer(&items, true, &target, false).is_ok()
                    }
                })
                .await;
            if moved {
                // What is no longer on the desktop leaves the vial.
                for id in items.iter().filter(|id| !std::path::Path::new(id).exists()) {
                    live.engine.remove_item(id);
                }
                let _ = this.update(cx, |view, cx| view.announce(cx));
            }
        })
        .detach();
    }

    /// Takes back the last change (any vial's).
    fn undo(&mut self, cx: &mut Context<Self>) {
        let Some(live) = &self.live else { return };
        if live.engine.undo() {
            self.announce(cx);
        }
    }

    /// Moves the keyboard to another item; with Shift the selection grows from where it started.
    fn move_cursor(&mut self, dir: nav::Dir, extend: bool, cx: &mut Context<Self>) {
        let bounds = self.bounds.borrow();
        // Where each item was drawn, in order: an item that has not been drawn yet counts as the origin.
        let places: Vec<(f32, f32)> = (0..self.items.len())
            .map(|i| bounds.get(&i).map_or((0.0, 0.0), |b| (f32::from(b.origin.x), f32::from(b.origin.y))))
            .collect();
        drop(bounds);
        let from = self.cursor.or_else(|| self.selected.iter().next_back().copied());
        let Some(to) = nav::step(&places, from, dir) else { return };
        self.cursor = Some(to);
        if extend {
            let anchor = *self.anchor.get_or_insert(from.unwrap_or(to));
            self.selected = nav::range(anchor, to).collect();
        } else {
            self.anchor = Some(to);
            self.selected = BTreeSet::from([to]);
        }
        cx.notify();
    }

    /// The ids of the selected items, in the order they are shown.
    fn selected_ids(&self) -> Vec<String> {
        self.selected.iter().filter_map(|i| self.items.get(*i)).map(|it| it.id.clone()).collect()
    }

    fn on_mouse_move(&mut self, e: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        let left = e.pressed_button == Some(MouseButton::Left);
        if let Some(marquee) = self.marquee.as_mut() {
            if !left {
                self.marquee = None;
            } else {
                marquee.now = e.position;
                // Dragged up or to the left, the corners are the wrong way round: take the real top-left.
                let top_left = gpui::point(marquee.start.x.min(marquee.now.x), marquee.start.y.min(marquee.now.y));
                let bottom_right = gpui::point(marquee.start.x.max(marquee.now.x), marquee.start.y.max(marquee.now.y));
                let rect = Bounds::from_corners(top_left, bottom_right);
                let mut chosen = marquee.base.clone();
                chosen.extend(self.bounds.borrow().iter().filter(|(_, b)| b.intersects(&rect)).map(|(i, _)| *i));
                self.selected = chosen;
            }
            cx.notify();
            return;
        }
        if let Some(press) = self.header_press.as_mut() {
            if !left {
                self.header_press = None;
            } else if let (Some(now), Some(hwnd)) = (win::cursor_position(), win::hwnd(window)) {
                let (dx, dy) = (now.0 - press.cursor.0, now.1 - press.cursor.1);
                if press.moved || ((dx * dx + dy * dy) as f32).sqrt() > DRAG_THRESHOLD {
                    press.moved = true;
                    let mut r = press.window;
                    r.left += dx;
                    r.right += dx;
                    r.top += dy;
                    r.bottom += dy;
                    win::place_at_bottom(hwnd, &r);
                }
            }
            return;
        }
        if let Some((index, start)) = self.press {
            if left {
                let (dx, dy) = (f32::from(e.position.x - start.x), f32::from(e.position.y - start.y));
                if (dx * dx + dy * dy).sqrt() > DRAG_THRESHOLD {
                    self.press = None;
                    self.start_drag(index, cx);
                }
            } else {
                self.press = None;
            }
        }
    }
}

impl Render for VialView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.wants_keyboard {
            self.wants_keyboard = false;
            self.take_keyboard(window);
        }
        if std::mem::take(&mut self.material_dirty) {
            window.set_background_appearance(self.theme.material);
        }
        let t = self.theme;
        let chevron = div()
            .id("collapse")
            .w(px(30.))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .text_xs()
            .hover(move |s| s.bg(rgba(t.hover)))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.toggle_collapsed(window, cx)))
            .child(if self.shown_collapsed() { "v" } else { "^" });

        let title = match &self.editing {
            Some(text) => div()
                .px_2()
                .h(px(24.))
                .flex()
                .items_center()
                .rounded(px(4.))
                .border_1()
                .border_color(rgba(0xaad2ffd9))
                .bg(rgba(0x00000040))
                .child(if self.rename_fresh { div().px_1().rounded(px(2.)).bg(rgba(0x78afff80)).child(text.clone()).into_any_element() } else { div().child(format!("{text}|")).into_any_element() })
                .into_any_element(),
            None => div().child(self.name.clone()).into_any_element(),
        };
        let can_up = self.portal.as_ref().is_some_and(|p| vials_core::portal::up(&p.root, &p.dir).is_some());
        let header = div()
            .id("header")
            .h(px(34.))
            .flex_none()
            .flex()
            .items_center()
            .border_b_1()
            .border_color(rgba(t.line))
            // Inside a folder of a portal: back to the folder above.
            .when(can_up, |d| {
                d.child(
                    div()
                        .id("portal-up")
                        .w(px(30.))
                        .h_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .hover(move |s| s.bg(rgba(t.hover)))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.portal_up(cx)))
                        .child("<"),
                )
            })
            .child(
                // Press and drag moves the window, a plain click renames it, right click opens the menu.
                div()
                    .id("title")
                    .flex_1()
                    .h_full()
                    .px_3()
                    .flex()
                    .items_center()
                    .when(self.title_center, |d| d.justify_center())
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _: &MouseDownEvent, window, _| {
                            if this.editing.is_some() || this.locked {
                                return;
                            }
                            if let (Some(cursor), Some(hwnd)) = (win::cursor_position(), win::hwnd(window)) {
                                if let Some(rect) = win::window_rect(hwnd) {
                                    this.header_press = Some(HeaderPress { cursor, window: rect, placement: this.placement.rect, moved: false });
                                }
                            }
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _: &MouseUpEvent, _, cx| {
                            if let Some(press) = this.header_press.take() {
                                if press.moved {
                                    // Keep the full-size rectangle where the drag ended.
                                    let now = win::cursor_position().unwrap_or(press.cursor);
                                    let (dx, dy) = (now.0 - press.cursor.0, now.1 - press.cursor.1);
                                    let mut r = press.placement;
                                    r.left += dx;
                                    r.right += dx;
                                    r.top += dy;
                                    r.bottom += dy;
                                    this.placement.rect = r;
                                } else {
                                    this.begin_rename(cx);
                                }
                            }
                        }),
                    )
                    .on_mouse_down(MouseButton::Right, cx.listener(|this, _: &MouseDownEvent, _, cx| this.open_vial_menu(cx)))
                    .child(title),
            )
            .child(chevron);

        let selected = self.selected.clone();
        let bounds = self.bounds.clone();
        let live = self.live.is_some();
        let is_portal = self.portal.is_some();
        let icon_px = f32::from(self.icon_size);
        // The icon plus room for a two-line name on each side.
        let item_w = icon_px + 52.0;
        let grid = div()
            // Where the items were drawn (window coordinates, one per child): what the box is held against.
            .on_children_prepainted(move |all, _, _| {
                *bounds.borrow_mut() = all.into_iter().enumerate().collect();
            })
            .id("grid")
            .flex_1()
            .min_h_0()
            .p_2()
            .flex()
            .flex_wrap()
            .content_start()
            .gap_1()
            .on_mouse_down(MouseButton::Right, cx.listener(|this, _: &MouseDownEvent, _, cx| this.open_vial_menu(cx)))
            // Press on empty space and drag: a box that selects the items it touches (Ctrl adds to the selection).
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    if this.bounds.borrow().values().any(|b| b.contains(&e.position)) {
                        return;
                    }
                    this.take_keyboard(window);
                    let base = if e.modifiers.control { this.selected.clone() } else { BTreeSet::new() };
                    this.selected = base.clone();
                    this.marquee = Some(Marquee { start: e.position, now: e.position, base });
                    cx.notify();
                }),
            )
            .children(self.items.iter().enumerate().map(|(i, item)| {
                let is_selected = selected.contains(&i);
                let id = item.id.clone();
                let masked = self.masked();
                let icon = match self.icons.get(&item.id).filter(|_| !masked) {
                    Some(image) => img(image.clone()).size(px(icon_px)).object_fit(gpui::ObjectFit::Contain).into_any_element(),
                    None => div().size(px(icon_px)).rounded(px(6.)).bg(rgba(t.placeholder)).into_any_element(),
                };
                let remove_id = item.id.clone();
                div()
                    .id(("item", i))
                    .group("item")
                    .relative()
                    .w(px(item_w))
                    .py_2()
                    .px_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(rgba(0x00000000))
                    .when(is_selected, move |d| d.bg(rgba(t.selected)).border_color(rgba(t.selected_border)))
                    .when(!is_selected, move |d| d.hover(move |s| s.bg(rgba(t.hover)).border_color(rgba(t.hover_border))))
                    .when(item.missing, |d| d.opacity(0.5))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                            if e.modifiers.control {
                                if !this.selected.remove(&i) {
                                    this.selected.insert(i);
                                }
                            } else if !this.selected.contains(&i) {
                                // Already part of a selection: keep it, so the group can be dragged.
                                this.selected = BTreeSet::from([i]);
                            }
                            this.press = Some((i, e.position));
                            this.take_keyboard(window);
                            cx.notify();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                            if !this.selected.contains(&i) {
                                this.selected = BTreeSet::from([i]);
                            }
                            this.item_menu(i, cx);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                        if e.click_count() >= 2 {
                            this.open_item(&id, cx);
                        } else if !e.modifiers().control && this.selected.len() > 1 {
                            // A plain click on one of several selected items narrows it to that one.
                            this.selected = BTreeSet::from([i]);
                            cx.notify();
                        }
                    }))
                    .child(icon)
                    .child(div().text_xs().max_w_full().h(px(34.)).overflow_hidden().text_center().child(if masked { SharedString::from("\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}") } else { item.name.clone() }))
                    .when(live && !is_portal, |d| {
                        d.child(
                            div()
                                .id(("remove", i))
                                .absolute()
                                .top_0()
                                .right_0()
                                .size(px(16.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(8.))
                                .bg(rgba(0x000000aa))
                                .text_xs()
                                .invisible()
                                .group_hover("item", |s| s.visible())
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.remove(&remove_id, cx)))
                                .child("x"),
                        )
                    })
            }));

        div()
            .id("vial")
            .track_focus(&self.focus)
            .on_hover(cx.listener(|this, over: &bool, window, cx| this.on_hover_change(*over, window, cx)))
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| this.on_key(e, cx)))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, cx| this.on_mouse_move(e, window, cx)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    this.press = None;
                    if this.marquee.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| this.drop_paths(paths, cx)))
            .drag_over::<ExternalPaths>(|s, _, _, _| s.border_color(rgba(0xaad2ffd9)).bg(rgba(0x78afff30)))
            .size_full()
            .rounded(px(t.radius))
            .border_1()
            .border_color(rgba(t.border))
            .bg(rgba(t.panel))
            .text_color(rgba(t.text))
            .flex()
            .flex_col()
            .relative()
            .when_some(self.liquid.clone().filter(|_| t.liquid), |d, image| d.child(img(image).absolute().top_0().left_0().size_full()))
            .child(header)
            .when_some(self.search.clone(), |d, text| {
                let shown = self.items.len();
                d.child(
                    div()
                        .id("search")
                        .h(px(26.))
                        .flex_none()
                        .px_3()
                        .flex()
                        .items_center()
                        .gap_2()
                        .border_b_1()
                        .border_color(rgba(t.line))
                        .text_xs()
                        .child(div().text_color(rgba(0xffffff90)).child("Search"))
                        .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().child(if text.is_empty() { "type to find: words, .pdf, kind:images".to_string() } else { format!("{text}|") }))
                        .child(div().text_color(rgba(0xffffff90)).child(format!("{shown} found")))
                        .child(div().id("search-close").px_1().cursor_pointer().hover(move |s| s.bg(rgba(t.hover))).child("x").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close_search(cx)))),
                )
            })
            .when(!self.shown_collapsed(), |d| d.child(grid))
            .when_some(self.marquee.as_ref(), |d, m| {
                let (left, top) = (f32::from(m.start.x.min(m.now.x)), f32::from(m.start.y.min(m.now.y)));
                let (width, height) = (f32::from((m.start.x - m.now.x).abs()), f32::from((m.start.y - m.now.y).abs()));
                d.child(div().absolute().left(px(left)).top(px(top)).w(px(width)).h(px(height)).border_1().border_color(rgba(t.selected_border)).bg(rgba(t.selected)))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, Modifiers, TestAppContext};

    fn init() -> ViewInit {
        ViewInit {
            name: "Test".into(),
            ids: (0..4).map(|i| format!("C:/no/such/item{i}.txt")).collect(),
            collapsed: false,
            placement: Placement { rect: PixelRect { left: 0, top: 0, right: 400, bottom: 240 }, collapsed_height: 40 },
            live: None,
            theme: Theme::of(vials_core::model::VialTheme::Classic, 50),
        }
    }

    #[gpui::test]
    fn dragging_a_box_over_items_selects_them(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| VialView::new(window, init(), cx));
        cx.run_until_parked();
        let bounds = view.read_with(cx, |v, _| v.bounds.borrow().clone());
        assert_eq!(bounds.len(), 4, "every item reports where it is drawn: {bounds:?}");
        let first = bounds[&0];
        let last = bounds[&3];
        // From empty space below the items, up over the first and last one.
        let start = point(last.right() + px(2.), last.bottom() + px(20.));
        let end = point(first.left() + px(4.), first.top() + px(4.));
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        assert!(view.read_with(cx, |v, _| v.marquee.is_some()), "the box started");
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
        let selected = view.read_with(cx, |v, _| v.selected.iter().copied().collect::<Vec<_>>());
        assert_eq!(selected, vec![0, 1, 2, 3]);
    }

    fn press(view: &Entity<VialView>, cx: &mut gpui::VisualTestContext, keys: &str) {
        let key = gpui::KeyDownEvent { keystroke: gpui::Keystroke::parse(keys).unwrap(), is_held: false };
        view.update(cx, |v, cx| v.on_key(&key, cx));
    }

    #[gpui::test]
    fn a_portal_shows_its_folder_goes_into_subfolders_and_never_above_its_root(cx: &mut TestAppContext) {
        let root = std::env::temp_dir().join(format!("desqu-portal-view-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("a.txt"), "a").unwrap();
        std::fs::write(root.join("sub").join("inner.txt"), "i").unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| VialView::new(window, init(), cx));
        let names = |view: &Entity<VialView>, cx: &mut gpui::VisualTestContext| view.read_with(cx, |v, _| v.items.iter().map(|i| i.name.to_string()).collect::<Vec<_>>());
        view.update(cx, |v, cx| v.set_portal_source(Some(root.to_string_lossy().into_owned()), cx));
        cx.run_until_parked();
        assert_eq!(names(&view, cx), vec!["sub", "a.txt"], "folders first");

        view.update(cx, |v, cx| v.open_item(&root.join("sub").to_string_lossy(), cx));
        cx.run_until_parked();
        assert_eq!(names(&view, cx), vec!["inner.txt"], "inside the folder");

        view.update(cx, |v, cx| v.portal_up(cx));
        cx.run_until_parked();
        assert_eq!(names(&view, cx), vec!["sub", "a.txt"], "back at the top");
        view.update(cx, |v, cx| v.portal_up(cx));
        cx.run_until_parked();
        assert_eq!(names(&view, cx), vec!["sub", "a.txt"], "no way above the folder of the portal");

        // Something outside the folder is not entered either.
        view.update(cx, |v, cx| v.portal_go(std::env::temp_dir(), cx));
        cx.run_until_parked();
        assert_eq!(names(&view, cx), vec!["sub", "a.txt"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    fn type_text(view: &Entity<VialView>, cx: &mut gpui::VisualTestContext, text: &str) {
        for c in text.chars() {
            let keystroke = gpui::Keystroke { modifiers: Modifiers::none(), key: c.to_string(), key_char: Some(c.to_string()) };
            let key = gpui::KeyDownEvent { keystroke, is_held: false };
            view.update(cx, |v, cx| v.on_key(&key, cx));
        }
    }

    #[gpui::test]
    fn the_search_filters_a_vial_and_a_portal_looks_in_the_folders_below_too(cx: &mut TestAppContext) {
        let root = std::env::temp_dir().join(format!("desqu-search-view-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("sub")).unwrap();
        for f in ["report.pdf", "notes.txt", "sub/report-old.pdf"] {
            std::fs::write(root.join(f), "x").unwrap();
        }
        let (view, cx) = cx.add_window_view(|window, cx| VialView::new(window, init(), cx));
        let names = |view: &Entity<VialView>, cx: &mut gpui::VisualTestContext| {
            let mut v = view.read_with(cx, |v, _| v.items.iter().map(|i| i.name.to_string()).collect::<Vec<_>>());
            v.sort();
            v
        };
        view.update(cx, |v, cx| v.set_portal_source(Some(root.to_string_lossy().into_owned()), cx));
        cx.run_until_parked();
        assert_eq!(names(&view, cx), vec!["notes.txt", "report.pdf", "sub"]);

        press(&view, cx, "ctrl-f");
        type_text(&view, cx, "report");
        assert_eq!(names(&view, cx), vec!["report.pdf"], "the folder's own items filter at once");
        cx.run_until_parked();
        assert_eq!(names(&view, cx), vec!["report-old.pdf", "report.pdf"], "and the search finds the one below");

        press(&view, cx, "backspace");
        press(&view, cx, "backspace");
        press(&view, cx, "backspace");
        press(&view, cx, "backspace");
        press(&view, cx, "backspace");
        press(&view, cx, "backspace");
        cx.run_until_parked();
        assert_eq!(names(&view, cx), vec!["notes.txt", "report.pdf", "sub"], "an empty search shows everything");

        type_text(&view, cx, "kind:folders");
        cx.run_until_parked();
        assert_eq!(names(&view, cx), vec!["sub"]);
        press(&view, cx, "escape");
        cx.run_until_parked();
        assert_eq!(names(&view, cx), vec!["notes.txt", "report.pdf", "sub"], "Esc closes the search");
        assert!(view.read_with(cx, |v, _| v.search.is_none()));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[gpui::test]
    fn privacy_mode_hides_names_and_pictures_until_the_pointer_is_over_the_vial(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| VialView::new(window, init(), cx));
        assert!(!view.read_with(cx, |v, _| v.masked()), "off by default");
        view.update(cx, |v, _| v.privacy_mask = true);
        assert!(view.read_with(cx, |v, _| v.masked()), "on and the pointer is elsewhere");
        view.update(cx, |v, _| v.hovered = true);
        assert!(!view.read_with(cx, |v, _| v.masked()), "the pointer is over it");
    }

    #[gpui::test]
    fn new_notes_are_empty_text_files_in_the_notes_portal_with_free_names(cx: &mut TestAppContext) {
        let dir = std::env::temp_dir().join(format!("desqu-notes-view-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| VialView::new(window, init(), cx));
        view.update(cx, |v, cx| v.set_portal_source(Some(dir.to_string_lossy().into_owned()), cx));
        cx.run_until_parked();
        view.update(cx, |v, cx| v.new_note(cx));
        view.update(cx, |v, cx| v.new_note(cx));
        cx.run_until_parked();
        let mut names = view.read_with(cx, |v, _| v.items.iter().map(|i| i.name.to_string()).collect::<Vec<_>>());
        names.sort();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(names, vec!["Note (2).txt", "Note.txt"]);
    }

    #[gpui::test]
    fn the_arrow_keys_walk_the_items_and_shift_extends_the_selection(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| VialView::new(window, init(), cx));
        cx.run_until_parked();
        let selected = |view: &Entity<VialView>, cx: &mut gpui::VisualTestContext| view.read_with(cx, |v, _| v.selected.iter().copied().collect::<Vec<_>>());
        press(&view, cx, "right");
        assert_eq!(selected(&view, cx), vec![0], "the first press lands on the first item");
        press(&view, cx, "right");
        press(&view, cx, "right");
        assert_eq!(selected(&view, cx), vec![2]);
        press(&view, cx, "shift-left");
        assert_eq!(selected(&view, cx), vec![1, 2]);
        press(&view, cx, "home");
        assert_eq!(selected(&view, cx), vec![0]);
        press(&view, cx, "end");
        assert_eq!(selected(&view, cx), vec![3]);
        press(&view, cx, "escape");
        assert!(selected(&view, cx).is_empty());
    }

    #[gpui::test]
    fn a_box_dragged_down_and_right_selects_only_what_it_touches(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| VialView::new(window, init(), cx));
        cx.run_until_parked();
        let bounds = view.read_with(cx, |v, _| v.bounds.borrow().clone());
        let (a, b) = (bounds[&0], bounds[&1]);
        // Starts above the first item and ends inside the second: the third and fourth stay out.
        let start = point(a.left() + px(2.), a.top() - px(6.));
        let end = point(b.left() + px(10.), b.top() + px(10.));
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
        let selected = view.read_with(cx, |v, _| v.selected.iter().copied().collect::<Vec<_>>());
        assert_eq!(selected, vec![0, 1]);
    }
}
