//! Vials in GPUI. Read-only viewer: it opens one window per vial of the real Vials config, in the
//! same place, with the same items and icons, but never writes the config or hides anything.
//!   vials-gpui [--config <path to config.json>] [--live]
//! With `--live` it also owns the desktop: items in a vial are hidden on the desktop (put back on
//! exit, or from the journal after a crash), vials can be edited (drop, drag out, rename, sort,
//! paste, new, delete), new desktop files are sorted into vials by the rules, and a moved vial
//! saves its place.

mod engine;
mod menu;
mod theme;
mod tray;
mod view;
mod win;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    point, px, size, AnyWindowHandle, App, AppContext, Application, Bounds, Entity, WindowBounds,
    WindowOptions,
};
use vials_core::gesture;
use vials_core::layout::{self, MonitorRef};
use vials_core::model::{Vial, VialsConfig};

use engine::Engine;
use menu::MenuEntry;
use view::{bump, Live, Placement, Revision, VialView};
use vials_platform::{MouseEvent, MouseWatcher};

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
                    cx.new(|cx| VialView::new(window, &name, items, collapsed, Placement { rect, collapsed_height }, live, look, cx))
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

fn main() {
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
        match tray::single_instance() {
            Some(s) => Some(s),
            None => {
                if let Some(action) = &action {
                    if !tray::forward_action(action) {
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
        let tray = tray::start(event_tx.clone()).map_err(|e| eprintln!("no tray icon: {e}")).ok();
        if action.is_some() {
            let _ = event_tx.send(tray::Event::Action(action.clone().unwrap_or_default()));
        }
        let (mouse_tx, mouse_rx) = std::sync::mpsc::channel::<MouseEvent>();
        let mouse = MouseWatcher::start(Box::new(move |e| {
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
            loop {
                while let Ok(event) = event_rx.try_recv() {
                    match event {
                        tray::Event::Quit => {
                            let _ = cx.update(|cx| cx.quit());
                        }
                        tray::Event::NewVial => {
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
                        tray::Event::Action(name) if name == "quit" => {
                            let _ = cx.update(|cx| cx.quit());
                        }
                        tray::Event::Action(_) => {}
                    }
                }
                while let Ok(event) = mouse_rx.try_recv() {
                    match event {
                        MouseEvent::ContextDown(at) => context_point.set(Some(at)),
                        MouseEvent::ButtonDown(_) => {}
                        MouseEvent::Marquee(down, up) => {
                            // A box dragged on empty desktop offers a vial there; a box that caught icons
                            // is a selection, which Explorer needs a moment to settle.
                            let Some(rect) = gesture::marquee(down, up) else { continue };
                            if !engine.config().create_on_drag {
                                continue;
                            }
                            let (engine, revision) = (engine.clone(), revision.clone());
                            cx.spawn(async move |cx| {
                                cx.background_executor().timer(std::time::Duration::from_millis(350)).await;
                                let empty = matches!(desqu_desktop::selection_count(), Ok(0));
                                if !empty {
                                    return;
                                }
                                let _ = cx.update(|cx| {
                                    let entry = MenuEntry::new("Create vial here", move |cx| {
                                        if engine.create_vial(rect).is_some() {
                                            bump(&revision, cx);
                                        }
                                    });
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
