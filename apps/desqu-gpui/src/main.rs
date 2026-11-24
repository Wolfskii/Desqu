//! Desqu (GPUI): one tray icon and one window for the small desktop utilities. Each module (Vials,
//! Workspaces, Screenbound) is its own app that Desqu starts, stops and talks to; this program owns
//! the tray icon, the list of which modules are on, and starting them at sign-in.
//!   desqu-gpui [--config-dir <folder>] [--autostart] [--action=show|quit]

// A program with no console: started from the desktop menu, the tray or a sign-in it must not open a terminal.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_settings;
mod backup;
mod assets;
mod hotkeys;
mod modules;
mod palette;
mod report;
mod screenbound_remote;
mod supervisor;
mod tour;
mod workspaces_page;
mod view;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use desqu_native_shell::{tray, Identity};
use gpui::{App, Application};

use modules::Settings;
use supervisor::Supervisor;
use view::Shell;

/// Who this app is to Windows (names of its mutex, tray window and startup entry are derived from it).
pub const ID: Identity = Identity { name: "Desqu", key: "DesquGpui" };
/// Where the update manifest lives. Not published yet for this build.
pub const MANIFEST_URL: &str = "https://wolfskii.github.io/Desqu/updates/desqu-gpui/latest.json";

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
    appdata.join("io.github.wolfskii.desqu")
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
    if action.as_deref() == Some("quit") {
        return;
    }

    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)).unwrap_or_default();
    let settings_path = config_dir().join("gpui-shell.json");
    let shell = Arc::new(Shell { supervisor: Arc::new(Supervisor::new(&exe_dir)), settings_path: settings_path.clone(), settings: Mutex::new(Settings::load(&settings_path)), nav: Mutex::new(None) });
    // A copy of the settings once a day (the newest 14 are kept).
    {
        let also = shell.settings.lock().unwrap_or_else(|e| e.into_inner()).backup_onedrive;
        std::thread::spawn(move || {
            if backup::is_due(&backup::folder(), backup::now_secs(), 86_400) {
                if let Err(e) = backup::back_up(&backup::Paths::real(), also) {
                    eprintln!("no automatic backup: {e}");
                }
            }
        });
    }
    // The modules that are on start with Desqu (from sign-in: only those marked "start with system").
    {
        let (supervisor, settings) = (shell.supervisor.clone(), shell.settings.lock().unwrap_or_else(|e| e.into_inner()).clone());
        view::sync_autostart(&settings);
        std::thread::spawn(move || {
            for kind in settings.to_start(autostarted) {
                if let Err(e) = supervisor.start(kind) {
                    eprintln!("{e}");
                }
            }
        });
    }

    let shell_for_exit = shell.clone();
    // The apps have no window of their own under Desqu: their settings are on their pages here.
    let menu: Vec<String> = vec!["Open Desqu".to_string(), "Quit Desqu".to_string()];
    Application::new().with_assets(assets::Assets).run(move |cx: &mut App| {
        let (event_tx, event_rx) = std::sync::mpsc::channel::<tray::Event>();
        let labels: Vec<&str> = menu.iter().map(String::as_str).collect();
        let tray = tray::start(&ID, &labels, event_tx.clone()).map_err(|e| eprintln!("no tray icon: {e}")).ok();
        // A first run (or a plain launch) shows the window; a sign-in launch stays in the tray.
        if !autostarted || action.as_deref() == Some("show") {
            view::open(cx, shell.clone());
        }
        let last = 1;
        cx.spawn(async move |cx| {
            let _keep = &tray;
            // The command palette's hotkey follows the setting.
            let mut palette_key: Option<(String, desqu_native_shell::hotkey::Hotkey)> = None;
            let (palette_tx, palette_rx) = std::sync::mpsc::channel::<()>();
            let mut tick = 0u32;
            loop {
                if tick.is_multiple_of(10) {
                    let wanted = shell.settings.lock().unwrap_or_else(|e| e.into_inner()).palette_keys().to_string();
                    if palette_key.as_ref().map_or("", |(text, _)| text.as_str()) != wanted {
                        palette_key = None;
                        if !wanted.is_empty() {
                            let for_press = palette_tx.clone();
                            match desqu_native_shell::hotkey::Hotkey::register(&wanted, Box::new(move || {
                                let _ = for_press.send(());
                            })) {
                                Ok(h) => palette_key = Some((wanted, h)),
                                Err(e) => eprintln!("{e}"),
                            }
                        }
                    }
                }
                tick = tick.wrapping_add(1);
                while palette_rx.try_recv().is_ok() {
                    let shell = shell.clone();
                    let _ = cx.update(|cx| palette::toggle(cx, shell));
                }
                while let Ok(event) = event_rx.try_recv() {
                    let shell = shell.clone();
                    match event {
                        tray::Event::Menu(0) => {
                            let _ = cx.update(|cx| view::open(cx, shell));
                        }
                        tray::Event::Menu(i) if i == last => {
                            let _ = cx.update(|cx| cx.quit());
                        }
                        tray::Event::Menu(_) => {}
                        tray::Event::Action(name) if name == "show" => {
                            let _ = cx.update(|cx| view::open(cx, shell));
                        }
                        tray::Event::Action(name) if name == "quit" => {
                            let _ = cx.update(|cx| cx.quit());
                        }
                        tray::Event::Action(_) => {}
                    }
                }
                cx.background_executor().timer(std::time::Duration::from_millis(100)).await;
            }
        })
        .detach();
    });
    // Quitting Desqu quits the modules it started: they put the desktop and the windows back first.
    shell_for_exit.supervisor.stop_all();
}
