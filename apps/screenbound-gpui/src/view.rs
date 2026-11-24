//! The Screenbound window: on/off, the zone (drawn on a preview of the monitors, or a preset), the
//! apps that stay inside it, what is being managed now, and the last things that happened.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    canvas, div, point, prelude::*, px, rgba, size, AnyWindowHandle, App, Bounds, ClickEvent, Context, Entity, Global, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Render, SharedString, Window, WindowBackgroundAppearance, WindowBounds,
    WindowOptions,
};
use screenbound_module::core::monitor::MonitorInfo;
use screenbound_module::core::zone::NormalizedRect;

use crate::model::{self, PRESETS};
use crate::runtime::Backend;

#[derive(Default)]
struct Open(Option<AnyWindowHandle>);
impl Global for Open {}

const PREVIEW_W: f32 = 640.0;
const PREVIEW_H: f32 = 150.0;

pub fn button(label: impl Into<SharedString>, active: bool) -> gpui::Stateful<gpui::Div> {
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

/// A monitor drawn in the preview: its index and rectangle in preview pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub index: usize,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// The monitors, scaled to fit a `width` x `height` box and centred in it, keeping their real arrangement.
pub fn layout_monitors(monitors: &[MonitorInfo], width: f32, height: f32) -> Vec<Placed> {
    if monitors.is_empty() {
        return Vec::new();
    }
    let min_x = monitors.iter().map(|m| m.bounds.left).min().unwrap_or(0) as f32;
    let min_y = monitors.iter().map(|m| m.bounds.top).min().unwrap_or(0) as f32;
    let max_x = monitors.iter().map(|m| m.bounds.right).max().unwrap_or(1) as f32;
    let max_y = monitors.iter().map(|m| m.bounds.bottom).max().unwrap_or(1) as f32;
    let scale = (width / (max_x - min_x).max(1.0)).min(height / (max_y - min_y).max(1.0));
    let (used_w, used_h) = ((max_x - min_x) * scale, (max_y - min_y) * scale);
    let (ox, oy) = ((width - used_w) / 2.0, (height - used_h) / 2.0);
    monitors
        .iter()
        .enumerate()
        .map(|(index, m)| Placed {
            index,
            x: ox + (m.bounds.left as f32 - min_x) * scale,
            y: oy + (m.bounds.top as f32 - min_y) * scale,
            w: (m.bounds.right - m.bounds.left) as f32 * scale,
            h: (m.bounds.bottom - m.bounds.top) as f32 * scale,
        })
        .collect()
}

/// How long ago, for the activity list.
pub fn ago(now_ms: u64, then_ms: u64) -> String {
    let s = now_ms.saturating_sub(then_ms) / 1000;
    match s {
        0..=4 => "just now".into(),
        5..=59 => format!("{s}s ago"),
        60..=3599 => format!("{}m ago", s / 60),
        _ => format!("{}h ago", s / 3600),
    }
}

struct Drag {
    monitor: usize,
    start: (f32, f32),
    current: (f32, f32),
}

pub struct ScreenboundView {
    rt: Arc<dyn Backend>,
    /// In the Desqu window: no start-up or update rows, and no button for the second window.
    embedded: bool,
    /// Where the preview is in the window (set while painting), to turn mouse positions into positions on a monitor.
    origin: Rc<Cell<Point<Pixels>>>,
    drag: Option<Drag>,
    note: String,
    update_note: String,
    update_url: Option<String>,
    /// The snapshot generation whose apps were last merged into the config.
    merged: u64,
}

impl ScreenboundView {
    fn check_update(&mut self, cx: &mut Context<Self>) {
        self.update_note = "Checking...".into();
        self.update_url = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async { desqu_native_shell::update::check(env!("CARGO_PKG_VERSION"), crate::MANIFEST_URL) }).await;
            let _ = this.update(cx, |view, cx| {
                match result {
                    Ok(desqu_native_shell::update::Outcome::UpToDate) => view.update_note = format!("Version {} is the latest", env!("CARGO_PKG_VERSION")),
                    Ok(desqu_native_shell::update::Outcome::Available(r)) => {
                        view.update_note = format!("Version {} is available", r.version);
                        view.update_url = r.url;
                    }
                    Err(e) => view.update_note = e,
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Opens windows are remembered in the Apps list (so they can be turned on); done once per new snapshot.
    fn remember_apps(&mut self) {
        let generation = self.rt.generation();
        if self.merged == generation {
            return;
        }
        self.merged = generation;
        let snap = self.rt.snapshot();
        let Some(config) = &snap.config else { return };
        let merged = model::remember_apps(&config.known_apps, &snap.windows);
        if merged != config.known_apps {
            self.rt.change(move |c| {
                c.known_apps = model::remember_apps(&c.known_apps, &[]).into_iter().chain(merged.iter().cloned()).fold(Vec::new(), |mut acc, a| {
                    if !acc.iter().any(|x: &screenbound_module::core::config::KnownApp| x.process_name.eq_ignore_ascii_case(&a.process_name)) {
                        acc.push(a);
                    }
                    acc
                });
                true
            });
        }
    }

    fn zone_from_drag(&self, monitors: &[Placed], drag: &Drag) -> Option<NormalizedRect> {
        let m = monitors.iter().find(|m| m.index == drag.monitor)?;
        let rel = |p: (f32, f32)| (f64::from((p.0 - m.x) / m.w.max(1.0)), f64::from((p.1 - m.y) / m.h.max(1.0)));
        Some(model::drag_rect(rel(drag.start), rel(drag.current)))
    }
}

impl Render for ScreenboundView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.remember_apps();
        let snap = self.rt.snapshot();
        let enabled = snap.config.as_ref().is_none_or(|c| c.enabled);
        let zone = snap.config.as_ref().and_then(|c| c.zones.first()).map(|z| z.rect);
        let placed = layout_monitors(&snap.monitors, PREVIEW_W, PREVIEW_H);
        let drag_rect = self.drag.as_ref().and_then(|d| self.zone_from_drag(&placed, d));
        let shown_zone = drag_rect.or(zone);
        let managed = snap.status.as_ref().map(|s| s.managed.clone()).unwrap_or_default();

        let running: Vec<String> = snap.windows.iter().map(|w| w.identity.process_name.clone()).collect();
        let apps = snap.config.as_ref().map(|c| model::app_rows(&c.known_apps, &running)).unwrap_or_default();

        let origin = self.origin.clone();
        let preview = div()
            .id("preview")
            .relative()
            .w(px(PREVIEW_W))
            .h(px(PREVIEW_H))
            .rounded(px(8.))
            .bg(rgba(0x00000040))
            .child(canvas(move |bounds, _, _| origin.set(bounds.origin), |_, _, _, _| {}).absolute().size_full())
            .children(placed.iter().map(|m| {
                let tint = shown_zone.map(|z| (z.x as f32, z.y as f32, z.width as f32, z.height as f32));
                div()
                    .absolute()
                    .left(px(m.x))
                    .top(px(m.y))
                    .w(px(m.w))
                    .h(px(m.h))
                    .border_1()
                    .border_color(rgba(0xffffff50))
                    .bg(rgba(0xffffff12))
                    .when_some(tint, |d, (x, y, w, h)| {
                        d.child(
                            div()
                                .absolute()
                                .left(px(m.w * x))
                                .top(px(m.h * y))
                                .w(px(m.w * w))
                                .h(px(m.h * h))
                                .border_1()
                                .border_color(rgba(0xaad2ffff))
                                .bg(rgba(0x78afff60)),
                        )
                    })
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, _, cx| {
                    let o = this.origin.get();
                    let p = (f32::from(e.position.x - o.x), f32::from(e.position.y - o.y));
                    let snap = this.rt.snapshot();
                    let placed = layout_monitors(&snap.monitors, PREVIEW_W, PREVIEW_H);
                    if let Some(m) = placed.iter().find(|m| p.0 >= m.x && p.0 <= m.x + m.w && p.1 >= m.y && p.1 <= m.y + m.h) {
                        this.drag = Some(Drag { monitor: m.index, start: p, current: p });
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                if let Some(d) = this.drag.as_mut() {
                    let o = this.origin.get();
                    d.current = (f32::from(e.position.x - o.x), f32::from(e.position.y - o.y));
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    if let Some(d) = this.drag.take() {
                        let snap = this.rt.snapshot();
                        let placed = layout_monitors(&snap.monitors, PREVIEW_W, PREVIEW_H);
                        if let Some(rect) = this.zone_from_drag(&placed, &d) {
                            this.rt.change(move |c| model::set_zone(c, rect));
                        }
                        cx.notify();
                    }
                }),
            );

        let presets = div().flex().flex_wrap().gap_1().children(PRESETS.iter().enumerate().map(|(i, preset)| {
            let monitor = snap.monitors.iter().find(|m| m.is_primary).or(snap.monitors.first()).cloned();
            button(preset.label, false).id(("preset", i)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                let rect = (PRESETS[i].rect)(monitor.as_ref());
                this.rt.change(move |c| model::set_zone(c, rect));
                cx.notify();
            }))
        }));

        let app_list = apps.iter().enumerate().map(|(i, row)| {
            let name = row.process_name.clone();
            let on = row.applied;
            div()
                .flex()
                .items_center()
                .gap_2()
                .min_h(px(30.))
                .child(button(if on { "On" } else { "Off" }, on).id(("app", i)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    let name = name.clone();
                    this.rt.change(move |c| model::set_app_enabled(c, &name, !on));
                    cx.notify();
                })))
                .child(div().w(px(8.)).h(px(8.)).rounded(px(4.)).bg(rgba(if row.running { 0x6fe08aff } else { 0xffffff30 })))
                .child(div().flex_1().min_w_0().text_sm().child(if row.title.is_empty() { row.process_name.clone() } else { format!("{} ({})", row.title, row.process_name) }))
        });

        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
        let activity = snap.activity.iter().rev().take(6).map(|a| {
            div().flex().gap_2().text_xs().child(div().w(px(70.)).text_color(rgba(0xffffff70)).child(ago(now, a.timestamp_ms))).child(div().flex_1().child(a.message.clone()))
        });

        let autostart_on = desqu_native_shell::autostart::is_enabled(&crate::ID);
        let section = |title: &str| div().text_sm().text_color(rgba(0xffffffb0)).pt_2().child(title.to_string());

        div()
            .id("screenbound")
            .size_full()
            .p(px(24.))
            .bg(rgba(0x14171eff))
            .text_color(rgba(0xffffffff))
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().text_lg().child("Screenbound"))
                    .child(div().text_xs().px_2().py(px(2.)).rounded(px(4.)).bg(rgba(if enabled { 0x2f8f4f80 } else { 0xffb03060 })).child(if enabled { "On" } else { "Paused" }))
                    .child(div().flex_1())
                    .child(button("Release all windows", false).id("release").on_click(cx.listener(|this, _: &ClickEvent, _, _| this.rt.release_all())))
                    .child(button(if enabled { "Pause" } else { "Resume" }, false).id("pause").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.rt.change(move |c| {
                            c.enabled = !enabled;
                            true
                        });
                        cx.notify();
                    }))),
            )
            .child(div().text_xs().text_color(rgba(0xffffff80)).child("When an app goes fullscreen, it stays inside the zone. Everything else stays visible."))
            .child(section("Zone: drag on a monitor, or pick a preset"))
            .child(preview)
            .child(presets)
            .child(section("Apps that stay inside the zone"))
            .child(
                div()
                    .id("apps")
                    .h(px(150.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .when(apps.is_empty(), |d| d.child(div().text_xs().text_color(rgba(0xffffff80)).child("Open the apps you want to manage: they appear here.")))
                    .children(app_list),
            )
            .child(section(&format!("Managed now: {}", managed.len())))
            .children(managed.iter().take(4).map(|m| div().text_xs().child(format!("{} ({:?})", m.process_name, m.kind))))
            .when(!self.embedded, |d| {
                d.child(div().flex().child(button("Groups, rules and browser...", false).id("manage").on_click(cx.listener(|this, _: &ClickEvent, _, cx| crate::manage::open(cx, this.rt.clone())))))
            })
            .child(section("Activity"))
            .children(activity)
            .when(!self.note.is_empty(), |d| d.child(div().text_xs().text_color(rgba(0xffb870ff)).child(self.note.clone())))
            .when(!self.embedded, |d| {
                d.child(div().flex_1())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(div().flex_1().text_sm().child("Start with Windows"))
                        .child(button(if autostart_on { "On" } else { "Off" }, autostart_on).id("autostart").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.note = match desqu_native_shell::autostart::set(&crate::ID, "", !autostart_on) {
                                Ok(()) => String::new(),
                                Err(e) => e,
                            };
                            cx.notify();
                        }))),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(div().flex_1().text_sm().child(if self.update_note.is_empty() { "Updates".to_string() } else { self.update_note.clone() }))
                        .when_some(self.update_url.clone(), |d, url| d.child(button("Download", true).id("download").on_click(move |_, _, _| open_url(&url))))
                        .child(button("Check now", false).id("check").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.check_update(cx)))),
                )
            })
    }
}

fn open_url(url: &str) {
    let _ = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
}

/// The view, redrawing when the snapshot changes (a window went fullscreen, a monitor changed).
fn create(rt: Arc<dyn Backend>, embedded: bool, cx: &mut App) -> Entity<ScreenboundView> {
    cx.new(|cx| {
        let watched = rt.clone();
        cx.spawn(async move |this, cx| {
            let mut seen = watched.generation();
            loop {
                cx.background_executor().timer(std::time::Duration::from_millis(300)).await;
                let now = watched.generation();
                if now != seen {
                    seen = now;
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        return;
                    }
                }
            }
        })
        .detach();
        ScreenboundView { rt: rt.clone(), embedded, origin: Rc::new(Cell::new(Point::default())), drag: None, note: String::new(), update_note: String::new(), update_url: None, merged: 0 }
    })
}

/// The view for the Desqu window, without the rows that only make sense in the app's own window.
pub fn embedded(rt: Arc<dyn Backend>, cx: &mut App) -> Entity<ScreenboundView> {
    create(rt, true, cx)
}

/// Opens the window, or brings the open one to the front.
pub fn open(cx: &mut App, rt: Arc<dyn Backend>) {
    if let Some(Open(Some(handle))) = cx.try_global::<Open>().map(|o| Open(o.0)) {
        if handle.update(cx, |_, window, _| window.activate_window()).is_ok() {
            return;
        }
    }
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(160.), px(60.)), size(px(720.), px(900.))))),
            titlebar: Some(gpui::TitlebarOptions { title: Some("Screenbound".into()), ..Default::default() }),
            focus: true,
            window_background: WindowBackgroundAppearance::Opaque,
            ..Default::default()
        },
        move |_, cx| create(rt, false, cx),
    );
    if let Ok(handle) = opened {
        cx.set_global(Open(Some(handle.into())));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenbound_module::core::geometry::Rect;
    use screenbound_module::core::monitor::MonitorId;

    fn mon(id: &str, l: i32, t: i32, r: i32, b: i32) -> MonitorInfo {
        MonitorInfo {
            id: MonitorId(id.into()),
            device_name: id.into(),
            friendly_name: id.into(),
            number: 1,
            is_primary: false,
            bounds: Rect::new(l, t, r, b),
            work_area: Rect::new(l, t, r, b),
            dpi: 96,
        }
    }

    #[test]
    fn monitors_keep_their_arrangement_and_fit_the_preview() {
        // A wide monitor and a smaller one to its left, as on a real desk.
        let placed = layout_monitors(&[mon("a", 0, 0, 3840, 1080), mon("b", -1920, 0, 0, 1080)], 640.0, 150.0);
        assert_eq!(placed.len(), 2);
        let (a, b) = (placed[0], placed[1]);
        assert!(b.x < a.x, "the left monitor is drawn left");
        assert!((b.x + b.w - a.x).abs() < 0.01, "and they touch");
        assert!(a.w / a.h > 3.0 && (a.h - b.h).abs() < 0.01);
        assert!(a.x + a.w <= 640.5 && a.h <= 150.5, "everything fits the box");
        assert!(layout_monitors(&[], 640.0, 150.0).is_empty());
    }

    #[test]
    fn times_are_shown_as_how_long_ago() {
        assert_eq!(ago(10_000, 9_000), "just now");
        assert_eq!(ago(100_000, 70_000), "30s ago");
        assert_eq!(ago(1_000_000, 700_000), "5m ago");
        assert_eq!(ago(10_000_000, 1_000_000), "2h ago");
        assert_eq!(ago(1, 5), "just now", "a clock that went backwards is not negative");
    }
}
