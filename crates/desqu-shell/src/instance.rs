//! Single instance with `--quit` forwarding.
//!
//! Two instances would fight over the same windows, so a second launch never runs: its arguments
//! are forwarded to the first one, which shows its window or, for `--quit`, exits (running
//! `RunEvent::Exit`, so modules restore managed windows first).

use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Runtime};

use crate::args::{self, LaunchArgs};
use crate::window;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecondLaunch {
    Show,
    Quit,
}

pub fn decide(args: &LaunchArgs) -> SecondLaunch {
    if args.quit {
        SecondLaunch::Quit
    } else {
        SecondLaunch::Show
    }
}

pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    tauri_plugin_single_instance::init(|app, argv, _cwd| {
        match decide(&args::parse(argv)) {
            SecondLaunch::Quit => app.exit(0),
            SecondLaunch::Show => window::show_main(app),
        }
    })
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
        let a = LaunchArgs { autostart: true, quit: false };
        assert_eq!(decide(&a), SecondLaunch::Show);
    }

    #[test]
    fn quit_wins() {
        let a = LaunchArgs { autostart: true, quit: true };
        assert_eq!(decide(&a), SecondLaunch::Quit);
    }
}
