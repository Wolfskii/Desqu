//! One vial's window content: header (drag area, name, collapse), then a grid of its items.

use std::path::Path;
use std::sync::Arc;

use gpui::{
    div, img, prelude::*, px, rgba, white, ClickEvent, Context, FontWeight, Image, ImageFormat, IntoElement, Render,
    SharedString, Window, WindowControlArea,
};
use vials_core::model::PixelRect;

use crate::engine::Engine;
use crate::win;

pub struct Item {
    pub id: String,
    pub name: SharedString,
    pub missing: bool,
    pub icon: Option<Arc<Image>>,
}

/// Where the window sits, so collapsing can resize it in place.
pub struct Placement {
    pub rect: PixelRect,
    pub collapsed_height: i32,
}

pub struct VialView {
    name: SharedString,
    items: Vec<Item>,
    selected: Option<usize>,
    collapsed: bool,
    placement: Placement,
    save_gen: u64,
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
        live: Option<(Arc<Engine>, String)>,
        cx: &mut Context<Self>,
    ) -> Self {
        // Live mode: a moved or resized window is saved, once it has stayed still for a moment.
        if let Some((engine, vial_id)) = live {
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
        }

        let items: Vec<Item> = ids
            .into_iter()
            .map(|id| Item { name: display_name(&id).into(), missing: !Path::new(&id).exists(), icon: None, id })
            .collect();
        // Icons come from the shell (about 6 ms each) and some handlers (.url shortcuts) only work in a
        // single-threaded COM apartment, which GPUI's pool threads are not: one plain thread of our own
        // fetches them all, and the UI picks them up as they arrive.
        let wanted: Vec<(usize, String)> = items.iter().enumerate().filter(|(_, it)| !it.missing).map(|(i, it)| (i, it.id.clone())).collect();
        if !wanted.is_empty() {
            let (tx, rx) = std::sync::mpsc::channel::<(usize, Vec<u8>)>();
            std::thread::spawn(move || {
                for (i, id) in wanted {
                    if let Ok(png) = desqu_desktop::icon_png(&id, 32) {
                        if tx.send((i, png)).is_err() {
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
                        for (i, png) in got {
                            if let Some(item) = view.items.get_mut(i) {
                                item.icon = Some(Arc::new(Image::from_bytes(ImageFormat::Png, png)));
                            }
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
        Self { name: name.to_string().into(), items, selected: None, collapsed, placement, save_gen: 0 }
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
        let grid = div().flex_1().min_h_0().p_2().flex().flex_wrap().content_start().gap_1().children(
            self.items.iter().enumerate().map(|(i, item)| {
                let id = item.id.clone();
                let icon = match &item.icon {
                    Some(image) => img(image.clone()).size(px(32.)).into_any_element(),
                    None => div().size(px(32.)).rounded(px(6.)).bg(rgba(0xffffff18)).into_any_element(),
                };
                div()
                    .id(("item", i))
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
                    .on_click(cx.listener(move |this, e: &ClickEvent, _w, cx| {
                        this.selected = Some(i);
                        if e.click_count() >= 2 {
                            win::open_path(&id);
                        }
                        cx.notify();
                    }))
                    .child(icon)
                    .child(div().text_xs().max_w_full().h(px(34.)).overflow_hidden().text_center().child(item.name.clone()))
            }),
        );

        div()
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
