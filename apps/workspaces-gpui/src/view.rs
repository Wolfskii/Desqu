//! The Workspaces window: one card per virtual desktop with the items tied to it, the first-run
//! question, and the few settings.

use std::sync::Arc;

use gpui::{
    div, point, prelude::*, px, rgba, size, AnyWindowHandle, App, Bounds, ClickEvent, Context, Global, IntoElement, Render, SharedString,
    Window, WindowBackgroundAppearance, WindowBounds, WindowOptions,
};

use crate::engine::{DesktopEntry, Engine};

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

/// What a file is called in a list: its name without the folder (and without `.lnk` / `.url`).
pub fn item_label(id: &str) -> String {
    let name = id.rsplit(['\\', '/']).next().unwrap_or(id);
    for ext in [".lnk", ".url"] {
        if let Some(stem) = name.strip_suffix(ext) {
            return stem.to_string();
        }
    }
    name.to_string()
}

struct WorkspacesView {
    engine: Arc<Engine>,
    note: String,
    update_note: String,
    update_url: Option<String>,
}

impl WorkspacesView {
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

fn card(entry: &DesktopEntry, cx: &mut Context<WorkspacesView>) -> gpui::Div {
    let id = entry.id.clone();
    let count = entry.items.len();
    div()
        .flex()
        .flex_col()
        .gap_1()
        .p_3()
        .rounded(px(8.))
        .border_1()
        .border_color(rgba(if entry.current { 0xaad2ffb4 } else { 0xffffff24 }))
        .bg(rgba(0xffffff0c))
        .when(entry.removed, |d| d.opacity(0.85))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().text_base().child(entry.name.clone()))
                .when(entry.current, |d| d.child(div().text_xs().px_2().rounded(px(4.)).bg(rgba(0x78afff48)).child("You are here")))
                .when(entry.removed, |d| d.child(div().text_xs().px_2().rounded(px(4.)).bg(rgba(0xffb03060)).child("Desktop removed")))
                .child(div().flex_1())
                .child(div().text_sm().text_color(rgba(0xffffff90)).child(format!("{count} {}", if count == 1 { "item" } else { "items" })))
                .when(count > 0, |d| {
                    d.child(
                        button(if entry.removed { "Release its items" } else { "Show on every desktop" }, false)
                            .id(SharedString::from(format!("release-{id}")))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.engine.release_desktop(&id);
                                cx.notify();
                            })),
                    )
                }),
        )
        .when(entry.removed, |d| d.child(div().text_xs().text_color(rgba(0xffffff80)).child("This desktop no longer exists, so its items are shown everywhere again.")))
        .when(!entry.removed && count == 0, |d| {
            d.child(div().text_xs().text_color(rgba(0xffffff80)).child(
                "Nothing is tied to this desktop yet. Select icons on another desktop and press Ctrl+X, then come here and press Ctrl+V on the desktop to move them here. Files you add while you are here join it automatically.",
            ))
        })
        .when(count > 0, |d| {
            d.child(div().flex().flex_wrap().gap_1().children(entry.items.iter().take(24).map(|i| {
                div().text_xs().px_2().py(px(2.)).rounded(px(4.)).bg(rgba(0xffffff14)).child(item_label(i))
            })))
        })
}

impl Render for WorkspacesView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (entries, any_desktops) = self.engine.entries();
        let cfg = self.engine.config();
        let problem = self.engine.problem();

        let first_run = (!cfg.initialized && any_desktops).then(|| {
            div()
                .flex()
                .flex_col()
                .gap_2()
                .p_3()
                .rounded(px(8.))
                .border_1()
                .border_color(rgba(0xffd27a80))
                .bg(rgba(0xffd27a14))
                .child(div().text_base().child("Which virtual desktop do the items on your desktop belong to?"))
                .child(div().text_xs().text_color(rgba(0xffffff90)).child("Nothing is moved or deleted: items stay in your Desktop folders."))
                .child(div().flex().flex_wrap().gap_1().children(
                    entries.iter().filter(|e| !e.removed).map(|e| {
                        let id = e.id.clone();
                        button(e.name.clone(), false).id(SharedString::from(format!("first-{id}"))).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            if let Err(err) = this.engine.answer_first_run(Some(&id)) {
                                this.note = err;
                            }
                            cx.notify();
                        }))
                    }),
                ))
                .child(button("Leave them on every desktop", false).id("first-none").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    let _ = this.engine.answer_first_run(None);
                    cx.notify();
                })))
        });

        let on = cfg.auto_assign_new;
        let autostart_on = desqu_native_shell::autostart::is_enabled(&crate::ID);

        div()
            .id("workspaces")
            .size_full()
            .p(px(24.))
            .bg(rgba(0x14171eff))
            .text_color(rgba(0xffffffff))
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_lg().child("Workspaces"))
            .child(div().text_xs().text_color(rgba(0xffffff80)).child("Give each virtual desktop its own desktop icons."))
            .when_some(first_run, |d, f| d.child(f))
            .when(!any_desktops, |d| {
                d.child(
                    div().p_3().rounded(px(8.)).border_1().border_color(rgba(0xffffff24)).text_sm().child(
                        "Windows lists virtual desktops only once you have more than one: press Win+Tab and choose New desktop.",
                    ),
                )
            })
            .child(
                div()
                    .id("cards")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(entries.iter().map(|e| card(e, cx))),
            )
            .when_some(problem.or_else(|| (!self.note.is_empty()).then(|| self.note.clone())), |d, p| {
                d.child(div().text_xs().text_color(rgba(0xffb870ff)).child(p))
            })
            .child(div().h(px(1.)).bg(rgba(0xffffff20)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().flex_1().text_sm().child("New files join the desktop you are on"))
                    .child(button(if on { "On" } else { "Off" }, on).id("auto").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.engine.set_auto_assign(!on);
                        cx.notify();
                    }))),
            )
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
    }
}

fn open_url(url: &str) {
    let _ = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
}

/// Opens the window, or brings the open one to the front.
pub fn open(cx: &mut App, engine: Arc<Engine>) {
    if let Some(Open(Some(handle))) = cx.try_global::<Open>().map(|o| Open(o.0)) {
        if handle.update(cx, |_, window, _| window.activate_window()).is_ok() {
            return;
        }
    }
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(180.), px(80.)), size(px(680.), px(680.))))),
            titlebar: Some(gpui::TitlebarOptions { title: Some("Workspaces".into()), ..Default::default() }),
            focus: true,
            window_background: WindowBackgroundAppearance::Opaque,
            ..Default::default()
        },
        move |_, cx| {
            cx.new(|cx| {
                // Redraw when the desktop pass changed something (a switch, a new file, a paste).
                let watched = engine.clone();
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
                WorkspacesView { engine: engine.clone(), note: String::new(), update_note: String::new(), update_url: None }
            })
        },
    );
    if let Ok(handle) = opened {
        cx.set_global(Open(Some(handle.into())));
    }
}

#[cfg(test)]
mod tests {
    use super::item_label;

    #[test]
    fn items_are_listed_by_their_file_name_without_shortcut_extensions() {
        assert_eq!(item_label(r"C:\Users\me\Desktop\Notes.lnk"), "Notes");
        assert_eq!(item_label("C:/Users/me/Desktop/Site.url"), "Site");
        assert_eq!(item_label(r"C:\Users\me\Desktop\report.pdf"), "report.pdf");
        assert_eq!(item_label("::{645FF040-5081-101B-9F08-00AA002F954E}"), "::{645FF040-5081-101B-9F08-00AA002F954E}");
    }
}
