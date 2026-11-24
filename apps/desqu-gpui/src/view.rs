//! The Desqu window, laid out like the Tauri Desqu: a sidebar (Home, the apps, Settings) and a page.
//! Home is a list with one row per app (on/off switch, a chevron to its page); an app's page has
//! start with system, its log and its settings; Settings has start-up, updates and about.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use gpui::{
    div, point, prelude::*, px, rgb, rgba, size, AnyWindowHandle, App, Bounds, ClickEvent, Context, FontWeight, Global, IntoElement, Render,
    SharedString, Window, WindowBackgroundAppearance, WindowBounds, WindowOptions,
};

use screenbound_module::core::config::AppConfig;
use crate::modules::{Kind, Settings, TriState, ALL};
use crate::supervisor::{Status, Supervisor};

/// What Screenbound's page holds: the views of the app itself, fed from its saved settings.
struct ScreenboundViews {
    remote: Arc<crate::screenbound_remote::Remote>,
    main: gpui::Entity<screenbound_gpui::view::ScreenboundView>,
    manage: gpui::Entity<screenbound_gpui::manage::ManageView>,
}

impl Drop for ScreenboundViews {
    fn drop(&mut self) {
        self.remote.stop();
    }
}

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

/// An icon from `assets/icons`, in the colour of the text.
fn icon(path: &'static str, size: f32, color: u32) -> gpui::Svg {
    gpui::svg().path(path).size(px(size)).flex_none().text_color(rgb(color))
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
    /// The Workspaces item whose desktop is being chosen.
    picked: Option<String>,
    /// Screenbound's own views, made the first time its page is opened.
    screenbound: Option<ScreenboundViews>,
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

    /// What the on/off switch shows: on only while the app is wanted and has not stopped by itself.
    fn shown_on(&self, kind: Kind) -> bool {
        self.settings().enabled(kind) && self.shell.supervisor.status(kind) != Status::Stopped
    }

    fn toggle(&mut self, kind: Kind, cx: &mut Context<Self>) {
        if self.busy.contains(&kind) {
            return;
        }
        // An app that ended by itself shows as off, so the next click turns it on (starts it again).
        let on = !self.shown_on(kind);
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
                .child(icon(kind.icon(), 16., TEXT))
                .child(div().flex_1().child(kind.name()))
                .child(div().size(px(7.)).rounded(px(4.)).bg(rgb(dot)))
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
            .child(item("nav-home".into(), self.page == Page::Home).child(icon("icons/house.svg", 16., TEXT)).child("Home").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.page = Page::Home;
                cx.notify();
            })))
            .child(div().px(px(10.)).pt(px(12.)).pb(px(4.)).text_size(px(11.)).text_color(rgb(TEXT_DIM)).child("APPS"))
            .children(apps)
            .child(div().flex_1())
            .child(item("nav-settings".into(), self.page == Page::Settings).child(icon("icons/settings.svg", 16., TEXT)).child("Settings").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
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
        let on = switch(SharedString::from(format!("on-{}", kind.name())), on_off(self.shown_on(kind)), usable)
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

    /// One app in the list on Home: its name and what it does, the switch, and a chevron to its page.
    fn app_row(&self, kind: Kind, last: bool, cx: &mut Context<Self>) -> gpui::Div {
        let settings = self.settings();
        let status = self.shell.supervisor.status(kind);
        let (on, _) = self.switches(kind, &settings, cx);
        let name = kind.name();
        div()
            .flex()
            .items_center()
            .gap(px(12.))
            .px(px(14.))
            .min_h(px(64.))
            .when(!last, |d| d.border_b_1().border_color(rgb(BORDER)))
            .when(status == Status::Missing, |d| d.opacity(0.6))
            .child(
                div()
                    .id(SharedString::from(format!("row-{name}")))
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .py(px(10.))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.page = Page::App(kind);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .size(px(36.))
                            .rounded(px(8.))
                            .bg(rgba(ACCENT_SOFT))
                            .border_1()
                            .border_color(rgb(ACCENT))
                            .child(icon(kind.icon(), 20., ACCENT)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .child(
                                div()
                                    .flex()
                                    .items_baseline()
                                    .gap(px(8.))
                                    .child(div().font_weight(FontWeight::SEMIBOLD).child(name))
                                    .child(div().text_size(px(12.)).text_color(rgb(status_color(status))).child(status_text(status))),
                            )
                            .child(div().text_size(px(12.)).text_color(rgb(TEXT_DIM)).overflow_hidden().whitespace_nowrap().text_ellipsis().child(kind.description())),
                    ),
            )
            .child(on)
            .child(
                div()
                    .id(SharedString::from(format!("chevron-{name}")))
                    .flex_none()
                    .w(px(24.))
                    .cursor_pointer()
                    .child(icon("icons/chevron-right.svg", 18., TEXT_DIM))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.page = Page::App(kind);
                        cx.notify();
                    })),
            )
    }

    fn home(&self, cx: &mut Context<Self>) -> gpui::Div {
        let settings = self.settings();
        let all = settings.all_start();
        let any_on = ALL.into_iter().any(|k| settings.enabled(k));
        let rows: Vec<gpui::Div> = ALL.iter().enumerate().map(|(i, kind)| self.app_row(*kind, i + 1 == ALL.len(), cx)).collect();
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .max_w(px(900.))
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
                            .child(switch("start-all", on_off(all == TriState::On), any_on).when(any_on, |d| d.on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_all_start(cx)))))
                            .child(dim("Start all apps with system")),
                    ),
            )
            .child(div().flex().flex_col().rounded(px(6.)).border_1().border_color(rgb(BORDER)).bg(rgb(SURFACE_1)).children(rows))
    }

    fn app_page(&self, kind: Kind, cx: &mut Context<Self>) -> gpui::Div {
        let settings = self.settings();
        let status = self.shell.supervisor.status(kind);
        let (on, start) = self.switches(kind, &settings, cx);
        let explain = match status {
            Status::Running => format!("{} is running. Its settings are below; its own window has the rest.", kind.name()),
            Status::RunningElsewhere => format!("A copy of {} that Desqu did not start is running. Desqu leaves it alone.", kind.name()),
            Status::Stopped => format!("{} ended unexpectedly. Its log says why; turn it on to start it again.", kind.name()),
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
                            .child(button("page-log", "Show its log", true).on_click(move |_, _, _| open_path(&crate::supervisor::log_path(kind)))),
                    ),
            )
            .when_some(self.options(kind, cx), |d, options| d.child(options))
    }

    /// Changes one option of an app: the running app takes it (and saves it), an app that is off has
    /// its file changed. The page looks again a moment later, once the app has saved.
    fn set_option(&mut self, kind: Kind, key: &'static str, value: String, cx: &mut Context<Self>) {
        let running = matches!(self.shell.supervisor.status(kind), Status::Running | Status::RunningElsewhere);
        if let Err(e) = crate::app_settings::apply(kind, running, key, &value) {
            self.note = e;
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(400)).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    fn workspaces_running(&self) -> bool {
        matches!(self.shell.supervisor.status(Kind::Workspaces), Status::Running | Status::RunningElsewhere)
    }

    /// Looks at the page again now and once more shortly, after the app has had time to save.
    fn refresh_soon(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(500)).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    fn move_item(&mut self, item: String, desktop: Option<String>, cx: &mut Context<Self>) {
        self.picked = None;
        if let Err(e) = crate::workspaces_page::assign(self.workspaces_running(), &item, desktop.as_deref()) {
            self.note = e;
        }
        self.refresh_soon(cx);
    }

    /// One item of a desktop: click it to choose another desktop for it.
    fn item_chip(&self, item: &crate::workspaces_page::Item, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let id = item.id.clone();
        div()
            .id(SharedString::from(format!("ws-item-{}", item.id)))
            .px(px(8.))
            .py(px(2.))
            .rounded(px(4.))
            .bg(rgb(SURFACE_2))
            .text_size(px(12.))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(BORDER_STRONG)))
            .child(item.label.clone())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.picked = Some(id.clone());
                cx.notify();
            }))
    }

    /// The virtual desktops and their items: tie an item to a desktop, or show it on every desktop.
    fn workspaces_section(&self, cx: &mut Context<Self>) -> gpui::Div {
        let page = crate::workspaces_page::load();
        let running = self.workspaces_running();
        let mut out = div().flex().flex_col().gap(px(10.));
        if !page.has_desktops {
            return out.child(card().child(dim("Windows lists virtual desktops only once you have more than one: press Win+Tab and choose New desktop.")));
        }
        if page.first_run {
            out = out.child(
                card()
                    .child(card_title("Which virtual desktop do the items on your desktop belong to?"))
                    .child(dim(if running { "Nothing is moved or deleted: items stay in your Desktop folders." } else { "Turn Workspaces on to answer this." }))
                    .when(running, |d| {
                        d.child(div().flex().flex_wrap().gap(px(6.)).children(page.cards.iter().filter(|c| !c.removed).map(|c| {
                            let id = c.id.clone();
                            button(SharedString::from(format!("first-{id}")), c.name.clone(), true).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                if let Err(e) = crate::workspaces_page::first_run(Some(&id)) {
                                    this.note = e;
                                }
                                this.refresh_soon(cx);
                            }))
                        })))
                        .child(div().child(button("first-none", "Leave them on every desktop", true).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            if let Err(e) = crate::workspaces_page::first_run(None) {
                                this.note = e;
                            }
                            this.refresh_soon(cx);
                        }))))
                    }),
            );
        }
        if let Some(item) = self.picked.clone() {
            let label = page
                .shared
                .iter()
                .chain(page.cards.iter().flat_map(|c| c.items.iter()))
                .find(|i| i.id == item)
                .map(|i| i.label.clone())
                .unwrap_or_else(|| item.clone());
            out = out.child(
                card()
                    .border_color(rgb(ACCENT))
                    .child(div().child(format!("Show \"{label}\" on:")))
                    .child(div().flex().flex_wrap().gap(px(6.)).children(page.cards.iter().filter(|c| !c.removed).map(|c| {
                        let (item, id) = (item.clone(), c.id.clone());
                        button(SharedString::from(format!("to-{id}")), c.name.clone(), true)
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.move_item(item.clone(), Some(id.clone()), cx)))
                    })))
                    .child(
                        div()
                            .flex()
                            .gap(px(6.))
                            .child({
                                let item = item.clone();
                                button("to-all", "Every desktop", true).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.move_item(item.clone(), None, cx)))
                            })
                            .child(button("to-cancel", "Cancel", true).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.picked = None;
                                cx.notify();
                            }))),
                    ),
            );
        }
        for c in &page.cards {
            let count = c.items.len();
            let id = c.id.clone();
            out = out.child(
                card()
                    .when(c.current, |d| d.border_color(rgb(ACCENT)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(c.name.clone()))
                            .when(c.current, |d| d.child(div().px(px(6.)).rounded(px(4.)).bg(rgba(ACCENT_SOFT)).text_size(px(12.)).child("You are here")))
                            .when(c.removed, |d| d.child(div().px(px(6.)).rounded(px(4.)).bg(rgba(0xe8b04a40)).text_size(px(12.)).child("Desktop removed")))
                            .child(div().flex_1())
                            .child(dim(format!("{count} {}", if count == 1 { "item" } else { "items" })))
                            .when(count > 0, |d| {
                                d.child(
                                    button(SharedString::from(format!("release-{id}")), if c.removed { "Release its items" } else { "Show on every desktop" }, true)
                                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                            if let Err(e) = crate::workspaces_page::release(this.workspaces_running(), &id) {
                                                this.note = e;
                                            }
                                            this.refresh_soon(cx);
                                        })),
                                )
                            }),
                    )
                    .when(!c.removed, |d| {
                        let (desktop, on) = (c.id.clone(), c.open_on_visit);
                        d.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10.))
                                .child(
                                    switch(SharedString::from(format!("visit-{desktop}")), on_off(on), true).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        if let Err(e) = crate::workspaces_page::set_open_on_visit(this.workspaces_running(), &desktop, !on) {
                                            this.note = e;
                                        }
                                        this.refresh_soon(cx);
                                    })),
                                )
                                .child(dim("Open its apps and folders the first time I come here")),
                        )
                    })
                    .when(!c.removed, |d| {
                        let desktop = c.id.clone();
                        let (has, label) = match &c.wallpaper {
                            Some(path) => (true, format!("Wallpaper: {}", path.rsplit(['\\', '/']).next().unwrap_or(path))),
                            None => (false, "No wallpaper of its own".to_string()),
                        };
                        d.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(dim(label).flex_1())
                                .child({
                                    let desktop = desktop.clone();
                                    button(SharedString::from(format!("wp-keep-{desktop}")), "Use my wallpaper now", true).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        if let Err(e) = crate::workspaces_page::set_wallpaper(this.workspaces_running(), &desktop, true) {
                                            this.note = e;
                                        }
                                        this.refresh_soon(cx);
                                    }))
                                })
                                .when(has, |d| {
                                    d.child(button(SharedString::from(format!("wp-clear-{}", c.id)), "Remove", true).on_click({
                                        let desktop = c.id.clone();
                                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                                            if let Err(e) = crate::workspaces_page::set_wallpaper(this.workspaces_running(), &desktop, false) {
                                                this.note = e;
                                            }
                                            this.refresh_soon(cx);
                                        })
                                    }))
                                }),
                        )
                    })
                    .when(c.removed, |d| d.child(dim("This desktop no longer exists, so its items are shown everywhere again.")))
                    .when(!c.removed && count == 0, |d| d.child(dim("Nothing is tied to this desktop yet. Click an item below to tie it here.")))
                    .when(count > 0, |d| d.child(div().flex().flex_wrap().gap(px(6.)).children(c.items.iter().take(40).map(|i| self.item_chip(i, cx))))),
            );
        }
        if !page.shared.is_empty() {
            out = out.child(
                card()
                    .child(dim(format!("On every desktop ({}). Click one to tie it to a desktop:", page.shared.len())))
                    .child(div().flex().flex_wrap().gap(px(6.)).children(page.shared.iter().take(60).map(|i| self.item_chip(i, cx)))),
            );
        }
        out
    }

    /// A row with a label, a hint and a switch for an option that is on or off.
    fn option_switch(&self, kind: Kind, key: &'static str, label: &'static str, hint: &'static str, on: bool, cx: &mut Context<Self>) -> gpui::Div {
        div()
            .flex()
            .items_center()
            .gap(px(12.))
            .child(div().flex_1().child(div().child(label)).child(div().text_size(px(12.)).text_color(rgb(TEXT_DIM)).child(hint)))
            .child(
                switch(SharedString::from(format!("opt-{}-{key}", kind.name())), on_off(on), true)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_option(kind, key, (!on).to_string(), cx))),
            )
    }

    /// The options of an app that has some here (Vials, Workspaces); the app's window has the rest.
    fn options(&self, kind: Kind, cx: &mut Context<Self>) -> Option<gpui::Div> {
        use vials_core::model::VialTheme;
        match kind {
            Kind::Vials => {
                let cfg = crate::app_settings::vials();
                let themes = [
                    (VialTheme::Glass, "glass", "Glass"),
                    (VialTheme::Liquid, "liquid", "Liquid"),
                    (VialTheme::Classic, "classic", "Classic"),
                    (VialTheme::Fluent, "fluent", "Fluent dark"),
                    (VialTheme::FluentLight, "fluentLight", "Fluent light"),
                    (VialTheme::System, "system", "Follow Windows"),
                ];
                let look = div().flex().flex_wrap().gap(px(6.)).children(themes.into_iter().map(|(theme, value, label)| {
                    button(SharedString::from(format!("look-{value}")), label, true)
                        .when(cfg.theme == theme, |d| d.border_color(rgb(ACCENT)).bg(rgba(ACCENT_SOFT)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_option(Kind::Vials, "theme", value.to_string(), cx)))
                }));
                let tints = div().flex().gap(px(6.)).children([0u8, 25, 50, 75, 100].into_iter().map(|tint| {
                    button(SharedString::from(format!("tint-{tint}")), tint.to_string(), true)
                        .when(cfg.glass_tint.abs_diff(tint) <= 12, |d| d.border_color(rgb(ACCENT)).bg(rgba(ACCENT_SOFT)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_option(Kind::Vials, "glassTint", tint.to_string(), cx)))
                }));
                let alive = matches!(self.shell.supervisor.status(kind), Status::Running | Status::RunningElsewhere);
                Some(
                    card()
                        .child(card_title("Settings"))
                        .child(div().child("Look of vials"))
                        .child(look)
                        .child(div().child("Glass tint (lower is clearer)"))
                        .child(tints)
                        .child(self.option_switch(kind, "autoSort", "Sort new desktop items", "Put new files into a vial by its rules", cfg.auto_sort, cx))
                        .child(self.option_switch(kind, "hideOnDesktop", "Hide items on the desktop", "Items in a vial leave the desktop", cfg.hide_on_desktop, cx))
                        .child(self.option_switch(kind, "createOnDrag", "Create vials by dragging a box", "Drag over empty desktop", cfg.create_on_drag, cx))
                        .child(self.option_switch(kind, "contextMenu", "Desktop right-click entry", "\"Create vial here\"", cfg.context_menu, cx))
                        .child(self.option_switch(kind, "offerForSelection", "Offer a vial for selected icons", "A box dragged over icons offers \"New vial with these\"", cfg.offer_for_selection, cx))
                        .child(self.option_switch(kind, "hoverOpen", "Open collapsed vials on hover", "A collapsed vial opens while the pointer is over it", cfg.hover_open, cx))
                        .child(self.option_switch(kind, "wallpaperTint", "Colour from the wallpaper", "Vials take a dark shade of the wallpaper", cfg.wallpaper_tint, cx))
                        .child(self.option_switch(kind, "privacyMask", "Privacy mode", "Names and pictures stay hidden until the pointer is over a vial", cfg.privacy_mask, cx))
                        .child(self.option_switch(kind, "hideFromCapture", "Hide vials from screen sharing", "They do not show in shares, recordings or screenshots", cfg.hide_from_capture, cx))
                        .child(card_title("Peek hotkey"))
                        .child(div().flex().flex_wrap().gap(px(6.)).children([("Off", ""), ("Win+Shift+Space", "win+shift+space"), ("Ctrl+Alt+V", "ctrl+alt+v")].into_iter().map(|(label, value)| {
                            button(SharedString::from(format!("peek-{value}")), label, true)
                                .when(cfg.peek_hotkey == value, |d| d.border_color(rgb(ACCENT)).bg(rgba(ACCENT_SOFT)))
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_option(Kind::Vials, "peekHotkey", value.to_string(), cx)))
                        })))
                        .child(self.option_switch(kind, "quickHide", "Quick-hide", "Double-click the empty desktop to hide or show all vials and icons", cfg.quick_hide, cx))
                        .child(
                            div().child(
                                button("vials-rules", "Edit rules...", alive).when(alive, |d| {
                                    d.on_click(cx.listener(|_, _: &ClickEvent, _, _| {
                                        let _ = desqu_native_shell::tray::forward_action(&Kind::Vials.identity(), "rules");
                                    }))
                                }),
                            ),
                        ),
                )
            }
            Kind::Workspaces => {
                let cfg = crate::app_settings::workspaces();
                Some(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .child(card().child(card_title("Settings")).child(self.option_switch(
                            kind,
                            "autoAssignNew",
                            "New files join the desktop you are on",
                            "A file that appears on the desktop belongs to the current virtual desktop",
                            cfg.auto_assign_new,
                            cx,
                        ))
                        .child(self.option_switch(kind, "showName", "Show the desktop's name when you switch", "A short note in the middle of the screen", cfg.show_name, cx))
                        .child(self.option_switch(kind, "jumpHotkeys", "Win+Ctrl+1..9 jumps to a desktop", "Goes straight to desktop 1 to 9", cfg.jump_hotkeys, cx)))
                        .child(card_title("Desktops"))
                        .child(self.workspaces_section(cx)),
                )
            }
            Kind::Screenbound => self.screenbound.as_ref().map(|views| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(card_title("Zone and apps"))
                    .child(div().w_full().min_w(px(0.)).h(px(980.)).rounded(px(6.)).border_1().border_color(rgb(BORDER)).overflow_hidden().child(views.main.clone()))
                    .child(card_title("Groups, rules and browser"))
                    .child(div().w_full().min_w(px(0.)).h(px(660.)).rounded(px(6.)).border_1().border_color(rgb(BORDER)).overflow_hidden().child(views.manage.clone()))
            }),
        }
    }

    /// "Save a report": a zip with versions, a summary of the settings and the tail of each log, on the desktop.
    fn save_report(&mut self, cx: &mut Context<Self>) {
        let Some(folder) = desqu_desktop::desktop_folders().into_iter().next() else {
            self.note = "The Desktop folder was not found".into();
            return;
        };
        let supervisor = self.shell.supervisor.clone();
        self.note = "Saving the report...".into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { crate::report::save(&folder, |kind| supervisor.status(kind)) }).await;
            let _ = this.update(cx, |view, cx| {
                match result {
                    Ok(path) => {
                        view.note = format!("Saved {}", path.display());
                        // Show it in Explorer, selected.
                        let _ = std::process::Command::new("explorer.exe").arg(format!("/select,{}", path.display())).spawn();
                    }
                    Err(e) => view.note = format!("Could not save the report: {e}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Every hotkey the apps use, with a warning for one that is taken twice or that Windows uses.
    fn hotkeys_card(&self) -> gpui::Div {
        use crate::hotkeys::{check, collect, pretty, Problem};
        let entries = collect(&crate::app_settings::vials(), &crate::app_settings::workspaces(), &AppConfig::load(&crate::screenbound_remote::config_path()).unwrap_or_default());
        let problems = check(&entries);
        let mut card = card().gap(px(6.)).child(card_title("Hotkeys"));
        if entries.is_empty() {
            return card.child(dim("No app has a hotkey on. Turn them on on the page of each app: the peek of Vials, the jump of Workspaces, the zone of Screenbound."));
        }
        for (entry, problem) in entries.iter().zip(problems) {
            let keys = if entry.app == "Workspaces" { format!("{}+1 to 9", pretty(entry.keys.trim_end_matches("+1"))) } else { pretty(&entry.keys) };
            card = card.child(
                div()
                    .flex()
                    .flex_col()
                    .child(div().flex().gap(px(8.)).child(div().w(px(110.)).font_weight(FontWeight::SEMIBOLD).child(keys)).child(div().flex_1().child(format!("{}: {}", entry.app, entry.what))))
                    .when_some(problem, |d, p| {
                        d.child(div().pl(px(118.)).text_size(px(12.)).text_color(rgb(WARN)).child(match p {
                            Problem::NotAKey => "These are not keys Windows can use.".to_string(),
                            Problem::Twice(other) => format!("The same keys as {other}: only one of them works."),
                            Problem::Taken(by) => format!("{by}: it may not work, or it may stop that."),
                        }))
                    }),
            );
        }
        card
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
            .child(self.hotkeys_card())
            .child(
                card()
                    .gap(px(6.))
                    .child(card_title("Report a problem"))
                    .child(dim("Saves a zip on your desktop to attach to a bug report: the versions, which apps are running, a summary of the settings (options and counts, not your file names) and the last lines of each app's log. Your user folder is hidden in it."))
                    .child(div().flex().child(button("report", "Save a report", true).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.save_report(cx))))),
            )
            .child(div().flex().child(button("quit", "Quit Desqu and its apps", true).on_click(|_, _, cx| cx.quit())))
    }
}

impl Render for DesquView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Screenbound's views are made the first time its page is shown.
        if self.page == Page::App(Kind::Screenbound) && self.screenbound.is_none() {
            let remote = crate::screenbound_remote::Remote::start();
            let backend: Arc<dyn screenbound_gpui::runtime::Backend> = remote.clone();
            let main = screenbound_gpui::view::embedded(backend.clone(), cx);
            let manage = screenbound_gpui::manage::embedded(backend, cx);
            self.screenbound = Some(ScreenboundViews { remote, main, manage });
        }
        let page = match self.page {
            Page::Home => self.home(cx),
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
                DesquView { shell, page: Page::Home, busy: HashSet::new(), picked: None, screenbound: None, note: String::new(), update_note: String::new(), update_url: None }
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
