//! Screenbound in GPUI: make fullscreen mean fullscreen inside the part of the screen you choose.
//!   screenbound-gpui [--config-dir <folder>] [--autostart] [--action=show|pause|resume|quit]
//! It lives in the tray; a second launch brings its window up (or hands over its `--action`).
//! Not in this window yet (the engine still applies them if they are in the config): groups, rules,
//! the browser integration, diagnostics.

mod model;
mod runtime;
mod view;

use std::path::PathBuf;
use std::sync::Arc;

use desqu_native_shell::{tray, Identity};
use gpui::{App, Application};

use runtime::Runtime;

/// Who this app is to Windows (names of its mutex, tray window and startup entry are derived from it).
pub const ID: Identity = Identity { name: "Screenbound", key: "ScreenboundGpui" };
/// Where the update manifest lives. Not published yet for this build.
pub const MANIFEST_URL: &str = "https://wolfskii.github.io/Desqu/updates/screenbound-gpui/latest.json";
const TRAY_MENU: [&str; 4] = ["Open Screenbound", "Pause or resume", "Release all windows", "Quit Screenbound"];

fn arg_value(name: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == name {
            return args.next();
        }
    }
    None
}

fn config_dir() -> PathBuf {
    if let Some(p) = arg_value("--config-dir") {
        return PathBuf::from(p);
    }
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default();
    appdata.join("io.github.wolfskii.screenbound")
}

fn main() {
    let action = std::env::args().find_map(|a| a.strip_prefix("--action=").map(str::to_string));
    let autostarted = std::env::args().any(|a| a == "--autostart");
    // One running copy: a second launch asks it to show its window (or to do its `--action`).
    let Some(_single) = tray::single_instance(&ID) else {
        let wanted = action.as_deref().unwrap_or("show");
        if !tray::forward_action(&ID, wanted) {
            eprintln!("the running copy did not take the action");
        }
        return;
    };
    // Asking something that is not running to quit or pause: nothing to do.
    if matches!(action.as_deref(), Some("quit" | "pause" | "resume")) {
        return;
    }

    let rt = match Runtime::start(&config_dir(), env!("CARGO_PKG_VERSION")) {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Screenbound could not start: {e}");
            std::process::exit(1);
        }
    };
    let rt_for_exit = rt.clone();
    Application::new().run(move |cx: &mut App| {
        let (event_tx, event_rx) = std::sync::mpsc::channel::<tray::Event>();
        // `--no-tray-icon`: the Desqu shell shows the icon and tells this app what to do.
        let started = if std::env::args().any(|a| a == "--no-tray-icon") { tray::start_hidden(&ID, event_tx.clone()) } else { tray::start(&ID, &TRAY_MENU, event_tx.clone()) };
        let tray = started.map_err(|e| eprintln!("no tray icon: {e}")).ok();
        if !autostarted {
            view::open(cx, Arc::clone(&rt));
        }
        cx.spawn(async move |cx| {
            let _keep = &tray;
            loop {
                while let Ok(event) = event_rx.try_recv() {
                    let rt = Arc::clone(&rt);
                    match event {
                        tray::Event::Menu(0) => {
                            let _ = cx.update(|cx| view::open(cx, rt));
                        }
                        tray::Event::Menu(1) => {
                            let enabled = rt.snapshot().config.as_ref().is_none_or(|c| c.enabled);
                            rt.change(move |c| {
                                c.enabled = !enabled;
                                true
                            });
                        }
                        tray::Event::Menu(2) => rt.release_all(),
                        tray::Event::Menu(3) => {
                            let _ = cx.update(|cx| cx.quit());
                        }
                        tray::Event::Action(name) => match name.as_str() {
                            "show" => {
                                let _ = cx.update(|cx| view::open(cx, rt));
                            }
                            "pause" | "resume" => {
                                let pause = name == "pause";
                                rt.change(move |c| {
                                    c.enabled = !pause;
                                    true
                                });
                            }
                            "quit" => {
                                let _ = cx.update(|cx| cx.quit());
                            }
                            _ => {}
                        },
                        tray::Event::Menu(_) => {}
                    }
                }
                cx.background_executor().timer(std::time::Duration::from_millis(100)).await;
            }
        })
        .detach();
    });
    // The window and tray are gone: give every managed window its place back before the process ends.
    rt_for_exit.stop();
}
