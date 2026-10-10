//! Single instance with `--quit` forwarding.
//!
//! Two instances would fight over the same windows, so a second launch never runs: its arguments
//! are forwarded to the first one, which shows its window or, for `--quit`, exits (running
//! `RunEvent::Exit`, so modules restore managed windows first).

use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Runtime};

use crate::args::{self, LaunchArgs};
use crate::window;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecondLaunch {
    Show,
    Quit,
    /// Hand the named action to the running modules; do not touch the window.
    Action(String),
}

pub fn decide(args: &LaunchArgs) -> SecondLaunch {
    if args.quit {
        SecondLaunch::Quit
    } else if let Some(name) = &args.action {
        SecondLaunch::Action(name.clone())
    } else {
        SecondLaunch::Show
    }
}

pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    tauri_plugin_single_instance::init(|app, argv, _cwd| {
        match decide(&args::parse(argv)) {
            SecondLaunch::Quit => app.exit(0),
            SecondLaunch::Show => window::show_main(app),
            SecondLaunch::Action(name) => deliver_action(app, &name),
        }
    })
}

/// Event a module can listen to for `--action=<name>` launches; the payload is the name.
pub const ACTION_EVENT: &str = "shell://action";

fn deliver_action<R: Runtime>(app: &AppHandle<R>, name: &str) {
    use tauri::Emitter;
    let _ = app.emit(ACTION_EVENT, name);
}

/// For the first instance, at the end of `setup` (after modules started): hands a `--action=<name>`
/// the app was started with to the modules.
pub fn deliver_launch_action<R: Runtime>(app: &AppHandle<R>, launch: &LaunchArgs) {
    if let Some(name) = &launch.action {
        deliver_action(app, name);
    }
}

/// For the first instance: `--quit` with nothing to quit means exit before any module starts.
/// Call at the very top of `setup`, before registering module plugins. Returns `true` if exiting.
pub fn exit_if_quit_requested<R: Runtime>(app: &AppHandle<R>, launch: &LaunchArgs) -> bool {
    if decide(launch) == SecondLaunch::Quit {
        app.exit(0);
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_second_launch_shows_the_window() {
        assert_eq!(decide(&LaunchArgs::default()), SecondLaunch::Show);
    }

    #[test]
    fn autostart_second_launch_still_shows() {
        let a = LaunchArgs { autostart: true, ..Default::default() };
        assert_eq!(decide(&a), SecondLaunch::Show);
    }

    #[test]
    fn quit_wins() {
        let a = LaunchArgs { autostart: true, quit: true, action: Some("x".into()) };
        assert_eq!(decide(&a), SecondLaunch::Quit);
    }

    #[test]
    fn an_action_is_forwarded_without_showing_the_window() {
        let a = LaunchArgs { action: Some("create-vial".into()), ..Default::default() };
        assert_eq!(decide(&a), SecondLaunch::Action("create-vial".into()));
    }
}
