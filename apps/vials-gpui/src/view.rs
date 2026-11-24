//! One vial's window content: header (name, collapse), then a grid of its items.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    div, img, RenderImage, prelude::*, px, rgba, App, Bounds, ClickEvent, Context, Entity, ExternalPaths, FocusHandle, FontWeight, Image,
    ImageFormat, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    Render, SharedString, Window,
};
use vials_core::model::PixelRect;

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

pub struct Item {
    pub id: String,
    pub name: SharedString,
    pub missing: bool,
}

/// Where the window sits, so collapsing can resize it in place.
pub struct Placement {
    pub rect: PixelRect,
    pub collapsed_height: i32,
}

/// Pointer movement (px) before a press turns into a drag.
const DRAG_THRESHOLD: f32 = 5.0;

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
            this.refresh_liquid(cx);
            if let Some(now) = win::window_rect(hwnd) {
                if this.collapsed {
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
        view
    }

    fn set_items(&mut self, ids: Vec<String>, cx: &mut Context<Self>) {
        self.items = ids
            .iter()
            .map(|id| Item { name: display_name(id).into(), missing: !Path::new(id).exists(), id: id.clone() })
            .collect();
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

    fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(live) = self.live.clone() else { return };
        let look = live.engine.look();
        if look != self.theme {
            self.theme = look;
            self.material_dirty = true;
            self.refresh_liquid(cx);
        }
        if let Some(vial) = live.engine.vials().into_iter().find(|v| v.id == live.vial_id) {
            self.name = vial.name.clone().into();
            let ids = vial.items;
            self.set_items(ids, cx);
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
                if let Ok(png) = desqu_desktop::icon_png(&id, 32) {
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

    fn toggle_collapsed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.collapsed = !self.collapsed;
        if let Some(hwnd) = win::hwnd(window) {
            let mut r = self.placement.rect;
            if self.collapsed {
                r.bottom = r.top + self.placement.collapsed_height;
            }
            win::place_at_bottom(hwnd, &r);
        }
        if let Some(live) = &self.live {
            live.engine.set_collapsed(&live.vial_id, self.collapsed);
        }
        cx.notify();
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
        cx.spawn(async move |this, cx| {
            let end = vials_platform::drag_items(&ids);
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
        if item.missing || !live.engine.is_assigned(&item.id) {
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
        if self.live.is_none() {
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
        for (key, label) in [("name", "Sort by name"), ("type", "Sort by type"), ("modified", "Sort by date modified"), ("size", "Sort by size")] {
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
        if k.modifiers.control && k.key == "v" {
            self.paste(cx);
            return;
        }
        if k.modifiers.control && k.key == "a" {
            self.selected = (0..self.items.len()).collect();
            cx.notify();
            return;
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
        // Once per window: lets the shell draw Explorer's picture of dragged items over it.
        #[cfg(not(test))]
        if let Some(hwnd) = win::hwnd(window) {
            vials_platform::show_drag_image_over(hwnd.0 as isize);
        }
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
            .child(if self.collapsed { "v" } else { "^" });

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
        let header = div()
            .id("header")
            .h(px(34.))
            .flex_none()
            .flex()
            .items_center()
            .border_b_1()
            .border_color(rgba(t.line))
            .child(
                // Press and drag moves the window, a plain click renames it, right click opens the menu.
                div()
                    .id("title")
                    .flex_1()
                    .h_full()
                    .px_3()
                    .flex()
                    .items_center()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _: &MouseDownEvent, window, _| {
                            if this.editing.is_some() {
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
                let icon = match self.icons.get(&item.id) {
                    Some(image) => img(image.clone()).size(px(32.)).into_any_element(),
                    None => div().size(px(32.)).rounded(px(6.)).bg(rgba(t.placeholder)).into_any_element(),
                };
                let remove_id = item.id.clone();
                div()
                    .id(("item", i))
                    .group("item")
                    .relative()
                    .w(px(84.))
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
                            win::open_path(&id);
                        } else if !e.modifiers().control && this.selected.len() > 1 {
                            // A plain click on one of several selected items narrows it to that one.
                            this.selected = BTreeSet::from([i]);
                            cx.notify();
                        }
                    }))
                    .child(icon)
                    .child(div().text_xs().max_w_full().h(px(34.)).overflow_hidden().text_center().child(item.name.clone()))
                    .when(live, |d| {
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
            .when(!self.collapsed, |d| d.child(grid))
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
