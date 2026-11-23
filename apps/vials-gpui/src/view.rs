//! One vial's window content: header (drag area, name, collapse), then a grid of its items.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use gpui::{
    div, img, prelude::*, px, rgba, white, ClickEvent, Context, Entity, ExternalPaths, FocusHandle, FontWeight, Image,
    ImageFormat, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    Render, SharedString, Window, WindowControlArea,
};
use vials_core::model::PixelRect;

use crate::engine::Engine;
use crate::win;

/// Bumped whenever the config changed (an item came or left a vial), so every vial window reloads.
pub struct Revision(pub u64);

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

/// Pointer movement (px) before a press on an item turns into a drag.
const DRAG_THRESHOLD: f32 = 5.0;

pub struct VialView {
    name: SharedString,
    items: Vec<Item>,
    icons: HashMap<String, Arc<Image>>,
    icons_requested: HashSet<String>,
    selected: Option<usize>,
    press: Option<(usize, Point<Pixels>)>,
    collapsed: bool,
    placement: Placement,
    save_gen: u64,
    live: Option<Live>,
    focus: FocusHandle,
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
    pub fn new(
        window: &mut Window,
        name: &str,
        ids: Vec<String>,
        collapsed: bool,
        placement: Placement,
        live: Option<Live>,
        cx: &mut Context<Self>,
    ) -> Self {
        if let Some(live) = live.clone() {
            // A moved or resized window is saved once it has stayed still for a moment.
            let (engine, vial_id) = (live.engine.clone(), live.vial_id.clone());
            cx.observe_window_bounds(window, move |this, window, cx| {
                let Some(hwnd) = win::hwnd(window) else { return };
                this.save_gen += 1;
                let generation = this.save_gen;
                let (engine, vial_id) = (engine.clone(), vial_id.clone());
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(std::time::Duration::from_millis(500)).await;
                    if this.update(cx, |view, _| view.save_gen == generation).unwrap_or(false) {
                        if let Some(rect) = win::window_rect(hwnd) {
                            engine.persist_rect(&vial_id, rect);
                        }
                    }
                })
                .detach();
            })
            .detach();
            // Items that came or left (any vial window, or the desktop) show up here too.
            cx.observe(&live.revision, |this, _, cx| this.reload(cx)).detach();
            // The window only takes the keyboard while it is being used.
            cx.observe_window_activation(window, |_, window, _| {
                if !window.is_window_active() {
                    if let Some(hwnd) = win::hwnd(window) {
                        win::release_keyboard(hwnd);
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
            selected: None,
            press: None,
            collapsed,
            placement,
            save_gen: 0,
            live,
            focus: cx.focus_handle(),
        };
        view.set_items(ids, cx);
        view
    }

    fn set_items(&mut self, ids: Vec<String>, cx: &mut Context<Self>) {
        self.items = ids
            .iter()
            .map(|id| Item { name: display_name(id).into(), missing: !Path::new(id).exists(), id: id.clone() })
            .collect();
        self.selected = self.selected.filter(|i| *i < self.items.len());
        let wanted: Vec<String> = self
            .items
            .iter()
            .filter(|it| !it.missing && !self.icons.contains_key(&it.id) && !self.icons_requested.contains(&it.id))
            .map(|it| it.id.clone())
            .collect();
        self.request_icons(wanted, cx);
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(live) = &self.live else { return };
        let ids = live.engine.vial_items(&live.vial_id);
        self.set_items(ids, cx);
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

    /// Tells every vial window that the config changed.
    fn announce(&self, cx: &mut Context<Self>) {
        if let Some(live) = &self.live {
            live.revision.update(cx, |r, cx| {
                r.0 += 1;
                cx.notify();
            });
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
        cx.notify();
    }

    /// A click in the vial gives it the keyboard (for Delete, Ctrl+C); it hands it back when it loses focus.
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
        if live.engine.drop_items(&live.vial_id, &paths) {
            self.announce(cx);
        }
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
        let ids = vec![item.id.clone()];
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

    fn on_key(&mut self, e: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(id) = self.selected.and_then(|i| self.items.get(i)).map(|it| it.id.clone()) else { return };
        let k = &e.keystroke;
        if k.key == "delete" {
            self.remove(&id, cx);
        } else if k.modifiers.control && (k.key == "c" || k.key == "x") {
            let _ = vials_platform::clipboard::put_files(&[id], k.key == "x");
        }
    }
}

impl Render for VialView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let chevron = div()
            .id("collapse")
            .w(px(30.))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .text_xs()
            .hover(|s| s.bg(rgba(0xffffff22)))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.toggle_collapsed(window, cx)))
            .child(if self.collapsed { "v" } else { "^" });

        let header = div()
            .h(px(34.))
            .flex_none()
            .flex()
            .items_center()
            .border_b_1()
            .border_color(rgba(0xffffff24))
            .child(
                // The drag area: Windows treats it as a title bar, so the window moves with the mouse.
                div()
                    .flex_1()
                    .h_full()
                    .px_3()
                    .flex()
                    .items_center()
                    .window_control_area(WindowControlArea::Drag)
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(self.name.clone()),
            )
            .child(chevron);

        let selected = self.selected;
        let live = self.live.is_some();
        let grid = div().flex_1().min_h_0().p_2().flex().flex_wrap().content_start().gap_1().children(
            self.items.iter().enumerate().map(|(i, item)| {
                let id = item.id.clone();
                let icon = match self.icons.get(&item.id) {
                    Some(image) => img(image.clone()).size(px(32.)).into_any_element(),
                    None => div().size(px(32.)).rounded(px(6.)).bg(rgba(0xffffff18)).into_any_element(),
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
                    .when(selected == Some(i), |d| d.bg(rgba(0x78afff48)).border_color(rgba(0xaad2ffb4)))
                    .when(selected != Some(i), |d| d.hover(|s| s.bg(rgba(0xffffff28)).border_color(rgba(0xffffff57))))
                    .when(item.missing, |d| d.opacity(0.5))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                            this.selected = Some(i);
                            this.press = Some((i, e.position));
                            this.take_keyboard(window);
                            cx.notify();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                            this.selected = Some(i);
                            this.item_menu(i, cx);
                            cx.notify();
                        }),
                    )
                    .on_click(cx.listener(move |_, e: &ClickEvent, _, _| {
                        if e.click_count() >= 2 {
                            win::open_path(&id);
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
            }),
        );

        div()
            .id("vial")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| this.on_key(e, cx)))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                if let Some((index, start)) = this.press {
                    if e.pressed_button == Some(MouseButton::Left) {
                        let (dx, dy) = (f32::from(e.position.x - start.x), f32::from(e.position.y - start.y));
                        if (dx * dx + dy * dy).sqrt() > DRAG_THRESHOLD {
                            this.press = None;
                            this.start_drag(index, cx);
                        }
                    } else {
                        this.press = None;
                    }
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, _| this.press = None))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| this.drop_paths(paths, cx)))
            .drag_over::<ExternalPaths>(|s, _, _, _| s.border_color(rgba(0xaad2ffd9)).bg(rgba(0x78afff30)))
            .size_full()
            .rounded(px(12.))
            .border_1()
            .border_color(rgba(0xffffff42))
            .bg(rgba(0x181c2633))
            .text_color(white())
            .flex()
            .flex_col()
            .child(header)
            .when(!self.collapsed, |d| d.child(grid))
    }
}
