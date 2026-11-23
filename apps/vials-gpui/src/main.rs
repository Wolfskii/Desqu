//! Vials in GPUI. Read-only viewer: it opens one window per vial of the real Vials config, in the
//! same place, with the same items and icons, but never writes the config or hides anything.
//!   vials-gpui [--config <path to config.json>]

mod view;
mod win;

use std::path::PathBuf;

use gpui::{point, px, size, App, AppContext, Application, Bounds, WindowBackgroundAppearance, WindowBounds, WindowOptions};
use vials_core::layout;
use vials_core::model::VialsConfig;

use view::{Placement, VialView};

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

fn main() {
    let path = config_path();
    let config = match VialsConfig::load(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("cannot read {}: {e}", path.display());
            std::process::exit(1);
        }
    };
    Application::new().run(move |cx: &mut App| {
        let monitors = win::monitors();
        for vial in config.vials.iter().filter(|v| !v.inbox) {
            let Some(monitor) = layout::pick_monitor(vial.monitor.as_deref(), &monitors) else { continue };
            let rect = layout::resolve(&vial.rect, &monitor.bounds);
            let height = layout::window_height(rect.height(), vial.collapsed, monitor.scale).max(1);
            let collapsed_height = layout::window_height(rect.height(), true, monitor.scale).max(1);
            let mut placed = rect;
            placed.bottom = placed.top + height;
            let (name, items, collapsed) = (vial.name.clone(), vial.items.clone(), vial.collapsed);
            let full = rect;
            let title = name.clone();

            // The size here is a placeholder: the exact physical rectangle is applied right after.
            let opened = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(0.), px(0.)), size(px(300.), px(200.))))),
                    titlebar: Some(gpui::TitlebarOptions { title: Some(title.into()), appears_transparent: true, ..Default::default() }),
                    focus: false,
                    is_resizable: true,
                    window_background: WindowBackgroundAppearance::Blurred,
                    ..Default::default()
                },
                move |_, cx| {
                    cx.new(|cx| VialView::new(&name, items, collapsed, Placement { rect: full, collapsed_height }, cx))
                },
            );
            if let Ok(handle) = opened {
                let _ = handle.update(cx, |_, window, _| {
                    if let Some(hwnd) = win::hwnd(window) {
                        win::make_vial_window(hwnd);
                        win::place_at_bottom(hwnd, &placed);
                    }
                });
            }
        }
    });
}
