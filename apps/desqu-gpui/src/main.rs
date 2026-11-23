//! Desqu (GPUI): one tray icon and one window for the small desktop utilities. Each module (Vials,
//! Workspaces, Screenbound) is its own app that Desqu starts, stops and talks to; this program owns
//! the tray icon, the list of which modules are on, and starting them at sign-in.
//!   desqu-gpui [--config-dir <folder>] [--autostart] [--action=show|quit]

mod modules;
mod supervisor;
mod view;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use desqu_native_shell::{tray, Identity};
use gpui::{App, Application};

use modules::{Settings, ALL};
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
    let shell = Arc::new(Shell { supervisor: Arc::new(Supervisor::new(&exe_dir)), settings_path: settings_path.clone(), settings: Mutex::new(Settings::load(&settings_path)) });
    // The modules that are on start with Desqu.
    {
        let (supervisor, settings) = (shell.supervisor.clone(), shell.settings.lock().unwrap_or_else(|e| e.into_inner()).clone());
        std::thread::spawn(move || {
            for kind in ALL.into_iter().filter(|k| settings.enabled(*k)) {
                if let Err(e) = supervisor.start(kind) {
                    eprintln!("{e}");
                }
            }
        });
    }

    let shell_for_exit = shell.clone();
    let menu: Vec<String> = std::iter::once("Open Desqu".to_string()).chain(ALL.iter().map(|k| format!("Open {}", k.name()))).chain(std::iter::once("Quit Desqu".to_string())).collect();
    Application::new().run(move |cx: &mut App| {
        let (event_tx, event_rx) = std::sync::mpsc::channel::<tray::Event>();
        let labels: Vec<&str> = menu.iter().map(String::as_str).collect();
        let tray = tray::start(&ID, &labels, event_tx.clone()).map_err(|e| eprintln!("no tray icon: {e}")).ok();
        // A first run (or a plain launch) shows the window; a sign-in launch stays in the tray.
        if !autostarted || action.as_deref() == Some("show") {
            view::open(cx, shell.clone());
        }
        let last = ALL.len() + 1;
        cx.spawn(async move |cx| {
            let _keep = &tray;
            loop {
                while let Ok(event) = event_rx.try_recv() {
                    let shell = shell.clone();
                    match event {
                        tray::Event::Menu(0) => {
                            let _ = cx.update(|cx| view::open(cx, shell));
                        }
                        tray::Event::Menu(i) if i == last => {
                            let _ = cx.update(|cx| cx.quit());
                        }
                        tray::Event::Menu(i) => {
                            // "Open <module>": its own window, if it is running.
                            if let Some(kind) = ALL.get(i - 1) {
                                let kind = *kind;
                                std::thread::spawn(move || {
                                    shell.supervisor.open(kind);
                                });
                            }
                        }
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
