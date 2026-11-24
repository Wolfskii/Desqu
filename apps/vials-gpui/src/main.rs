//! Vials in GPUI. Read-only viewer: it opens one window per vial of the real Vials config, in the
//! same place, with the same items and icons, but never writes the config or hides anything.
//!   vials-gpui [--config <path to config.json>] [--live]
//! With `--live` it also owns the desktop: items in a vial are hidden on the desktop (put back on
//! exit, or from the journal after a crash), vials can be edited (drop, drag out, rename, sort,
//! paste, new, delete), new desktop files are sorted into vials by the rules, and a moved vial
//! saves its place.

mod engine;
mod field;
mod liquid;
mod desktops;
mod menu;
mod rules_window;
mod settings;
mod theme;
mod view;
mod win;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gpui::{
    point, px, size, AnyWindowHandle, App, AppContext, Application, Bounds, Entity, WindowBounds,
    WindowOptions,
};
use vials_core::gesture;
use vials_core::layout::{self, MonitorRef};
use vials_core::model::{Vial, VialsConfig};

use desqu_native_shell::{tray, Identity};
use engine::Engine;
use menu::MenuEntry;
use view::{bump, Live, Placement, Revision, ViewInit, VialView};
use vials_platform::{MouseEvent, MouseWatcher};

/// Who this app is to Windows (names of its mutex, tray window and startup entry are derived from it).
pub const ID: Identity = Identity { name: "Vials", key: "VialsGpui" };
/// Where the update manifest lives. Not published yet for this build: until it is, a check says it
/// could not reach one.
pub const MANIFEST_URL: &str = "https://wolfskii.github.io/Desqu/updates/vials-gpui/latest.json";
/// The tray menu: the order is what `Event::Menu(index)` refers to.
/// The last left press on the desktop, and once known whether it was on an icon.
type Press = Option<((i32, i32), Option<bool>)>;

const TRAY_MENU: [&str; 3] = ["New vial", "Settings", "Quit Vials"];

fn config_path() -> PathBuf {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--config" {
            if let Some(p) = args.next() {
                return PathBuf::from(p);
            }
        }
    }
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default();
    appdata.join("io.github.wolfskii.vials").join("config.json")
}

/// What opening a vial window needs.
#[derive(Clone)]
struct Shell {
    monitors: Vec<MonitorRef>,
    engine: Option<Arc<Engine>>,
    revision: Entity<Revision>,
    /// The look of the read-only viewer (it has no engine to ask).
    theme: vials_core::model::VialTheme,
    tint: u8,
}

type Windows = Rc<RefCell<HashMap<String, AnyWindowHandle>>>;

impl Shell {
    fn open_vial(&self, cx: &mut App, vial: &Vial) -> Option<AnyWindowHandle> {
        let monitor = layout::pick_monitor(vial.monitor.as_deref(), &self.monitors)?;
        let rect = layout::resolve(&vial.rect, &monitor.bounds);
        let height = layout::window_height(rect.height(), vial.collapsed, monitor.scale).max(1);
        let collapsed_height = layout::window_height(rect.height(), true, monitor.scale).max(1);
        let mut placed = rect;
        placed.bottom = placed.top + height;
        let (name, items, collapsed) = (vial.name.clone(), vial.items.clone(), vial.collapsed);
        let live = self.engine.as_ref().map(|e| Live { engine: Arc::clone(e), revision: self.revision.clone(), vial_id: vial.id.clone() });
        let title = name.clone();
        let look = match &self.engine {
            Some(e) => e.look(),
            None => theme::Theme::of(self.theme, self.tint),
        };

        // The size here is a placeholder: the exact physical rectangle is applied right after.
        let handle = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(0.), px(0.)), size(px(300.), px(200.))))),
                    titlebar: Some(gpui::TitlebarOptions { title: Some(title.into()), appears_transparent: true, ..Default::default() }),
                    focus: false,
                    is_resizable: true,
                    window_background: look.material,
                    ..Default::default()
                },
                move |window, cx| {
                    cx.new(|cx| VialView::new(window, ViewInit { name, ids: items, collapsed, placement: Placement { rect, collapsed_height }, live, theme: look }, cx))
                },
            )
            .ok()?;
        let _ = handle.update(cx, |_, window, _| {
            if let Some(hwnd) = win::hwnd(window) {
                win::make_vial_window(hwnd);
                win::place_at_bottom(hwnd, &placed);
            }
        });
        Some(handle.into())
    }

    /// Brings the open windows in line with the vials: new vials get a window, deleted ones lose it.
    fn sync(&self, cx: &mut App, windows: &Windows, vials: &[Vial]) {
        let wanted: Vec<&str> = vials.iter().map(|v| v.id.as_str()).collect();
        let gone: Vec<String> = windows.borrow().keys().filter(|id| !wanted.contains(&id.as_str())).cloned().collect();
        for id in gone {
            if let Some(handle) = windows.borrow_mut().remove(&id) {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            }
        }
        for vial in vials {
            if windows.borrow().contains_key(&vial.id) {
                continue;
            }
            if let Some(handle) = self.open_vial(cx, vial) {
                windows.borrow_mut().insert(vial.id.clone(), handle);
            }
        }
    }
}

/// Puts vial windows on the virtual desktops Workspaces tied them to (or back on the current one).
fn apply_moves(cx: &mut App, windows: &Windows, moves: Vec<(String, desktops::Move)>) {
    if moves.is_empty() {
        return;
    }
    let manager = match desktops::Manager::new() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };
    for (id, mv) in moves {
        let Some(handle) = windows.borrow().get(&id).copied() else { continue };
        let target = match mv {
            desktops::Move::To(desktop) => Some(desktop),
            desktops::Move::ToCurrent => workspaces_platform::current_desktop(),
        };
        let Some(target) = target else { continue };
        let _ = handle.update(cx, |_, window, _| {
            if let Some(hwnd) = win::hwnd(window) {
                if let Err(e) = manager.move_to(hwnd, &target) {
                    eprintln!("could not move vial {id} to desktop {target}: {e}");
                }
            }
        });
    }
}

fn main() {
    desqu_native_shell::stdio::protect();
    desqu_native_shell::power::stay_responsive();
    desqu_native_shell::crash::install();
    let path = config_path();
    let config = match VialsConfig::load(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("cannot read {}: {e}", path.display());
            std::process::exit(1);
        }
    };
    let live_mode = std::env::args().any(|a| a == "--live");
    let action = std::env::args().find_map(|a| a.strip_prefix("--action=").map(str::to_string));
    // Live mode is one running copy: a second launch (for example the desktop's "Create vial here"
    // entry, which runs `--action=create-vial`) hands its action to the first and exits.
    let _single = if live_mode {
        match tray::single_instance(&ID) {
            Some(s) => Some(s),
            None => {
                if let Some(action) = &action {
                    if !tray::forward_action(&ID, action) {
                        eprintln!("the running copy did not take the action");
                    }
                } else {
                    eprintln!("Vials is already running");
                }
                return;
            }
        }
    } else {
        None
    };
    let monitors = win::monitors();
    let engine = if live_mode {
        match Engine::start(&path, config.clone(), monitors.clone()) {
            Ok(e) => Some(e),
            Err(e) => {
                eprintln!("live mode is off: {e}");
                None
            }
        }
    } else {
        None
    };
    let engine_for_exit = engine.clone();
    Application::new().run(move |cx: &mut App| {
        // Without a tray (the read-only viewer) the app ends with its last window.
        if !live_mode {
            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
        }
        let revision = cx.new(|_| Revision(0));
        let shell = Shell { monitors, engine: engine.clone(), revision: revision.clone(), theme: config.theme, tint: config.glass_tint };
        let windows: Windows = Rc::new(RefCell::new(HashMap::new()));

        let first: Vec<Vial> = match &engine {
            Some(e) => e.vials(),
            None => config.vials.iter().filter(|v| !v.inbox).cloned().collect(),
        };
        shell.sync(cx, &windows, &first);

        let Some(engine) = engine else { return };
        // Vials added or deleted anywhere (menu, drop...) change the set of windows.
        {
            let (shell, windows, engine) = (shell.clone(), windows.clone(), engine.clone());
            cx.observe(&revision, move |_, cx| shell.sync(cx, &windows, &engine.vials())).detach();
        }
        // The desktop's files changed: rules sort new ones into vials, sorted vials keep their order.
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        engine.watch(move || {
            let _ = tx.send(());
        });
        let _ = engine.reconcile();
        let (engine_for_desktop, revision_for_desktop) = (engine.clone(), revision.clone());
        cx.spawn(async move |cx| loop {
            let (engine, revision) = (&engine_for_desktop, &revision_for_desktop);
            let mut changed = false;
            while rx.try_recv().is_ok() {
                changed = true;
            }
            if changed {
                let engine = engine.clone();
                // Debounce: Explorer reports a burst of events for one change.
                cx.background_executor().timer(std::time::Duration::from_millis(400)).await;
                while rx.try_recv().is_ok() {}
                if cx.background_executor().spawn(async move { engine.reconcile() }).await {
                    let revision = revision.clone();
                    let _ = cx.update(|cx| bump(&revision, cx));
                }
            }
            cx.background_executor().timer(std::time::Duration::from_millis(300)).await;
        })
        .detach();

        // The tray icon, second launches, and the mouse on the desktop.
        let (event_tx, event_rx) = std::sync::mpsc::channel::<tray::Event>();
        // `--no-tray-icon`: the Desqu shell shows the icon and tells this app what to do.
        let started = if std::env::args().any(|a| a == "--no-tray-icon") { tray::start_hidden(&ID, event_tx.clone()) } else { tray::start(&ID, &TRAY_MENU, event_tx.clone()) };
        let tray = started.map_err(|e| eprintln!("no tray icon: {e}")).ok();
        if action.is_some() {
            let _ = event_tx.send(tray::Event::Action(action.clone().unwrap_or_default()));
        }
        let (mouse_tx, mouse_rx) = std::sync::mpsc::channel::<MouseEvent>();
        // Whether the last left press on the desktop landed on an icon: then the drag that follows
        // moves icons (perhaps into a vial, where they vanish) and is never a box drawn for a vial.
        // Looked up at once, while the icon is still where it was pressed.
        let press: Arc<Mutex<Press>> = Arc::default();
        let press_hook = press.clone();
        let mouse = MouseWatcher::start(Box::new(move |e| {
            if let MouseEvent::DesktopDown(at) = e {
                *press_hook.lock().unwrap_or_else(|e| e.into_inner()) = Some((at, None));
                let press = press_hook.clone();
                std::thread::spawn(move || {
                    let hit = desqu_desktop::item_at(desqu_desktop::Point { x: at.0, y: at.1 });
                    let on_item = !matches!(hit, Ok(None));
                    let mut slot = press.lock().unwrap_or_else(|e| e.into_inner());
                    if matches!(*slot, Some((p, None)) if p == at) {
                        *slot = Some((at, Some(on_item)));
                    }
                });
                return;
            }
            let _ = mouse_tx.send(e);
        }))
        .map_err(|e| eprintln!("cannot watch the desktop for dragged boxes: {e}"))
        .ok();
        let config = engine.config();
        if config.context_menu {
            let exe = std::env::current_exe().ok();
            if let Err(e) = vials_platform::set_context_menu(exe.as_deref().and_then(|p| p.to_str())) {
                eprintln!("could not add the desktop's right-click entry: {e}");
            }
        }
        let context_point: Rc<Cell<Option<(i32, i32)>>> = Rc::new(Cell::new(None));
        let engine = engine.clone();
        cx.spawn(async move |cx| {
            // Owned by this task: dropped (tray icon removed, hook released) when the app ends.
            let _keep = (&tray, &mouse);
            // Workspaces can tie a vial to a virtual desktop: look at its config about once a second.
            let mut follower = desktops::Follower::new(desktops::workspaces_config_path());
            let mut tick = 0u32;
            loop {
                if tick.is_multiple_of(10) {
                    let ids: Vec<String> = windows.borrow().keys().cloned().collect();
                    if let Some(moves) = follower.poll(&ids) {
                        let windows = windows.clone();
                        let _ = cx.update(|cx| apply_moves(cx, &windows, moves));
                    }
                }
                tick = tick.wrapping_add(1);
                while let Ok(event) = event_rx.try_recv() {
                    match event {
                        tray::Event::Menu(2) => {
                            let _ = cx.update(|cx| cx.quit());
                        }
                        tray::Event::Menu(1) => {
                            let (engine, revision) = (engine.clone(), revision.clone());
                            let _ = cx.update(|cx| settings::open(cx, engine, revision));
                        }
                        tray::Event::Menu(0) => {
                            if engine.create_vial_at(None).is_some() {
                                let revision = revision.clone();
                                let _ = cx.update(|cx| bump(&revision, cx));
                            }
                        }
                        tray::Event::Action(name) if name == "create-vial" => {
                            if engine.create_vial_at(context_point.take()).is_some() {
                                let revision = revision.clone();
                                let _ = cx.update(|cx| bump(&revision, cx));
                            }
                        }
                        tray::Event::Action(name) if name == "settings" => {
                            let (engine, revision) = (engine.clone(), revision.clone());
                            let _ = cx.update(|cx| settings::open(cx, engine, revision));
                        }
                        tray::Event::Action(name) if name == "rules" => {
                            let (engine, revision) = (engine.clone(), revision.clone());
                            let _ = cx.update(|cx| rules_window::open(cx, engine, revision));
                        }
                        // From Desqu's page for Vials: `set-setting:<name>:<value>`.
                        tray::Event::Action(name) if name.starts_with("set-setting:") => {
                            if let Some((key, value)) = name.trim_start_matches("set-setting:").split_once(':') {
                                if let Err(e) = engine.config().set_setting(key, value) {
                                    eprintln!("{e}");
                                } else {
                                    let (key, value) = (key.to_string(), value.to_string());
                                    if engine.edit_settings(|c| drop(c.set_setting(&key, &value))) {
                                        let revision = revision.clone();
                                        let _ = cx.update(|cx| bump(&revision, cx));
                                    }
                                }
                            }
                        }
                        tray::Event::Action(name) if name == "quit" => {
                            let _ = cx.update(|cx| cx.quit());
                        }
                        tray::Event::Action(_) | tray::Event::Menu(_) => {}
                    }
                }
                while let Ok(event) = mouse_rx.try_recv() {
                    match event {
                        MouseEvent::ContextDown(at) => context_point.set(Some(at)),
                        MouseEvent::ButtonDown(_) | MouseEvent::DesktopDown(_) => {}
                        MouseEvent::Marquee(down, up) => {
                            // A box dragged on empty desktop offers a vial there; a box that caught icons
                            // is a selection, which Explorer needs a moment to settle.
                            let Some(rect) = gesture::marquee(down, up) else { continue };
                            if !engine.config().create_on_drag {
                                continue;
                            }
                            let (engine, revision, press) = (engine.clone(), revision.clone(), press.clone());
                            cx.spawn(async move |cx| {
                                cx.background_executor().timer(std::time::Duration::from_millis(350)).await;
                                // A drag that started on an icon (into a vial, say) is no box.
                                for _ in 0..20 {
                                    let seen = *press.lock().unwrap_or_else(|e| e.into_inner());
                                    match seen {
                                        Some((at, Some(true))) if at == down => return,
                                        Some((at, None)) if at == down => {
                                            cx.background_executor().timer(std::time::Duration::from_millis(25)).await;
                                        }
                                        _ => break,
                                    }
                                }
                                let empty = matches!(desqu_desktop::selection_count(), Ok(0));
                                if !empty {
                                    return;
                                }
                                let _ = cx.update(|cx| {
                                    let entry = MenuEntry::new("Create vial here", move |cx| {
                                        if engine.create_vial(rect).is_some() {
                                            bump(&revision, cx);
                                        }
                                    })
                                    .icon(menu::Icon::Add);
                                    menu::open(cx, up, vec![entry]);
                                });
                            })
                            .detach();
                        }
                    }
                }
                cx.background_executor().timer(std::time::Duration::from_millis(100)).await;
            }
        })
        .detach();
    });
    // The windows are gone: give the desktop its items back before the process ends.
    if let Some(e) = engine_for_exit {
        e.restore();
        let _ = vials_platform::set_context_menu(None);
    }
}
