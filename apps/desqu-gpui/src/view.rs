//! The Desqu window, laid out like the Tauri Desqu: a sidebar (Home, the apps, Settings) and a page.
//! Home has one card per app (on/off, start with system, open its window); an app's page has the
//! same controls plus its log; Settings has start-up, updates and about.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use gpui::{
    div, point, prelude::*, px, rgb, rgba, size, AnyWindowHandle, App, Bounds, ClickEvent, Context, FontWeight, Global, IntoElement, Render,
    SharedString, Window, WindowBackgroundAppearance, WindowBounds, WindowOptions,
};

use crate::modules::{Kind, Settings, TriState, ALL};
use crate::supervisor::{Status, Supervisor};

// The Tauri app's colours (packages/ui/src/tokens.css).
const SURFACE_0: u32 = 0x111317;
const SURFACE_1: u32 = 0x181b20;
const SURFACE_2: u32 = 0x22262d;
const BORDER: u32 = 0x2a2f37;
const BORDER_STRONG: u32 = 0x3a414c;
const TEXT: u32 = 0xe6e8eb;
const TEXT_DIM: u32 = 0x8b939e;
const ACCENT: u32 = 0x4fa3ff;
/// The accent at 16 %.
const ACCENT_SOFT: u32 = 0x4fa3ff29;
const WARN: u32 = 0xe8b04a;
const ERROR: u32 = 0xff6b6b;
const GREEN: u32 = 0x4cc38a;

const SIDEBAR: f32 = 190.;
const CARD_MIN: f32 = 320.;
const GAP: f32 = 12.;
const PAGE_PAD: f32 = 20.;

#[derive(Default)]
struct Open(Option<AnyWindowHandle>);
impl Global for Open {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Home,
    App(Kind),
    Settings,
}

/// What a status is called in the window.
pub fn status_text(status: Status) -> &'static str {
    match status {
        Status::Off => "Off",
        Status::Running => "On",
        Status::RunningElsewhere => "Running on its own",
        Status::Stopped => "Stopped unexpectedly",
        Status::Missing => "Not installed",
    }
}

fn status_color(status: Status) -> u32 {
    match status {
        Status::Running => GREEN,
        Status::RunningElsewhere => ACCENT,
        Status::Stopped => WARN,
        Status::Off | Status::Missing => TEXT_DIM,
    }
}

/// Desqu's own startup entry exists exactly when some app that is on starts with Windows. The entry
/// passes `--autostart`, so a sign-in start stays in the tray and starts only those apps.
pub fn sync_autostart(settings: &Settings) {
    let wanted = settings.should_autostart();
    if desqu_native_shell::autostart::is_enabled(&crate::ID) != wanted {
        if let Err(e) = desqu_native_shell::autostart::set(&crate::ID, "--autostart", wanted) {
            eprintln!("could not change the startup entry: {e}");
        }
    }
}

/// A plain button (the Tauri `button`).
fn button(id: impl Into<SharedString>, label: impl Into<SharedString>, enabled: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id.into())
        .px(px(14.))
        .py(px(4.))
        .rounded(px(4.))
        .border_1()
        .border_color(rgb(BORDER_STRONG))
        .bg(rgb(SURFACE_2))
        .text_color(rgb(TEXT))
        .child(label.into())
        .when(enabled, |d| d.cursor_pointer().hover(|s| s.border_color(rgb(ACCENT))))
        .when(!enabled, |d| d.opacity(0.45))
}

/// The on/off switch (the Tauri `Switch` / `TriStateSwitch`): 42 x 24, a knob that sits left, right,
/// or in the middle for "some".
fn switch(id: impl Into<SharedString>, state: TriState, enabled: bool) -> gpui::Stateful<gpui::Div> {
    let (bg, border, knob_x, knob) = match state {
        TriState::On => (rgba((ACCENT << 8) | 0xff), rgba(0x7cbcffff), 20., 0xffffffff),
        TriState::Mixed => (rgba(ACCENT_SOFT), rgba((ACCENT << 8) | 0xff), 11., 0xd5d8deff),
        TriState::Off => (rgba(0x0d0f13ff), rgba((BORDER_STRONG << 8) | 0xff), 2., 0xd5d8deff),
    };
    div()
        .id(id.into())
        .relative()
        .flex_none()
        .w(px(42.))
        .h(px(24.))
        .rounded(px(12.))
        .border_1()
        .border_color(border)
        .bg(bg)
        .child(div().absolute().top(px(2.)).left(px(knob_x)).size(px(18.)).rounded(px(9.)).bg(rgba(knob)))
        .when(enabled, |d| d.cursor_pointer())
        .when(!enabled, |d| d.opacity(0.45))
}

fn on_off(on: bool) -> TriState {
    if on {
        TriState::On
    } else {
        TriState::Off
    }
}

fn dim(text: impl Into<SharedString>) -> gpui::Div {
    div().text_color(rgb(TEXT_DIM)).child(text.into())
}

fn card() -> gpui::Div {
    div().flex().flex_col().gap(px(10.)).p(px(12.)).rounded(px(6.)).border_1().border_color(rgb(BORDER)).bg(rgb(SURFACE_1))
}

fn heading(text: &'static str) -> gpui::Div {
    div().text_size(px(20.)).font_weight(FontWeight::BOLD).child(text)
}

fn card_title(text: &'static str) -> gpui::Div {
    div().font_weight(FontWeight::SEMIBOLD).child(text)
}

pub struct Shell {
    pub supervisor: Arc<Supervisor>,
    pub settings_path: PathBuf,
    pub settings: std::sync::Mutex<Settings>,
}

struct DesquView {
    shell: Arc<Shell>,
    page: Page,
    /// Apps being started or stopped: their switches wait.
    busy: HashSet<Kind>,
    note: String,
    update_note: String,
    update_url: Option<String>,
}

impl DesquView {
    fn settings(&self) -> Settings {
        self.shell.settings.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Changes the settings, saves them and brings the startup entry in line.
    fn edit(&mut self, change: impl FnOnce(&mut Settings)) {
        let mut s = self.shell.settings.lock().unwrap_or_else(|e| e.into_inner());
        change(&mut s);
        if let Err(e) = s.save(&self.shell.settings_path) {
            self.note = format!("could not save the settings: {e}");
        }
        sync_autostart(&s);
    }

    fn toggle(&mut self, kind: Kind, cx: &mut Context<Self>) {
        if self.busy.contains(&kind) {
            return;
        }
        let on = !self.settings().enabled(kind);
        self.note.clear();
        self.edit(|s| s.set(kind, on));
        self.busy.insert(kind);
        let shell = self.shell.clone();
        // Starting and stopping can take a moment (a stop waits for the app to put things back).
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    if on {
                        shell.supervisor.start(kind)
                    } else {
                        shell.supervisor.stop(kind);
                        Ok(())
                    }
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.busy.remove(&kind);
                if let Err(e) = result {
                    view.note = e;
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn toggle_start(&mut self, kind: Kind, cx: &mut Context<Self>) {
        let next = !self.settings().start_with_system(kind);
        self.edit(|s| {
            s.set_start_with_system(kind, next);
        });
        cx.notify();
    }

    fn toggle_all_start(&mut self, cx: &mut Context<Self>) {
        // From "some" or "none" it turns everything on, like the Tauri switch.
        let next = self.settings().all_start() != TriState::On;
        self.edit(|s| s.set_all_start_with_system(next));
        cx.notify();
    }

    fn open_app(&mut self, kind: Kind, cx: &mut Context<Self>) {
        let status = self.shell.supervisor.status(kind);
        self.note = if !matches!(status, Status::Running | Status::RunningElsewhere) {
            format!("Turn {} on first", kind.name())
        } else if !self.shell.supervisor.open(kind) {
            format!("{} did not answer", kind.name())
        } else {
            String::new()
        };
        cx.notify();
    }

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

    // ------------------------------------------------------------------------------- sidebar

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let item = |id: SharedString, active: bool| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(10.))
                .py(px(7.))
                .rounded(px(6.))
                .border_1()
                .border_color(if active { rgba((BORDER_STRONG << 8) | 0xff) } else { rgba(0) })
                .when(active, |d| d.bg(rgb(SURFACE_2)))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(SURFACE_2)))
        };
        let apps = ALL.into_iter().map(|kind| {
            let status = self.shell.supervisor.status(kind);
            let dot = match status {
                Status::Running | Status::RunningElsewhere | Status::Stopped => status_color(status),
                _ => BORDER_STRONG,
            };
            item(SharedString::from(format!("nav-{}", kind.name())), self.page == Page::App(kind))
                .child(div().size(px(8.)).rounded(px(4.)).bg(rgb(dot)))
                .child(kind.name())
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.page = Page::App(kind);
                    this.note.clear();
                    cx.notify();
                }))
        });
        div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .w(px(SIDEBAR))
            .flex_none()
            .px(px(8.))
            .py(px(12.))
            .bg(rgb(SURFACE_1))
            .border_r_1()
            .border_color(rgb(BORDER))
            .child(div().px(px(10.)).pt(px(4.)).pb(px(12.)).text_size(px(16.)).font_weight(FontWeight::BOLD).child("Desqu"))
            .child(item("nav-home".into(), self.page == Page::Home).child("Home").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.page = Page::Home;
                cx.notify();
            })))
            .child(div().px(px(10.)).pt(px(12.)).pb(px(4.)).text_size(px(11.)).text_color(rgb(TEXT_DIM)).child("APPS"))
            .children(apps)
            .child(div().flex_1())
            .child(item("nav-settings".into(), self.page == Page::Settings).child("Settings").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.page = Page::Settings;
                this.note.clear();
                cx.notify();
            })))
    }

    // ---------------------------------------------------------------------------------- pages

    /// The on switch and the "start with system" switch of one app, shared by its card and its page.
    fn switches(&self, kind: Kind, settings: &Settings, cx: &mut Context<Self>) -> (gpui::Stateful<gpui::Div>, gpui::Div) {
        let status = self.shell.supervisor.status(kind);
        let busy = self.busy.contains(&kind);
        let usable = status != Status::Missing && !busy;
        let enabled = settings.enabled(kind);
        let on = switch(SharedString::from(format!("on-{}", kind.name())), on_off(enabled), usable)
            .when(usable, |d| d.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle(kind, cx))));
        let start_usable = usable && enabled;
        let start = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                switch(SharedString::from(format!("start-{}", kind.name())), on_off(settings.start_with_system(kind)), start_usable)
                    .when(start_usable, |d| d.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_start(kind, cx)))),
            )
            .child(dim(if enabled { "Start with system" } else { "Start with system (turn the app on first)" }));
        (on, start)
    }

    fn app_card(&self, kind: Kind, settings: &Settings, cx: &mut Context<Self>) -> gpui::Div {
        let status = self.shell.supervisor.status(kind);
        let (on, start) = self.switches(kind, settings, cx);
        let alive = matches!(status, Status::Running | Status::RunningElsewhere);
        card()
            .when(status == Status::Missing, |d| d.opacity(0.6))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .size(px(38.))
                            .rounded(px(10.))
                            .bg(rgba(ACCENT_SOFT))
                            .border_1()
                            .border_color(rgb(ACCENT))
                            .text_color(rgb(ACCENT))
                            .text_size(px(18.))
                            .font_weight(FontWeight::BOLD)
                            .child(&kind.name()[..1]),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(kind.name()))
                            .child(div().text_size(px(12.)).text_color(rgb(status_color(status))).child(status_text(status))),
                    )
                    .child(on),
            )
            .child(dim(kind.description()))
            .child(
                div().flex().items_center().justify_between().gap(px(10.)).child(start).child(
                    button(SharedString::from(format!("open-{}", kind.name())), "Open", alive)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open_app(kind, cx))),
                ),
            )
    }

    fn home(&self, window: &Window, cx: &mut Context<Self>) -> gpui::Div {
        let settings = self.settings();
        let all = settings.all_start();
        let any_on = ALL.into_iter().any(|k| settings.enabled(k));
        // A grid like CSS `repeat(auto-fill, minmax(320px, 1fr))`.
        let width = f32::from(window.viewport_size().width) - SIDEBAR - 2. * PAGE_PAD;
        let columns = (((width + GAP) / (CARD_MIN + GAP)).floor() as usize).max(1);
        let cards: Vec<Kind> = ALL.to_vec();
        let mut rows = Vec::new();
        for row in cards.chunks(columns) {
            let mut line = div().flex().gap(px(GAP));
            for kind in row {
                line = line.child(self.app_card(*kind, &settings, cx).flex_1().min_w(px(0.)));
            }
            for _ in row.len()..columns {
                line = line.child(div().flex_1());
            }
            rows.push(line);
        }
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(16.))
                    .child(div().child(heading("Your apps")).child(dim("Turn on what you need. Each app runs on its own, and Desqu keeps them together.")))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(switch("start-all", all, any_on).when(any_on, |d| d.on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_all_start(cx)))))
                            .child(dim("Start all apps with system")),
                    ),
            )
            .children(rows)
    }

    fn app_page(&self, kind: Kind, cx: &mut Context<Self>) -> gpui::Div {
        let settings = self.settings();
        let status = self.shell.supervisor.status(kind);
        let alive = matches!(status, Status::Running | Status::RunningElsewhere);
        let (on, start) = self.switches(kind, &settings, cx);
        let explain = match status {
            Status::Running => format!("{} is running. Its settings are in its own window.", kind.name()),
            Status::RunningElsewhere => format!("A copy of {} that Desqu did not start is running. Desqu leaves it alone.", kind.name()),
            Status::Stopped => format!("{} ended unexpectedly. Its log says why; turning it off and on starts it again.", kind.name()),
            Status::Off => format!("{} is off. Turn it on to use it.", kind.name()),
            Status::Missing => format!("{} is not installed next to Desqu.", kind.exe()),
        };
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .max_w(px(760.))
            .child(heading(kind.name()))
            .child(dim(kind.description()))
            .child(
                card()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(div().flex_1().child(card_title("Running")).child(div().text_size(px(12.)).text_color(rgb(status_color(status))).child(status_text(status))))
                            .child(on),
                    )
                    .child(start)
                    .child(dim(explain))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(
                                button(SharedString::from(format!("page-open-{}", kind.name())), format!("Open {}", kind.name()), alive)
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open_app(kind, cx))),
                            )
                            .child(button("page-log", "Show its log", true).on_click(move |_, _, _| open_path(&crate::supervisor::log_path(kind)))),
                    ),
            )
    }

    fn settings_page(&self, cx: &mut Context<Self>) -> gpui::Div {
        let settings = self.settings();
        let starts = settings.should_autostart();
        let marked: Vec<&str> = ALL.into_iter().filter(|k| settings.start_with_system(*k)).map(Kind::name).collect();
        let logs = crate::supervisor::log_path(Kind::Vials).parent().map(PathBuf::from).unwrap_or_default();
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .max_w(px(760.))
            .child(heading("Settings"))
            .child(
                card()
                    .gap(px(6.))
                    .child(card_title("Start with system"))
                    .child(dim(if starts {
                        format!("Desqu starts with Windows and starts {}.", marked.join(", "))
                    } else {
                        "Desqu does not start with Windows.".to_string()
                    }))
                    .child(dim(
                        "This follows your apps: Desqu starts when at least one app that is on has Start with system, and then only those apps start. Change it on the Home page.",
                    )),
            )
            .child(
                card()
                    .gap(px(6.))
                    .child(card_title("Updates"))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(dim(if self.update_note.is_empty() { format!("Version {}", env!("CARGO_PKG_VERSION")) } else { self.update_note.clone() }).flex_1())
                            .when_some(self.update_url.clone(), |d, url| d.child(button("download", "Download", true).on_click(move |_, _, _| open_url(&url))))
                            .child(button("check", "Check now", true).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.check_update(cx)))),
                    ),
            )
            .child(
                card()
                    .gap(px(6.))
                    .child(card_title("About"))
                    .child(div().flex().gap(px(24.)).child("Desqu").child(dim(env!("CARGO_PKG_VERSION"))))
                    .child(dim("Each app keeps its output in the logs folder, which helps when one stops unexpectedly."))
                    .child(div().flex().child(button("logs", "Open the logs folder", true).on_click(move |_, _, _| open_path(&logs)))),
            )
            .child(div().flex().child(button("quit", "Quit Desqu and its apps", true).on_click(|_, _, cx| cx.quit())))
    }
}

impl Render for DesquView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = match self.page {
            Page::Home => self.home(window, cx),
            Page::App(kind) => self.app_page(kind, cx),
            Page::Settings => self.settings_page(cx),
        };
        div()
            .size_full()
            .flex()
            .bg(rgb(SURFACE_0))
            .text_color(rgb(TEXT))
            .text_size(px(13.))
            .font_family("Segoe UI")
            .child(self.sidebar(cx))
            .child(
                div()
                    .id("main")
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .overflow_y_scroll()
                    .when(!self.note.is_empty(), |d| {
                        d.child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .px(px(14.))
                                .py(px(6.))
                                .bg(rgba(0xff6b6b1f))
                                .text_color(rgb(ERROR))
                                .child(self.note.clone())
                                .child(button("dismiss", "Dismiss", true).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.note.clear();
                                    cx.notify();
                                }))),
                        )
                    })
                    .child(div().px(px(PAGE_PAD)).py(px(18.)).child(page)),
            )
    }
}

fn open_url(url: &str) {
    let _ = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
}

/// Opens a file or folder the way Explorer does (a log opens in the text editor).
fn open_path(path: &std::path::Path) {
    let _ = std::process::Command::new("explorer.exe").arg(path).spawn();
}

/// Opens the window, or brings the open one to the front.
pub fn open(cx: &mut App, shell: Arc<Shell>) {
    if let Some(Open(Some(handle))) = cx.try_global::<Open>().map(|o| Open(o.0)) {
        if handle.update(cx, |_, window, _| window.activate_window()).is_ok() {
            return;
        }
    }
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(160.), px(80.)), size(px(1100.), px(760.))))),
            titlebar: Some(gpui::TitlebarOptions { title: Some("Desqu".into()), ..Default::default() }),
            focus: true,
            window_background: WindowBackgroundAppearance::Opaque,
            window_min_size: Some(size(px(560.), px(420.))),
            ..Default::default()
        },
        move |_, cx| {
            cx.new(|cx| {
                // App states change on their own (one ends, one is started by hand): look again now and then.
                cx.spawn(async move |this, cx| loop {
                    cx.background_executor().timer(std::time::Duration::from_secs(2)).await;
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        return;
                    }
                })
                .detach();
                DesquView { shell, page: Page::Home, busy: HashSet::new(), note: String::new(), update_note: String::new(), update_url: None }
            })
        },
    );
    if let Ok(handle) = opened {
        cx.set_global(Open(Some(handle.into())));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_status_has_a_plain_description() {
        for s in [Status::Off, Status::Running, Status::RunningElsewhere, Status::Stopped, Status::Missing] {
            assert!(!status_text(s).is_empty());
        }
        assert_eq!(status_text(Status::Running), "On", "the same word as the Tauri Desqu");
        assert!(status_text(Status::Stopped).contains("unexpectedly"));
    }
}
