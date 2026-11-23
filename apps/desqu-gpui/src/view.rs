//! The Desqu window: one card per module (on/off, open its window, its state) and the few shared settings.

use std::path::PathBuf;
use std::sync::Arc;

use gpui::{
    div, point, prelude::*, px, rgba, size, AnyWindowHandle, App, Bounds, ClickEvent, Context, Global, IntoElement, Render, SharedString, Window,
    WindowBackgroundAppearance, WindowBounds, WindowOptions,
};

use crate::modules::{Kind, Settings, ALL};
use crate::supervisor::{Status, Supervisor};

#[derive(Default)]
struct Open(Option<AnyWindowHandle>);
impl Global for Open {}

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

/// What a status is called in the window.
pub fn status_text(status: Status) -> &'static str {
    match status {
        Status::Off => "Off",
        Status::Running => "Running",
        Status::RunningElsewhere => "Running on its own",
        Status::Stopped => "Stopped: it ended unexpectedly",
        Status::Missing => "Not installed",
    }
}

pub struct Shell {
    pub supervisor: Arc<Supervisor>,
    pub settings_path: PathBuf,
    pub settings: std::sync::Mutex<Settings>,
}

struct DesquView {
    shell: Arc<Shell>,
    note: String,
    update_note: String,
    update_url: Option<String>,
}

impl DesquView {
    fn toggle(&mut self, kind: Kind, cx: &mut Context<Self>) {
        let on = !self.shell.settings.lock().unwrap_or_else(|e| e.into_inner()).enabled(kind);
        {
            let mut s = self.shell.settings.lock().unwrap_or_else(|e| e.into_inner());
            s.set(kind, on);
            if let Err(e) = s.save(&self.shell.settings_path) {
                self.note = e;
            }
        }
        self.note.clear();
        let shell = self.shell.clone();
        // Starting and stopping can take a moment (a stop waits for the module to put things back).
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
                if let Err(e) = result {
                    view.note = e;
                }
                cx.notify();
            });
        })
        .detach();
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
}

impl Render for DesquView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = self.shell.settings.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let autostart_on = desqu_native_shell::autostart::is_enabled(&crate::ID);

        let cards = ALL.into_iter().map(|kind| {
            let status = self.shell.supervisor.status(kind);
            let on = settings.enabled(kind);
            let alive = matches!(status, Status::Running | Status::RunningElsewhere);
            div()
                .flex()
                .items_center()
                .gap_3()
                .p_3()
                .rounded(px(8.))
                .border_1()
                .border_color(rgba(if alive { 0x6fe08a60 } else { 0xffffff24 }))
                .bg(rgba(0xffffff0c))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .child(div().text_base().child(kind.name()))
                        .child(div().text_xs().text_color(rgba(0xffffff80)).child(kind.description()))
                        .child(div().text_xs().text_color(rgba(if alive { 0x6fe08aff } else { 0xffb870ff })).child(status_text(status))),
                )
                .child(
                    button("Open", false)
                        .id(SharedString::from(format!("open-{}", kind.name())))
                        .when(!alive, |d| d.opacity(0.4))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            if !this.shell.supervisor.open(kind) {
                                this.note = format!("{} did not answer", kind.name());
                            }
                            cx.notify();
                        })),
                )
                .child(
                    button(if on { "On" } else { "Off" }, on)
                        .id(SharedString::from(format!("toggle-{}", kind.name())))
                        .when(status == Status::Missing, |d| d.opacity(0.4))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle(kind, cx))),
                )
        });

        div()
            .id("desqu")
            .size_full()
            .p(px(24.))
            .bg(rgba(0x14171eff))
            .text_color(rgba(0xffffffff))
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_lg().child("Desqu"))
            .child(div().text_xs().text_color(rgba(0xffffff80)).child("Small desktop utilities for Windows. Turn on the ones you want: each runs on its own, and Desqu keeps them together."))
            .children(cards)
            .when(!self.note.is_empty(), |d| d.child(div().text_xs().text_color(rgba(0xffb870ff)).child(self.note.clone())))
            .child(div().flex_1())
            .child(div().h(px(1.)).bg(rgba(0xffffff20)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().flex_1().text_sm().child("Start with Windows (the modules that are on start with it)"))
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
            .child(
                div().flex().justify_end().child(button("Quit Desqu and its modules", false).id("quit").on_click(|_, _, cx| cx.quit())),
            )
    }
}

fn open_url(url: &str) {
    let _ = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
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
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(200.), px(100.)), size(px(620.), px(560.))))),
            titlebar: Some(gpui::TitlebarOptions { title: Some("Desqu".into()), ..Default::default() }),
            focus: true,
            window_background: WindowBackgroundAppearance::Opaque,
            ..Default::default()
        },
        move |_, cx| {
            cx.new(|cx| {
                // Module states change on their own (a module ends, one is started by hand): look again now and then.
                cx.spawn(async move |this, cx| loop {
                    cx.background_executor().timer(std::time::Duration::from_secs(2)).await;
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        return;
                    }
                })
                .detach();
                DesquView { shell, note: String::new(), update_note: String::new(), update_url: None }
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
        assert!(status_text(Status::Stopped).contains("unexpectedly"));
    }
}
