//! Screenbound in GPUI: make fullscreen mean fullscreen inside the part of the screen you choose.
//!   screenbound-gpui [--config-dir <folder>] [--autostart] [--action=show|pause|resume|quit] [--unregister-browser]
//! It lives in the tray; a second launch brings its window up (or hands over its `--action`).

// A program with no console: started from the desktop menu, the tray or a sign-in it must not open a terminal.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Arc;

use desqu_native_shell::tray;
use gpui::{App, Application};

use screenbound_gpui::runtime::{self, Backend, Runtime};
use screenbound_gpui::{view, ID};

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
    desqu_native_shell::stdio::protect();
    desqu_native_shell::power::stay_responsive();
    desqu_native_shell::crash::install();
    // Uninstalling: take the browser entries away (the installer runs this before deleting the files).
    if std::env::args().any(|a| a == "--unregister-browser") {
        if let Err(e) = runtime::unregister_host() {
            eprintln!("{e}");
        }
        return;
    }
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
    // The pre-rebrand settings folder is only looked at for the real settings location, never for a test folder.
    let legacy = if arg_value("--config-dir").is_none() { std::env::var_os("APPDATA").map(|a| screenbound_module::migrate::legacy_dir(std::path::Path::new(&a))) } else { None };
    let rt = match Runtime::start(&config_dir(), env!("CARGO_PKG_VERSION"), legacy.as_deref()) {
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
            view::open(cx, rt.clone() as Arc<dyn Backend>);
        }
        cx.spawn(async move |cx| {
            let _keep = &tray;
            // The zone hotkey follows the setting: registered, changed or given back as it changes.
            let mut hotkey: Option<(String, desqu_native_shell::hotkey::Hotkey)> = None;
            let (snap_tx, snap_rx) = std::sync::mpsc::channel::<()>();
            let mut tick = 0u32;
            loop {
                if tick.is_multiple_of(10) {
                    let wanted = rt.snapshot().config.as_ref().map(|c| c.zone_hotkey.clone()).unwrap_or_default();
                    if hotkey.as_ref().map_or("", |(text, _)| text.as_str()) != wanted {
                        hotkey = None;
                        if !wanted.is_empty() {
                            let for_press = snap_tx.clone();
                            match desqu_native_shell::hotkey::Hotkey::register(&wanted, Box::new(move || {
                                let _ = for_press.send(());
                            })) {
                                Ok(h) => hotkey = Some((wanted, h)),
                                Err(e) => eprintln!("{e}"),
                            }
                        }
                    }
                }
                tick = tick.wrapping_add(1);
                while snap_rx.try_recv().is_ok() {
                    rt.snap_foreground();
                }
                while let Ok(event) = event_rx.try_recv() {
                    let rt = Arc::clone(&rt);
                    match event {
                        tray::Event::Menu(0) => {
                            let _ = cx.update(|cx| view::open(cx, rt as Arc<dyn Backend>));
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
                                let _ = cx.update(|cx| view::open(cx, rt as Arc<dyn Backend>));
                            }
                            "pause" | "resume" => {
                                let pause = name == "pause";
                                rt.change(move |c| {
                                    c.enabled = !pause;
                                    true
                                });
                            }
                            // From the Desqu window: it changed the saved config, or asks for what only this process can do.
                            "release" => rt.release_all(),
                            "sync-browser" => rt.sync_browser(),
                            other if other.starts_with("replace-config:") => {
                                let file = std::path::PathBuf::from(other.trim_start_matches("replace-config:"));
                                match screenbound_module::core::config::AppConfig::load(&file) {
                                    Ok(new) => rt.change(move |c| {
                                        *c = new;
                                        true
                                    }),
                                    Err(e) => eprintln!("could not read the settings from Desqu: {e}"),
                                }
                                let _ = std::fs::remove_file(&file);
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
