//! A small popup menu at the pointer: a blurred window with a list of entries. It takes the keyboard
//! (Esc closes it) and closes itself when it loses focus or an entry is chosen.

use std::rc::Rc;

use gpui::{
    div, point, prelude::*, px, rgb, rgba, size, white, App, FontWeight, Bounds, Context, FocusHandle, IntoElement, KeyDownEvent,
    Render, SharedString, Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
};
use vials_core::gesture;

use crate::win;

type Action = Rc<dyn Fn(&mut App)>;

/// A small picture in front of an entry's label.
#[derive(Clone, Copy)]
pub enum Icon {
    /// A plus in an accent circle: "make something new".
    Add,
}

/// One line of a menu. An entry without a label is a separator.
pub struct MenuEntry {
    pub label: SharedString,
    pub checked: bool,
    pub enabled: bool,
    pub icon: Option<Icon>,
    pub action: Option<Action>,
}

impl MenuEntry {
    pub fn new(label: impl Into<SharedString>, action: impl Fn(&mut App) + 'static) -> Self {
        Self { label: label.into(), checked: false, enabled: true, icon: None, action: Some(Rc::new(action)) }
    }
    pub fn separator() -> Self {
        Self { label: SharedString::default(), checked: false, enabled: false, icon: None, action: None }
    }
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn checked(mut self, on: bool) -> Self {
        self.checked = on;
        self
    }
}

const WIDTH: f32 = 220.0;
const ROW: f32 = 28.0;
const SEPARATOR: f32 = 9.0;
const PAD: f32 = 6.0;
/// A menu of one entry (such as "Create vial here") is a single centred button, a little bigger.
const SINGLE_WIDTH: f32 = 200.0;
const SINGLE_ROW: f32 = 36.0;
const ACCENT: u32 = 0x4fa3ff;

fn single(entries: &[MenuEntry]) -> bool {
    entries.len() == 1
}

fn width_of(entries: &[MenuEntry]) -> f32 {
    if single(entries) {
        SINGLE_WIDTH
    } else {
        WIDTH
    }
}

fn height_of(entries: &[MenuEntry]) -> f32 {
    if single(entries) {
        return PAD * 2.0 + SINGLE_ROW;
    }
    PAD * 2.0 + entries.iter().map(|e| if e.label.is_empty() { SEPARATOR } else { ROW }).sum::<f32>()
}

fn icon_element(icon: Icon) -> gpui::AnyElement {
    match icon {
        // Two bars rather than a "+" glyph, so the plus sits exactly in the middle of its circle.
        Icon::Add => div()
            .size(px(18.))
            .flex_none()
            .rounded_full()
            .bg(rgb(ACCENT))
            .relative()
            .child(div().absolute().left(px(4.)).top(px(8.)).w(px(10.)).h(px(2.)).rounded(px(1.)).bg(white()))
            .child(div().absolute().left(px(8.)).top(px(4.)).w(px(2.)).h(px(10.)).rounded(px(1.)).bg(white()))
            .into_any_element(),
    }
}

struct MenuView {
    entries: Vec<MenuEntry>,
    focus: FocusHandle,
}

impl Render for MenuView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let single = single(&self.entries);
        let rows = self.entries.iter().enumerate().map(|(i, e)| {
            if e.label.is_empty() {
                return div().h(px(SEPARATOR)).flex().items_center().child(div().h(px(1.)).w_full().bg(rgba(0xffffff30))).into_any_element();
            }
            let action = e.action.clone();
            let enabled = e.enabled;
            div()
                .id(("entry", i))
                .h(px(if single { SINGLE_ROW } else { ROW }))
                .flex()
                .items_center()
                .rounded(px(6.))
                .text_sm()
                .when(single, |d| d.px_3().gap(px(10.)).font_weight(FontWeight::MEDIUM))
                .when(!single, |d| d.px_2().gap_2().child(div().w(px(14.)).text_xs().child(if e.checked { "v" } else { "" })))
                .when(enabled, |d| d.cursor_pointer().hover(|s| s.bg(rgba(0xffffff2a))))
                .when(!enabled, |d| d.opacity(0.45))
                .when_some(e.icon, |d, icon| d.child(icon_element(icon)))
                .child(e.label.clone())
                .on_click(cx.listener(move |_, _, window, cx| {
                    if !enabled {
                        return;
                    }
                    window.remove_window();
                    if let Some(action) = &action {
                        action(cx);
                    }
                }))
                .into_any_element()
        });
        div()
            .id("menu")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|_, e: &KeyDownEvent, window, _| {
                if e.keystroke.key == "escape" {
                    window.remove_window();
                }
            }))
            .size_full()
            .rounded(px(10.))
            .border_1()
            .border_color(rgba(0xffffff42))
            .bg(rgba(0x181c26aa))
            .text_color(white())
            .p(px(PAD))
            .flex()
            .flex_col()
            .children(rows)
    }
}

/// Opens a menu with its top-left corner at the pointer (physical screen pixels), kept on its monitor.
pub fn open(cx: &mut App, cursor: (i32, i32), entries: Vec<MenuEntry>) {
    let monitors = win::monitors();
    let Some(monitor) = vials_core::layout::monitor_containing(
        &vials_core::model::PixelRect { left: cursor.0, top: cursor.1, right: cursor.0 + 1, bottom: cursor.1 + 1 },
        &monitors,
    )
    .or(monitors.first()) else {
        return;
    };
    let scale = monitor.scale as f32;
    let (w, h) = ((width_of(&entries) * scale).round() as i32, (height_of(&entries) * scale).round() as i32);
    let (x, y) = gesture::popup_position(cursor, (w, h), &monitor.bounds);
    let rect = vials_core::model::PixelRect { left: x, top: y, right: x + w, bottom: y + h };
    let logical = size(px(width_of(&entries)), px(height_of(&entries)));

    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(0.), px(0.)), logical))),
            titlebar: None,
            kind: WindowKind::PopUp,
            focus: true,
            is_resizable: false,
            window_background: WindowBackgroundAppearance::Blurred,
            ..Default::default()
        },
        move |window, cx| {
            let focus = cx.focus_handle();
            window.focus(&focus);
            cx.new(|cx| {
                // Clicking anywhere else closes it.
                cx.observe_window_activation(window, |_: &mut MenuView, window, _| {
                    if !window.is_window_active() {
                        window.remove_window();
                    }
                })
                .detach();
                MenuView { entries, focus }
            })
        },
    );
    if let Ok(handle) = opened {
        let _ = handle.update(cx, |_, window, _| {
            if let Some(hwnd) = win::hwnd(window) {
                win::make_popup(hwnd);
                win::place_on_top(hwnd, &rect);
            }
            window.activate_window();
        });
    }
}
