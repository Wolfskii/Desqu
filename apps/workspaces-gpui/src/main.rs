//! Workspaces in GPUI: give each Windows virtual desktop its own desktop icons.
//!   workspaces-gpui [--config <path to config.json>] [--autostart] [--action=show|quit]
//! It lives in the tray; a second launch brings its window up (or hands over its `--action`).

// A program with no console: started from the desktop menu, the tray or a sign-in it must not open a terminal.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod engine;
mod view;

use std::path::PathBuf;
use std::sync::Arc;

use desqu_native_shell::{tray, Identity};
use gpui::{App, Application};

use engine::Engine;

/// Who this app is to Windows (names of its mutex, tray window and startup entry are derived from it).
pub const ID: Identity = Identity { name: "Workspaces", key: "WorkspacesGpui" };
/// Where the update manifest lives. Not published yet for this build.
pub const MANIFEST_URL: &str = "https://wolfskii.github.io/Desqu/updates/workspaces-gpui/latest.json";
const TRAY_MENU: [&str; 2] = ["Open Workspaces", "Quit Workspaces"];

fn arg_value(name: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == name {
            return args.next();
        }
    }
    None
}

fn config_path() -> PathBuf {
    if let Some(p) = arg_value("--config") {
        return PathBuf::from(p);
    }
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default();
    appdata.join("io.github.wolfskii.workspaces").join("config.json")
}

fn main() {
    desqu_native_shell::stdio::protect();
    desqu_native_shell::power::stay_responsive();
    desqu_native_shell::crash::install();
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
    // Quitting something that is not running: nothing to do.
    if action.as_deref() == Some("quit") {
        return;
    }

    let path = config_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let engine = match Engine::start(&path, Box::new(|| workspaces_platform::desktops().unwrap_or_default())) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Workspaces could not start: {e}");
            std::process::exit(1);
        }
    };
    // Task View hides the desktop icons while it shows; the clipboard decides whether the desktop's
    // right-click menu offers "Paste on this workspace".
    engine.watch_shell();
    let engine_for_exit = engine.clone();
    Application::new().run(move |cx: &mut App| {
        let (event_tx, event_rx) = std::sync::mpsc::channel::<tray::Event>();
        // `--no-tray-icon`: the Desqu shell shows the icon and tells this app what to do.
        let started = if std::env::args().any(|a| a == "--no-tray-icon") { tray::start_hidden(&ID, event_tx.clone()) } else { tray::start(&ID, &TRAY_MENU, event_tx.clone()) };
        let tray = started.map_err(|e| eprintln!("no tray icon: {e}")).ok();
        // A first run (or a plain launch) shows the window; a sign-in launch stays in the tray.
        if !autostarted || action.as_deref() == Some("show") {
            view::open(cx, Arc::clone(&engine));
        }
        cx.spawn(async move |cx| {
            let _keep = &tray;
            loop {
                while let Ok(event) = event_rx.try_recv() {
                    let engine = Arc::clone(&engine);
                    match event {
                        tray::Event::Menu(0) => {
                            let _ = cx.update(|cx| view::open(cx, engine));
                        }
                        tray::Event::Menu(1) => {
                            let _ = cx.update(|cx| cx.quit());
                        }
                        tray::Event::Action(name) if name == "show" => {
                            let _ = cx.update(|cx| view::open(cx, engine));
                        }
                        tray::Event::Action(name) if name == "quit" => {
                            let _ = cx.update(|cx| cx.quit());
                        }
                        // The desktop's right-click entry: paste the copied or cut files onto this desktop.
                        tray::Event::Action(name) if name == "paste-here" => {
                            std::thread::spawn(move || {
                                if let Err(e) = engine.paste_here() {
                                    eprintln!("could not paste on this workspace: {e}");
                                }
                            });
                        }
                        // From Desqu's page for Workspaces: `set-setting:autoAssignNew:<true|false>`.
                        tray::Event::Action(name) if name.starts_with("set-setting:autoAssignNew:") => {
                            engine.set_auto_assign(name.ends_with(":true"));
                        }
                        // From Desqu's Workspaces page: `assign-item:<item>:<desktop id or all>`,
                        // `release-desktop:<desktop id>` and `first-run:<desktop id or none>`.
                        tray::Event::Action(name) if name.starts_with("assign-item:") => {
                            if let Some((item, target)) = name.trim_start_matches("assign-item:").rsplit_once(':') {
                                let desktop = (target != "all").then_some(target);
                                if let Err(e) = engine.assign_item(item, desktop) {
                                    eprintln!("could not move {item} to {target}: {e}");
                                }
                            }
                        }
                        tray::Event::Action(name) if name.starts_with("release-desktop:") => {
                            engine.release_desktop(name.trim_start_matches("release-desktop:"));
                        }
                        tray::Event::Action(name) if name.starts_with("first-run:") => {
                            let target = name.trim_start_matches("first-run:");
                            if let Err(e) = engine.answer_first_run((target != "none").then_some(target)) {
                                eprintln!("first run: {e}");
                            }
                        }
                        // From Vials' right-click menu: `assign-vial:<vial id>:<virtual desktop id, or all>`.
                        tray::Event::Action(name) if name.starts_with("assign-vial:") => {
                            let rest = name.trim_start_matches("assign-vial:");
                            if let Some((vial, target)) = rest.split_once(':') {
                                let item = workspaces_core::model::vial_item(vial);
                                let desktop = (target != "all").then_some(target);
                                if let Err(e) = engine.assign_item(&item, desktop) {
                                    eprintln!("could not tie vial {vial} to {target}: {e}");
                                }
                            }
                        }
                        _ => {}
                    }
                }
                cx.background_executor().timer(std::time::Duration::from_millis(100)).await;
            }
        })
        .detach();
    });
    // The window and tray are gone: give the desktop its icons back before the process ends.
    engine_for_exit.stop();
}
