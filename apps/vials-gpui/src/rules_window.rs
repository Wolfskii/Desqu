//! The rules window: the automatic rules that send new desktop items to a vial. Rules are tried in
//! order and the first match wins. A new rule is one condition; rules with several conditions made
//! elsewhere are listed and can be reordered, switched off or deleted.

use std::sync::Arc;

use desqu_desktop::ItemKind;
use gpui::{
    div, point, prelude::*, px, rgba, size, AnyWindowHandle, App, Bounds, ClickEvent, Context, Entity, Global, IntoElement, Render, SharedString,
    Window, WindowBackgroundAppearance, WindowBounds, WindowOptions,
};
use vials_core::model::ItemMatcher;
use vials_core::rules::{all_of, build_matcher, describe, kind_label, ConditionKind};

use crate::engine::Engine;
use crate::field::TextField;
use crate::settings::button;
use crate::view::{bump, Revision};

#[derive(Default)]
struct Open(Option<AnyWindowHandle>);
impl Global for Open {}

const ITEM_KINDS: [ItemKind; 5] = [ItemKind::Shortcut, ItemKind::Folder, ItemKind::App, ItemKind::File, ItemKind::Special];

struct RulesView {
    engine: Arc<Engine>,
    revision: Entity<Revision>,
    field: Entity<TextField>,
    condition: ConditionKind,
    item_kind: ItemKind,
    target: Option<String>,
    /// Conditions already added to the rule being made; the rule needs all of them.
    pending: Vec<ItemMatcher>,
}

impl RulesView {
    fn changed(&self, cx: &mut App) {
        bump(&self.revision, cx);
    }
}

impl Render for RulesView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let cfg = self.engine.config();
        let vials: Vec<_> = cfg.vials.iter().filter(|v| !v.inbox).collect();
        let target = self.target.clone().filter(|t| vials.iter().any(|v| &v.id == t)).or_else(|| vials.first().map(|v| v.id.clone()));
        let vial_name = |id: &str| vials.iter().find(|v| v.id == id).map_or("(deleted vial)".to_string(), |v| v.name.clone());

        let list = cfg.rules.iter().enumerate().map(|(i, rule)| {
            let id = rule.id.clone();
            let (id_on, id_up, id_down, id_del) = (id.clone(), id.clone(), id.clone(), id);
            let enabled = rule.enabled;
            div()
                .flex()
                .items_center()
                .gap_2()
                .min_h(px(34.))
                .child(button(if enabled { "On" } else { "Off" }, enabled).id(("on", i)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.engine.set_rule_enabled(&id_on, !enabled);
                    this.changed(cx);
                })))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .when(!enabled, |d| d.opacity(0.5))
                        .child(div().text_sm().child(format!("If it {}", describe(&rule.matcher))))
                        .child(div().text_xs().text_color(rgba(0xffffff80)).child(format!("goes to {}", vial_name(&rule.target_vial)))),
                )
                .child(button("^", false).id(("up", i)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.engine.move_rule(&id_up, -1);
                    this.changed(cx);
                })))
                .child(button("v", false).id(("down", i)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.engine.move_rule(&id_down, 1);
                    this.changed(cx);
                })))
                .child(button("x", false).id(("del", i)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.engine.delete_rule(&id_del);
                    this.changed(cx);
                })))
        });

        let conditions = ConditionKind::ALL.into_iter().map(|kind| {
            button(kind.label(), self.condition == kind).id(kind.label()).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.condition = kind;
                this.field.update(cx, |f, cx| f.set_placeholder(kind.hint(), cx));
                cx.notify();
            }))
        });
        let kinds = ITEM_KINDS.into_iter().map(|kind| {
            button(kind_label(kind), self.item_kind == kind).id(kind_label(kind)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.item_kind = kind;
                cx.notify();
            }))
        });
        let targets = vials.iter().map(|v| {
            let id = v.id.clone();
            button(v.name.clone(), target.as_deref() == Some(v.id.as_str())).id(SharedString::from(v.id.clone())).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.target = Some(id.clone());
                cx.notify();
            }))
        });

        let text = self.field.read(cx).text().to_string();
        let matcher = build_matcher(self.condition, &text, self.item_kind);
        let can_add = (matcher.is_some() || !self.pending.is_empty()) && target.is_some();
        let can_and = matcher.is_some();
        let target_for_add = target.clone();

        div()
            .id("rules")
            .size_full()
            .p(px(24.))
            .bg(rgba(0x14171eff))
            .text_color(rgba(0xffffffff))
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_lg().child("Automatic rules"))
            .child(div().text_xs().text_color(rgba(0xffffff80)).child("New desktop items go to the vial of the first rule that matches."))
            .child(div().flex().child(button("Sort desktop items now", false).id("sort-now").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                // The same pass that runs by itself when a file appears.
                this.engine.reconcile();
                this.changed(cx);
            }))))
            .child(
                div()
                    .id("rule-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .when(cfg.rules.is_empty(), |d| d.child(div().text_sm().text_color(rgba(0xffffff80)).child("No rules yet.")))
                    .children(list),
            )
            .child(div().h(px(1.)).bg(rgba(0xffffff20)))
            .child(div().text_sm().child("New rule: if the item"))
            .child(div().flex().flex_wrap().gap_1().children(conditions))
            .child(if self.condition == ConditionKind::Kind {
                div().flex().flex_wrap().gap_1().children(kinds).into_any_element()
            } else {
                self.field.clone().into_any_element()
            })
            .child(div().flex().flex_wrap().items_center().gap_1().children(self.pending.iter().enumerate().map(|(i, m)| {
                button(format!("{}  x", describe(m)), true).id(("pend", i)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if i < this.pending.len() {
                        this.pending.remove(i);
                    }
                    cx.notify();
                }))
            })))
            .child(
                button("+ and also...", false)
                    .id("and")
                    .when(!can_and, |d| d.opacity(0.4))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        let text = this.field.read(cx).text().to_string();
                        if let Some(m) = build_matcher(this.condition, &text, this.item_kind) {
                            this.pending.push(m);
                            this.field.update(cx, |f, cx| f.clear(cx));
                            cx.notify();
                        }
                    })),
            )
            .child(div().text_sm().child("goes to"))
            .child(div().flex().flex_wrap().gap_1().children(targets))
            .child(
                div().flex().justify_end().child(
                    button("Add rule", can_add)
                        .id("add")
                        .when(!can_add, |d| d.opacity(0.4))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            let text = this.field.read(cx).text().to_string();
                            let mut all = this.pending.clone();
                            all.extend(build_matcher(this.condition, &text, this.item_kind));
                            let (Some(matcher), Some(target)) = (all_of(all), target_for_add.clone()) else {
                                return;
                            };
                            let name = describe(&matcher);
                            if this.engine.add_rule(&name, matcher, &target) {
                                this.pending.clear();
                                this.field.update(cx, |f, cx| f.clear(cx));
                                this.changed(cx);
                            }
                        })),
                ),
            )
    }
}

/// Opens the rules window, or brings the open one to the front.
pub fn open(cx: &mut App, engine: Arc<Engine>, revision: Entity<Revision>) {
    if let Some(Open(Some(handle))) = cx.try_global::<Open>().map(|o| Open(o.0)) {
        if handle.update(cx, |_, window, _| window.activate_window()).is_ok() {
            return;
        }
    }
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(260.), px(80.)), size(px(620.), px(700.))))),
            titlebar: Some(gpui::TitlebarOptions { title: Some("Vial rules".into()), ..Default::default() }),
            focus: true,
            is_resizable: false,
            window_background: WindowBackgroundAppearance::Opaque,
            ..Default::default()
        },
        move |_, cx| {
            let field = cx.new(|cx| TextField::new(ConditionKind::Contains.hint(), cx));
            cx.new(|cx| {
                cx.observe(&field, |_, _, cx| cx.notify()).detach();
                RulesView { engine, revision, field, condition: ConditionKind::Contains, item_kind: ItemKind::Shortcut, target: None, pending: Vec::new() }
            })
        },
    );
    if let Ok(handle) = opened {
        cx.set_global(Open(Some(handle.into())));
    }
}
