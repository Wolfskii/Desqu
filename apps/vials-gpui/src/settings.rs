//! The settings window: look of the vials and the on/off options. Changes apply at once (the glass
//! tint when the slider is let go), are saved by the engine, and reach every vial window through the
//! shared revision.

use std::sync::Arc;

use gpui::{
    div, point, prelude::*, px, rgba, size, AnyWindowHandle, App, Bounds, ClickEvent, Context, Entity, Global, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Render, SharedString, Window, WindowBackgroundAppearance,
    WindowBounds, WindowOptions,
};
use vials_core::model::{VialTheme, VialsConfig};

use crate::engine::Engine;
use crate::view::{bump, Revision};

/// The one settings window, if it is open.
#[derive(Default)]
struct Open(Option<AnyWindowHandle>);
impl Global for Open {}

const TRACK_LEFT: f32 = 24.0 + 250.0; // window padding + label column: where the slider track starts
const TRACK_WIDTH: f32 = 220.0;

/// Slider position (px from the left edge of the track) to a tint of 0..=100 in steps of 5.
pub fn tint_from_x(x: f32) -> u8 {
    let v = (x / TRACK_WIDTH).clamp(0.0, 1.0) * 100.0;
    ((v / 5.0).round() * 5.0) as u8
}

struct SettingsView {
    engine: Arc<Engine>,
    revision: Entity<Revision>,
    /// The tint while the slider is being dragged (applied on release).
    dragging: Option<u8>,
}

impl SettingsView {
    fn config(&self) -> VialsConfig {
        self.engine.config()
    }

    fn change(&self, cx: &mut App, edit: impl FnOnce(&mut VialsConfig)) {
        if self.engine.edit_settings(edit) {
            bump(&self.revision, cx);
        }
    }
}

fn button(label: impl Into<SharedString>, active: bool) -> gpui::Stateful<gpui::Div> {
    let label: SharedString = label.into();
    div()
        .id(label.clone())
        .px_3()
        .h(px(28.))
        .flex()
        .items_center()
        .rounded(px(6.))
        .border_1()
        .text_sm()
        .border_color(rgba(if active { 0xaad2ffb4 } else { 0xffffff30 }))
        .bg(rgba(if active { 0x78afff48 } else { 0xffffff10 }))
        .hover(|s| s.bg(rgba(0xffffff28)))
        .child(label)
}

fn row(label: &str, hint: &str) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .min_h(px(44.))
        .child(
            div()
                .w(px(250.))
                .flex_none()
                .flex()
                .flex_col()
                .child(div().text_sm().child(label.to_string()))
                .child(div().text_xs().text_color(rgba(0xffffff80)).child(hint.to_string())),
        )
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let cfg = self.config();
        let tint = self.dragging.unwrap_or(cfg.glass_tint);

        let themes = [
            (VialTheme::Glass, "Glass"),
            (VialTheme::Liquid, "Liquid"),
            (VialTheme::Classic, "Classic"),
            (VialTheme::Fluent, "Fluent dark"),
            (VialTheme::FluentLight, "Fluent light"),
        ];
        let theme_row = row("Look of vials", "Glass and Liquid blur what is behind").child(div().flex().flex_wrap().gap_1().w(px(260.)).children(
            themes.into_iter().map(|(theme, label)| {
                button(label, cfg.theme == theme).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.change(cx, |c| c.theme = theme)))
            }),
        ));

        let slider = row("Glass tint", "Lower is clearer").child(
            div()
                .id("tint-track")
                .w(px(TRACK_WIDTH))
                .h(px(24.))
                .flex()
                .items_center()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, e: &MouseDownEvent, _, cx| {
                        this.dragging = Some(tint_from_x(f32::from(e.position.x) - TRACK_LEFT));
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .relative()
                        .w_full()
                        .h(px(4.))
                        .rounded(px(2.))
                        .bg(rgba(0xffffff30))
                        .child(div().h_full().rounded(px(2.)).bg(rgba(0x78afffd0)).w(px(TRACK_WIDTH * f32::from(tint) / 100.)))
                        .child(
                            div()
                                .absolute()
                                .top(px(-6.))
                                .left(px(TRACK_WIDTH * f32::from(tint) / 100. - 8.))
                                .size(px(16.))
                                .rounded(px(8.))
                                .bg(rgba(0xffffffff)),
                        ),
                ),
        );

        let toggle = |label: &'static str, hint: &'static str, on: bool, set: fn(&mut VialsConfig, bool)| {
            row(label, hint).child(
                button(if on { "On" } else { "Off" }, on)
                    .id(label)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.change(cx, |c| set(c, !on)))),
            )
        };

        div()
            .id("settings")
            .size_full()
            .p(px(24.))
            .bg(rgba(0x14171eff))
            .text_color(rgba(0xffffffff))
            .flex()
            .flex_col()
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                if this.dragging.is_some() && e.pressed_button == Some(MouseButton::Left) {
                    this.dragging = Some(tint_from_x(f32::from(e.position.x) - TRACK_LEFT));
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    if let Some(tint) = this.dragging.take() {
                        this.change(cx, |c| c.glass_tint = tint);
                    }
                }),
            )
            .child(div().text_lg().pb_2().child("Vials settings"))
            .child(theme_row)
            .child(slider)
            .child(toggle("Sort new desktop items", "Put new files into a vial by its rules", cfg.auto_sort, |c, v| c.auto_sort = v))
            .child(toggle("Hide items on the desktop", "Items in a vial leave the desktop", cfg.hide_on_desktop, |c, v| c.hide_on_desktop = v))
            .child(toggle("Create vials by dragging a box", "Drag over empty desktop", cfg.create_on_drag, |c, v| c.create_on_drag = v))
            .child(toggle("Desktop right-click entry", "\"Create vial here\"", cfg.context_menu, |c, v| c.context_menu = v))
    }
}

/// Opens the settings window, or brings the open one to the front.
pub fn open(cx: &mut App, engine: Arc<Engine>, revision: Entity<Revision>) {
    if let Some(Open(Some(handle))) = cx.try_global::<Open>().map(|o| Open(o.0)) {
        if handle.update(cx, |_, window, _| window.activate_window()).is_ok() {
            return;
        }
    }
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(200.), px(120.)), size(px(560.), px(400.))))),
            titlebar: Some(gpui::TitlebarOptions { title: Some("Vials settings".into()), ..Default::default() }),
            focus: true,
            is_resizable: false,
            window_background: WindowBackgroundAppearance::Opaque,
            ..Default::default()
        },
        move |_, cx| cx.new(|_| SettingsView { engine, revision, dragging: None }),
    );
    if let Ok(handle) = opened {
        cx.set_global(Open(Some(handle.into())));
    }
}

#[cfg(test)]
mod tests {
    use super::tint_from_x;

    #[test]
    fn the_slider_snaps_to_steps_of_five_and_stays_in_range() {
        assert_eq!(tint_from_x(-50.0), 0);
        assert_eq!(tint_from_x(1000.0), 100);
        assert_eq!(tint_from_x(110.0), 50);
        assert_eq!(tint_from_x(45.0), 20);
    }
}
