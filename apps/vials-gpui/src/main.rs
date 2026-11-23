//! Vials in GPUI. Read-only viewer: it opens one window per vial of the real Vials config, in the
//! same place, with the same items and icons, but never writes the config or hides anything.
//!   vials-gpui [--config <path to config.json>] [--live]
//! With `--live` it also owns the desktop: items in a vial are hidden on the desktop (put back on
//! exit, or from the journal after a crash), vials can be edited (drop, drag out, rename, sort,
//! paste, new, delete), new desktop files are sorted into vials by the rules, and a moved vial
//! saves its place.

mod engine;
mod menu;
mod view;
mod win;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    point, px, size, AnyWindowHandle, App, AppContext, Application, Bounds, Entity, WindowBackgroundAppearance, WindowBounds,
    WindowOptions,
};
use vials_core::layout::{self, MonitorRef};
use vials_core::model::{Vial, VialsConfig};

use engine::Engine;
use view::{bump, Live, Placement, Revision, VialView};

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

        // The size here is a placeholder: the exact physical rectangle is applied right after.
        let handle = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(0.), px(0.)), size(px(300.), px(200.))))),
                    titlebar: Some(gpui::TitlebarOptions { title: Some(title.into()), appears_transparent: true, ..Default::default() }),
                    focus: false,
                    is_resizable: true,
                    window_background: WindowBackgroundAppearance::Blurred,
                    ..Default::default()
                },
                move |window, cx| {
                    cx.new(|cx| VialView::new(window, &name, items, collapsed, Placement { rect, collapsed_height }, live, cx))
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
    let monitors = win::monitors();
    let engine = if std::env::args().any(|a| a == "--live") {
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
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let revision = cx.new(|_| Revision(0));
        let shell = Shell { monitors, engine: engine.clone(), revision: revision.clone() };
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
        cx.spawn(async move |cx| loop {
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
    });
    // The windows are gone: give the desktop its items back before the process ends.
    if let Some(e) = engine_for_exit {
        e.restore();
    }
}
