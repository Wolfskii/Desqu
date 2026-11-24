//! The "Groups, rules and browser" window: named groups of apps, the rules that say what happens to
//! them, the browser's per-site fullscreen rules, and a look at what Screenbound sees (diagnostics).

use std::sync::Arc;

use gpui::{
    div, point, prelude::*, px, rgba, size, AnyWindowHandle, App, Bounds, ClickEvent, Context, Entity, Global, IntoElement, Render, SharedString,
    Window, WindowBackgroundAppearance, WindowBounds, WindowOptions,
};
use screenbound_module::core::browser::FullscreenMode;
use screenbound_module::core::window::ChromeMode;

use crate::field::TextField;
use crate::manage_model::{self as m, Scope};
use crate::runtime::Backend;
use crate::view::button;

#[derive(Default)]
struct Open(Option<AnyWindowHandle>);
impl Global for Open {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Groups,
    Rules,
    Browser,
    Diagnostics,
}

pub struct ManageView {
    rt: Arc<dyn Backend>,
    tab: Tab,
    name: Entity<TextField>,
    site: Entity<TextField>,
    scope: Scope,
    chrome: ChromeMode,
    effect: Effect,
    fill: m::Fill,
    topmost: bool,
    site_mode: FullscreenMode,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Effect {
    Fullscreen,
    Borderless,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn dim(text: impl Into<SharedString>) -> gpui::Div {
    div().text_xs().text_color(rgba(0xffffff80)).child(text.into())
}

fn row() -> gpui::Div {
    div().flex().flex_wrap().items_center().gap_2().min_h(px(32.))
}

fn rule_line() -> gpui::Div {
    div().h(px(1.)).bg(rgba(0xffffff20)).my_1()
}

impl Render for ManageView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snap = self.rt.snapshot();
        let cfg = snap.config.clone().unwrap_or_default();

        let tabs = [(Tab::Groups, "Groups"), (Tab::Rules, "Rules"), (Tab::Browser, "Browser"), (Tab::Diagnostics, "Diagnostics")].map(|(tab, label)| {
            button(label, self.tab == tab).id(label).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.tab = tab;
                cx.notify();
            }))
        });

        let body: gpui::AnyElement = match self.tab {
            Tab::Groups => {
                let list = cfg.groups.iter().enumerate().map(|(i, g)| {
                    let id = g.id.clone();
                    let members = cfg.known_apps.iter().filter(|a| a.group_id.as_deref() == Some(g.id.as_str())).count();
                    let color = u32::from_str_radix(g.color.trim_start_matches('#'), 16).unwrap_or(0x5b9cff);
                    row()
                        .child(div().w(px(12.)).h(px(12.)).rounded(px(6.)).bg(gpui::rgb(color)))
                        .child(div().flex_1().text_sm().child(g.name.clone()))
                        .child(dim(format!("{members} app(s)")))
                        .child(button("Delete", false).id(("gdel", i)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            let id = id.clone();
                            this.rt.change(move |c| m::delete_group(c, &id));
                            cx.notify();
                        })))
                });
                let apps = cfg.known_apps.iter().enumerate().map(|(i, a)| {
                    let process = a.process_name.clone();
                    let group = a.group_id.as_deref().and_then(|id| cfg.groups.iter().find(|g| g.id == id)).map_or("no group", |g| g.name.as_str());
                    let title = if a.title.is_empty() { a.process_name.clone() } else { format!("{} ({})", a.title, a.process_name) };
                    row().child(div().flex_1().min_w_0().overflow_hidden().text_sm().child(title)).child(button(group.to_string(), a.group_id.is_some()).id(("agrp", i)).on_click(cx.listener(
                        move |this, _: &ClickEvent, _, cx| {
                            let process = process.clone();
                            this.rt.change(move |c| m::cycle_app_group(c, &process));
                            cx.notify();
                        },
                    )))
                });
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(dim("Groups collect apps so one rule can cover all of them."))
                    .children(list)
                    .child(row().child(div().w(px(260.)).child(self.name.clone())).child(button("Add group", false).id("gadd").on_click(cx.listener(
                        |this, _: &ClickEvent, _, cx| {
                            let name = this.name.read(cx).text().to_string();
                            this.rt.change(move |c| m::add_group(c, &name).is_some());
                            this.name.update(cx, |f, cx| f.clear(cx));
                            cx.notify();
                        },
                    ))))
                    .child(rule_line())
                    .child(dim("Apps: click one to move it to the next group."))
                    .child(div().id("group-apps").h(px(260.)).overflow_y_scroll().flex().flex_col().children(apps))
                    .into_any_element()
            }
            Tab::Rules => {
                let zone = cfg.zones.first().map(|z| z.id.clone());
                let list = cfg.rules.iter().enumerate().map(|(i, r)| {
                    let (a, b) = (r.id.clone(), r.id.clone());
                    let enabled = r.enabled;
                    row()
                        .child(button(if enabled { "On" } else { "Off" }, enabled).id(("ron", i)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            let a = a.clone();
                            this.rt.change(move |c| m::set_rule_enabled(c, &a, !enabled));
                            cx.notify();
                        })))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .when(!enabled, |d| d.opacity(0.5))
                                .child(div().text_sm().child(r.name.clone()))
                                .child(dim(m::rule_summary(&cfg, r))),
                        )
                        .child(button("Delete", false).id(("rdel", i)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            let b = b.clone();
                            this.rt.change(move |c| m::delete_rule(c, &b));
                            cx.notify();
                        })))
                });
                let scopes = std::iter::once((Scope::All, "Every app that is on".to_string()))
                    .chain(cfg.groups.iter().map(|g| (Scope::Group(g.id.clone()), format!("Group {}", g.name))))
                    .enumerate()
                    .map(|(i, (scope, label))| {
                        button(label, self.scope == scope).id(("scope", i)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.scope = scope.clone();
                            cx.notify();
                        }))
                    });
                let scope = self.scope.clone();
                let (chrome, effect, fill, topmost) = (self.chrome, self.effect, self.fill, self.topmost);
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(dim("Rules say what happens to the windows of the apps that are on."))
                    .child(div().id("rules-list").max_h(px(230.)).overflow_y_scroll().flex().flex_col().children(list))
                    .child(rule_line())
                    .child(div().text_sm().child("New rule for"))
                    .child(div().flex().flex_wrap().gap_1().children(scopes))
                    .child(div().text_sm().child("does this"))
                    .child(
                        row()
                            .child(button("Fullscreen stays in the zone", effect == Effect::Fullscreen).id("fx-full").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.effect = Effect::Fullscreen;
                                cx.notify();
                            })))
                            .child(button("Windowed app becomes borderless", effect == Effect::Borderless).id("fx-border").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.effect = Effect::Borderless;
                                cx.notify();
                            }))),
                    )
                    .child(if effect == Effect::Fullscreen {
                        row()
                            .child(button("Keep the title bar", chrome == ChromeMode::Keep).id("keep").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.chrome = ChromeMode::Keep;
                                cx.notify();
                            })))
                            .child(button("Hide the title bar", chrome == ChromeMode::Hide).id("hide").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.chrome = ChromeMode::Hide;
                                cx.notify();
                            })))
                    } else {
                        row()
                            .child(button("Fill the monitor", fill == m::Fill::Monitor).id("fill-mon").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.fill = m::Fill::Monitor;
                                cx.notify();
                            })))
                            .child(button("Fill the zone", fill == m::Fill::Zone).id("fill-zone").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.fill = m::Fill::Zone;
                                cx.notify();
                            })))
                            .child(button("Keep on top", topmost).id("topmost").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.topmost = !this.topmost;
                                cx.notify();
                            })))
                    })
                    .child(row().child(div().w(px(260.)).child(self.name.clone())).child(button("Add rule", false).id("radd").on_click(cx.listener(
                        move |this, _: &ClickEvent, _, cx| {
                            let name = this.name.read(cx).text().to_string();
                            let (scope, zone) = (scope.clone(), zone.clone());
                            match effect {
                                Effect::Fullscreen => {
                                    if let Some(zone) = zone {
                                        this.rt.change(move |c| m::add_rule(c, &name, &scope, &zone, chrome).is_some());
                                        this.name.update(cx, |f, cx| f.clear(cx));
                                    }
                                }
                                Effect::Borderless => {
                                    this.rt.change(move |c| m::add_borderless_rule(c, &name, &scope, fill, topmost).is_some());
                                    this.name.update(cx, |f, cx| f.clear(cx));
                                }
                            }
                            cx.notify();
                        },
                    ))))
                    .into_any_element()
            }
            Tab::Browser => {
                let on = cfg.browser.integration_enabled;
                let sites = &cfg.browser.site_rules;
                let default_modes = m::MODES.into_iter().map(|mode| {
                    button(m::mode_label(mode), sites.default_mode == mode).id(("dm", mode as usize)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.rt.change(move |c| m::set_default_mode(c, mode, now_ms()));
                        cx.notify();
                    }))
                });
                let site_modes = m::MODES.into_iter().map(|mode| {
                    button(m::mode_label(mode), self.site_mode == mode).id(("sm", mode as usize)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.site_mode = mode;
                        cx.notify();
                    }))
                });
                let list = sites.rules.iter().enumerate().map(|(i, r)| {
                    let (a, b) = (r.id.clone(), r.id.clone());
                    let enabled = r.enabled;
                    row()
                        .child(button(if enabled { "On" } else { "Off" }, enabled).id(("son", i)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            let a = a.clone();
                            this.rt.change(move |c| m::set_site_rule_enabled(c, &a, !enabled, now_ms()));
                            cx.notify();
                        })))
                        .child(div().flex_1().text_sm().when(!enabled, |d| d.opacity(0.5)).child(format!("{}: {}", m::pattern_text(&r.pattern), m::mode_label(r.mode))))
                        .child(button("Delete", false).id(("sdel", i)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            let b = b.clone();
                            this.rt.change(move |c| m::delete_site_rule(c, &b, now_ms()));
                            cx.notify();
                        })))
                });
                let (mode, zone) = (self.site_mode, cfg.zones.first().map(|z| z.id.clone()));
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(row().child(div().flex_1().text_sm().child("Browser integration (needs the Screenbound browser extension)")).child(
                        button(if on { "On" } else { "Off" }, on).id("bint").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.rt.change(move |c| m::set_integration(c, !on));
                            this.rt.sync_browser();
                            cx.notify();
                        })),
                    ))
                    .child(div().text_sm().child("Fullscreen video on a site without its own rule:"))
                    .child(div().flex().flex_wrap().gap_1().children(default_modes))
                    .child(rule_line())
                    .child(div().text_sm().child("Sites"))
                    .when(sites.rules.is_empty(), |d| d.child(dim("No site rules yet.")))
                    .child(div().id("sites").max_h(px(170.)).overflow_y_scroll().flex().flex_col().children(list))
                    .child(div().flex().flex_wrap().gap_1().children(site_modes))
                    .child(row().child(div().w(px(300.)).child(self.site.clone())).child(button("Add site", false).id("sadd").on_click(cx.listener(
                        move |this, _: &ClickEvent, _, cx| {
                            let text = this.site.read(cx).text().to_string();
                            let zone = zone.clone();
                            this.rt.change(move |c| m::add_site_rule(c, &text, mode, zone.as_deref(), now_ms()));
                            this.site.update(cx, |f, cx| f.clear(cx));
                            cx.notify();
                        },
                    ))))
                    .into_any_element()
            }
            Tab::Diagnostics => {
                let status = snap.status.as_ref();
                let rows = snap.windows.iter().take(60).map(|w| {
                    let tags: Vec<&str> = [(w.managed, "managed"), (w.fullscreen_like, "fullscreen"), (w.hung, "not responding")]
                        .into_iter()
                        .filter(|(on, _)| *on)
                        .map(|(_, t)| t)
                        .collect();
                    let rule = w.matched_rule.as_deref().and_then(|id| cfg.rules.iter().find(|r| r.id == id)).map(|r| format!(" - rule {}", r.name)).unwrap_or_default();
                    div().text_xs().child(format!(
                        "{}  {}{}{}",
                        w.identity.process_name,
                        w.monitor_name.clone().unwrap_or_default(),
                        if tags.is_empty() { String::new() } else { format!("  [{}]", tags.join(", ")) },
                        rule
                    ))
                });
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_sm().child(format!(
                        "Screenbound is {}; window events are {}; {} window(s) managed; {} monitor(s); {} window(s) seen.",
                        if status.is_some_and(|s| s.enabled) { "on" } else { "off" },
                        if status.is_some_and(|s| s.events_active) { "live" } else { "not live (checking every few seconds)" },
                        status.map_or(0, |s| s.managed.len()),
                        snap.monitors.len(),
                        snap.windows.len()
                    )))
                    .child(row().child(button("Give every window its size back", false).id("release").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.rt.release_all();
                        cx.notify();
                    }))))
                    .child(dim("Open windows:"))
                    .child(div().id("seen").h(px(300.)).overflow_y_scroll().flex().flex_col().children(rows))
                    .into_any_element()
            }
        };

        div()
            .id("manage")
            .size_full()
            .p(px(24.))
            .bg(rgba(0x14171eff))
            .text_color(rgba(0xffffffff))
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_lg().child("Groups, rules and browser"))
            .child(div().flex().gap_1().children(tabs))
            .child(body)
    }
}

/// The view, redrawing when the snapshot changes.
fn create(rt: Arc<dyn Backend>, cx: &mut App) -> Entity<ManageView> {
    let name = cx.new(|cx| TextField::new("Name", cx));
    let site = cx.new(|cx| TextField::new("youtube.com or *.vimeo.com", cx));
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
        ManageView { rt, tab: Tab::Groups, name, site, scope: Scope::All, chrome: ChromeMode::Keep, effect: Effect::Fullscreen, fill: m::Fill::Monitor, topmost: false, site_mode: FullscreenMode::Zone }
    })
}

/// The view for the Desqu window.
pub fn embedded(rt: Arc<dyn Backend>, cx: &mut App) -> Entity<ManageView> {
    create(rt, cx)
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
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(200.), px(80.)), size(px(640.), px(640.))))),
            titlebar: Some(gpui::TitlebarOptions { title: Some("Screenbound: groups, rules and browser".into()), ..Default::default() }),
            focus: true,
            is_resizable: false,
            window_background: WindowBackgroundAppearance::Opaque,
            ..Default::default()
        },
        move |_, cx| create(rt, cx),
    );
    if let Ok(handle) = opened {
        cx.set_global(Open(Some(handle.into())));
    }
}
