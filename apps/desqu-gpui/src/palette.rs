//! The command palette: a hotkey opens a small window, you type, and Desqu finds what you meant: go to a page,
//! turn an app on or off, make a vial, go to a virtual desktop, open an item of a vial. Enter does it.
//! What is found and in which order is pure and tested; the window below it is thin.

use std::sync::Arc;

use gpui::{
    div, prelude::*, px, rgb, rgba, size, AnyWindowHandle, App, Bounds, Context, FocusHandle, Global, IntoElement, KeyDownEvent, Render, SharedString,
    Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
};

use crate::modules::{Kind, ALL};
use crate::view::{Page, Shell};

/// What a command does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Go(PageTarget),
    Toggle(Kind),
    NewVial,
    ReleaseWindows,
    GoToDesktop(usize),
    SaveReport,
    BackUp,
    SaveLayout,
    RestoreLayout,
    Quit,
    OpenItem(String),
}

/// A page of the Desqu window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageTarget {
    Home,
    App(Kind),
    Settings,
}

impl From<PageTarget> for Page {
    fn from(t: PageTarget) -> Page {
        match t {
            PageTarget::Home => Page::Home,
            PageTarget::App(k) => Page::App(k),
            PageTarget::Settings => Page::Settings,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub title: String,
    /// A word or two of what it is, shown at the right.
    pub hint: String,
    pub action: Action,
}

fn cmd(title: impl Into<String>, hint: &str, action: Action) -> Command {
    Command { title: title.into(), hint: hint.to_string(), action }
}

/// The commands that do not depend on what is on the desktop. `desktops` is how many virtual desktops there are.
pub fn builtin(is_on: impl Fn(Kind) -> bool, desktops: usize) -> Vec<Command> {
    let mut out = vec![cmd("Home", "go to", Action::Go(PageTarget::Home)), cmd("Settings", "go to", Action::Go(PageTarget::Settings))];
    for kind in ALL {
        out.push(cmd(kind.name(), "go to", Action::Go(PageTarget::App(kind))));
        out.push(cmd(format!("{} {}", if is_on(kind) { "Turn off" } else { "Turn on" }, kind.name()), "app", Action::Toggle(kind)));
    }
    out.push(cmd("New vial", "Vials", Action::NewVial));
    out.push(cmd("Release all windows", "Screenbound", Action::ReleaseWindows));
    for n in 0..desktops.min(9) {
        out.push(cmd(format!("Go to desktop {}", n + 1), "Workspaces", Action::GoToDesktop(n)));
    }
    out.push(cmd("Remember the window places", "layouts", Action::SaveLayout));
    out.push(cmd("Put the windows back", "layouts", Action::RestoreLayout));
    out.push(cmd("Save a report", "help", Action::SaveReport));
    out.push(cmd("Back up the settings", "settings", Action::BackUp));
    out.push(cmd("Quit Desqu", "Desqu", Action::Quit));
    out
}

/// One command per item of the vials, to open it: the title is the item's name, the hint the vial.
pub fn items(vials: &vials_core::model::VialsConfig) -> Vec<Command> {
    vials
        .vials
        .iter()
        .filter(|v| v.source.is_none())
        .flat_map(|v| v.items.iter().map(move |id| cmd(crate::workspaces_page::label(id, vials), &v.name, Action::OpenItem(id.clone()))))
        .collect()
}

/// The commands that fit `query` best first, at most `limit`. Each word of the query must be in the title (or the
/// hint); a title that starts with the query comes first, then one with a word that starts with it, then the rest.
/// An empty query gives the first commands as they are.
pub fn rank<'a>(commands: &'a [Command], query: &str, limit: usize) -> Vec<&'a Command> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return commands.iter().take(limit).collect();
    }
    let mut scored: Vec<(u8, usize, &Command)> = commands
        .iter()
        .enumerate()
        .filter_map(|(order, c)| {
            let title = c.title.to_lowercase();
            let hint = c.hint.to_lowercase();
            if !words.iter().all(|w| title.contains(w.as_str()) || hint.contains(w.as_str())) {
                return None;
            }
            let joined = words.join(" ");
            let score = if title.starts_with(&joined) {
                0
            } else if title.split_whitespace().any(|t| t.starts_with(words[0].as_str())) {
                1
            } else if words.iter().all(|w| title.contains(w.as_str())) {
                2
            } else {
                3
            };
            Some((score, order, c))
        })
        .collect();
    scored.sort_by_key(|s| (s.0, s.1));
    scored.into_iter().take(limit).map(|s| s.2).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<Command> {
        builtin(|k| k == Kind::Vials, 3)
    }

    #[test]
    fn an_empty_query_lists_the_commands_and_a_word_finds_the_ones_that_have_it() {
        let commands = all();
        assert_eq!(rank(&commands, "", 5).len(), 5);
        let titles = |q: &str| rank(&commands, q, 20).iter().map(|c| c.title.clone()).collect::<Vec<_>>();
        assert_eq!(titles("desktop 2"), vec!["Go to desktop 2"]);
        assert!(titles("turn").contains(&"Turn off Vials".to_string()) && titles("turn").contains(&"Turn on Workspaces".to_string()));
        assert!(titles("vials").iter().any(|t| t == "New vial") || titles("vial").iter().any(|t| t == "New vial"), "the hint counts too");
        assert!(titles("zzz").is_empty());
    }

    #[test]
    fn what_starts_with_the_query_comes_first() {
        let commands = vec![cmd("Open all", "x", Action::Quit), cmd("New vial", "x", Action::NewVial), cmd("Vials", "go to", Action::Go(PageTarget::App(Kind::Vials)))];
        let titles: Vec<&str> = rank(&commands, "vial", 10).iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, vec!["Vials", "New vial"], "a title that starts with it, then one with a word that does");
    }

    #[test]
    fn the_items_of_the_vials_can_be_found_by_name_but_not_the_ones_of_a_folder_portal() {
        let mut cfg = vials_core::model::VialsConfig::default();
        let id = cfg.add_vial(vials_core::model::NormalizedRect { x: 0.1, y: 0.1, width: 0.2, height: 0.2 }, None);
        cfg.assign(r"C:\d\Budget 2026.xlsx", Some(&id));
        let portal = cfg.add_portal(vials_core::model::NormalizedRect { x: 0.5, y: 0.1, width: 0.2, height: 0.2 }, None, r"C:\Data");
        cfg.vials.iter_mut().find(|v| v.id == portal).unwrap().items.push(r"C:\Data\not-listed.txt".into());
        let found = items(&cfg);
        assert_eq!(found.len(), 1);
        assert_eq!(rank(&found, "budget", 5)[0].action, Action::OpenItem(r"C:\d\Budget 2026.xlsx".into()));
    }

    #[test]
    fn every_action_that_goes_to_a_page_maps_to_one() {
        assert_eq!(Page::from(PageTarget::Home), Page::Home);
        assert_eq!(Page::from(PageTarget::App(Kind::Screenbound)), Page::App(Kind::Screenbound));
    }
}

#[cfg(test)]
mod window_tests {
    use super::*;
    use gpui::{Keystroke, Modifiers, TestAppContext};

    fn key(text: &str) -> KeyDownEvent {
        let (key, key_char) = if text.chars().count() == 1 { (text.to_string(), Some(text.to_string())) } else { (text.to_string(), None) };
        KeyDownEvent { keystroke: Keystroke { modifiers: Modifiers::none(), key, key_char }, is_held: false }
    }

    #[gpui::test]
    fn typing_narrows_the_list_and_the_arrows_move_the_choice(cx: &mut TestAppContext) {
        let shell = Arc::new(Shell {
            supervisor: Arc::new(crate::supervisor::Supervisor::new(std::path::Path::new("."))),
            settings_path: std::env::temp_dir().join("desqu-palette-test.json"),
            settings: std::sync::Mutex::new(crate::modules::Settings::default()),
            nav: std::sync::Mutex::new(None),
        });
        let (palette, cx) = cx.add_window_view(|_, cx| Palette { shell, commands: builtin(|_| false, 3), query: String::new(), chosen: 0, focus: cx.focus_handle() });
        let press = |text: &str, cx: &mut gpui::VisualTestContext| {
            let event = key(text);
            palette.update_in(cx, |p, window, cx| p.on_key(&event, window, cx));
        };
        let titles = |cx: &mut gpui::VisualTestContext| palette.read_with(cx, |p, _| p.matches().iter().map(|c| c.title.clone()).collect::<Vec<_>>());
        assert!(titles(cx).len() > 3, "an empty query lists the first commands");
        for c in "desk".chars() {
            press(&c.to_string(), cx);
        }
        assert_eq!(titles(cx), vec!["Go to desktop 1", "Go to desktop 2", "Go to desktop 3"]);
        press("down", cx);
        press("down", cx);
        press("down", cx);
        assert_eq!(palette.read_with(cx, |p, _| p.chosen), 2, "the choice stops at the last one");
        press("up", cx);
        assert_eq!(palette.read_with(cx, |p, _| p.chosen), 1);
        press("backspace", cx);
        assert_eq!(palette.read_with(cx, |p, _| (p.query.clone(), p.chosen)), ("des".to_string(), 0), "typing again starts at the top");
    }
}

// ------------------------------------------------------------------------------------------ the window

#[derive(Default)]
struct Current(Option<AnyWindowHandle>);
impl Global for Current {}

/// How many commands the window lists.
const SHOWN: usize = 8;

struct Palette {
    shell: Arc<Shell>,
    commands: Vec<Command>,
    query: String,
    chosen: usize,
    focus: FocusHandle,
}

impl Palette {
    fn matches(&self) -> Vec<&Command> {
        rank(&self.commands, &self.query, SHOWN)
    }

    fn on_key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let k = &e.keystroke;
        match k.key.as_str() {
            "escape" => return window.remove_window(),
            "down" => self.chosen = (self.chosen + 1).min(self.matches().len().saturating_sub(1)),
            "up" => self.chosen = self.chosen.saturating_sub(1),
            "backspace" => {
                self.query.pop();
                self.chosen = 0;
            }
            "enter" => {
                let action = self.matches().get(self.chosen).map(|c| c.action.clone());
                window.remove_window();
                if let Some(action) = action {
                    run(cx, &self.shell, action);
                }
                return;
            }
            _ => {
                if let Some(typed) = k.key_char.as_ref().filter(|_| !k.modifiers.control && !k.modifiers.alt) {
                    if self.query.chars().count() < 60 {
                        self.query.push_str(typed);
                        self.chosen = 0;
                    }
                }
            }
        }
        cx.notify();
    }
}

impl Render for Palette {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.focus(&self.focus);
        let matches: Vec<(String, String)> = self.matches().iter().map(|c| (c.title.clone(), c.hint.clone())).collect();
        let chosen = self.chosen;
        div()
            .id("palette")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| this.on_key(e, window, cx)))
            .size_full()
            .flex()
            .flex_col()
            .rounded(px(10.))
            .border_1()
            .border_color(rgba(0xffffff30))
            .bg(rgba(0x14171efa))
            .text_color(rgba(0xffffffff))
            .text_size(px(15.))
            .child(div().h(px(48.)).flex_none().px_4().flex().items_center().border_b_1().border_color(rgba(0xffffff20)).child(if self.query.is_empty() {
                div().text_color(rgba(0xffffff70)).child("Type what you want to do")
            } else {
                div().child(format!("{}|", self.query))
            }))
            .child(div().flex_1().min_h_0().flex().flex_col().p_1().children(matches.into_iter().enumerate().map(|(i, (title, hint))| {
                div()
                    .px_3()
                    .h(px(32.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .when(i == chosen, |d| d.bg(rgb(0x2b3a52)))
                    .child(div().flex_1().child(SharedString::from(title)))
                    .child(div().text_size(px(12.)).text_color(rgba(0xffffff70)).child(SharedString::from(hint)))
            })))
    }
}

/// Opens the palette (or closes the one that is open).
pub fn toggle(cx: &mut App, shell: Arc<Shell>) {
    if let Some(Current(Some(open))) = cx.try_global::<Current>().map(|c| Current(c.0)) {
        if open.update(cx, |_, window, _| window.remove_window()).is_ok() {
            cx.set_global(Current(None));
            return;
        }
    }
    let on = |k: Kind| shell.settings.lock().unwrap_or_else(|e| e.into_inner()).enabled(k);
    let mut commands = builtin(on, workspaces_platform::desktops().map(|d| d.list.len()).unwrap_or(0));
    commands.extend(items(&crate::app_settings::vials()));
    let bounds = Bounds::centered(None, size(px(560.), px(48. + 8. * 32. + 12.)), cx);
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            focus: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        },
        move |window, cx| {
            cx.new(|cx| {
                // Clicking away (or switching to another program) closes it.
                cx.observe_window_activation(window, |_, window, _| {
                    if !window.is_window_active() {
                        window.remove_window();
                    }
                })
                .detach();
                Palette { shell, commands, query: String::new(), chosen: 0, focus: cx.focus_handle() }
            })
        },
    );
    if let Ok(handle) = opened {
        let handle: AnyWindowHandle = handle.into();
        cx.set_global(Current(Some(handle)));
    }
}

/// Does what the command says.
fn run(cx: &mut App, shell: &Arc<Shell>, action: Action) {
    use desqu_native_shell::tray;
    match action {
        Action::Go(target) => {
            *shell.nav.lock().unwrap_or_else(|e| e.into_inner()) = Some(target.into());
            crate::view::open(cx, shell.clone());
        }
        Action::Toggle(kind) => {
            let on = {
                let mut settings = shell.settings.lock().unwrap_or_else(|e| e.into_inner());
                let on = !settings.enabled(kind);
                settings.set(kind, on);
                let _ = settings.save(&shell.settings_path);
                crate::view::sync_autostart(&settings);
                on
            };
            let shell = shell.clone();
            std::thread::spawn(move || {
                if on {
                    let _ = shell.supervisor.start(kind);
                } else {
                    shell.supervisor.stop(kind);
                }
            });
        }
        Action::NewVial => {
            let _ = tray::forward_action(&Kind::Vials.identity(), "create-vial");
        }
        Action::ReleaseWindows => {
            let _ = tray::forward_action(&Kind::Screenbound.identity(), "release");
        }
        Action::GoToDesktop(n) => {
            std::thread::spawn(move || {
                let Ok(d) = workspaces_platform::desktops() else { return };
                let Some(current) = d.current.as_deref().and_then(|c| d.list.iter().position(|x| x.id == c)) else { return };
                if let Some((dir, count)) = workspaces_core::jump::plan(current, n, d.list.len()) {
                    workspaces_platform::press_desktop_keys(dir == workspaces_core::jump::Direction::Right, count);
                }
            });
        }
        Action::SaveReport => {
            let shell = shell.clone();
            std::thread::spawn(move || {
                if let Some(folder) = desqu_desktop::desktop_folders().into_iter().next() {
                    if let Ok(zip) = crate::report::save(&folder, |k| shell.supervisor.status(k)) {
                        let _ = std::process::Command::new("explorer.exe").arg(format!("/select,{}", zip.display())).spawn();
                    }
                }
            });
        }
        Action::BackUp => {
            let also = shell.settings.lock().unwrap_or_else(|e| e.into_inner()).backup_onedrive;
            std::thread::spawn(move || {
                let _ = crate::backup::back_up(&crate::backup::Paths::real(), also);
            });
        }
        Action::SaveLayout => {
            std::thread::spawn(|| crate::windowlayout::WindowLayouts::new().save_now());
        }
        Action::RestoreLayout => {
            std::thread::spawn(|| {
                let _ = crate::windowlayout::WindowLayouts::new().restore_now();
            });
        }
        Action::Quit => cx.quit(),
        Action::OpenItem(path) => {
            let _ = std::process::Command::new("explorer.exe").arg(path).spawn();
        }
    }
}
