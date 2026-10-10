//! Taking a module over from its standalone app (decision D2).
//!
//! Both can be installed and share one settings folder, but only one process may run the engine
//! (the module lock). When Desqu finds the standalone app running, the user is asked, and on yes
//! the standalone app is told to quit (`--quit`, which restores its managed windows), Desqu starts
//! the module, and the standalone app's "start with system" entry is removed so it does not come
//! back at the next sign-in.

use std::path::{Path, PathBuf};
use std::time::Duration;

use desqu_shell::platform::{self, ProcessInfo};
use desqu_shell::settings;
use tauri::{AppHandle, Wry};

use crate::registry::Entry;

const QUIT_WAIT_STEP: Duration = Duration::from_millis(250);
const QUIT_WAIT_STEPS: u32 = 20; // 5 s in total

/// Executables of the standalone app's *other* running processes that we can ask to quit.
pub fn quit_targets(own_pid: u32, procs: &[ProcessInfo]) -> Vec<PathBuf> {
    let mut targets: Vec<PathBuf> = procs
        .iter()
        .filter(|p| p.pid != own_pid)
        .filter_map(|p| p.exe.clone())
        .collect();
    targets.sort();
    targets.dedup();
    targets
}

/// Stops the standalone app from coming back at sign-in: removes its Run entry and clears the
/// "start with system" flag in its shell settings (so it would not re-register itself either).
pub fn disable_standalone_autostart(standalone_dir: &Path, run_value: &str) -> Result<(), String> {
    platform::remove_run_value(run_value)?;
    let path = standalone_dir.join(desqu_shell::plugin::SETTINGS_FILE);
    if path.exists() {
        let mut shell = settings::load(&path).map_err(|e| e.to_string())?;
        if shell.start_with_system {
            shell.start_with_system = false;
            settings::save(&path, &shell).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Asks the standalone app to quit, then starts the module here. Blocking (bounded to ~5 s):
/// run it off the UI thread.
pub fn take_over(app: &AppHandle<Wry>, entry: &Entry, standalone_dir: &Path) -> Result<(), String> {
    let procs = platform::find_processes(entry.descriptor.standalone.exe_name);
    for exe in quit_targets(std::process::id(), &procs) {
        // A second launch with --quit is forwarded to the running instance, which exits.
        std::process::Command::new(&exe)
            .arg("--quit")
            .spawn()
            .map_err(|e| format!("could not ask {} to quit: {e}", exe.display()))?;
    }

    // The lock frees up once the standalone process is gone; retry for a bounded time.
    let mut last = String::new();
    for _ in 0..QUIT_WAIT_STEPS {
        match entry.module.start(app) {
            Ok(()) => {
                if let Err(e) =
                    disable_standalone_autostart(standalone_dir, entry.descriptor.standalone.run_value)
                {
                    tracing::warn!("could not disable the standalone autostart: {e}");
                }
                return Ok(());
            }
            Err(e) => last = e,
        }
        std::thread::sleep(QUIT_WAIT_STEP);
    }
    Err(format!(
        "{} did not quit in time ({last}). Close it from its tray icon and try again.",
        entry.descriptor.name
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, exe: Option<&str>) -> ProcessInfo {
        ProcessInfo { pid, exe: exe.map(PathBuf::from) }
    }

    #[test]
    fn quit_targets_skip_ourselves_and_unknown_paths() {
        let procs = [proc(1, Some(r"C:\a\screenbound.exe")), proc(2, None), proc(3, Some(r"C:\b\screenbound.exe"))];
        let targets = quit_targets(3, &procs);
        assert_eq!(targets, vec![PathBuf::from(r"C:\a\screenbound.exe")]);
    }

    #[test]
    fn quit_targets_dedup_the_same_exe() {
        let procs = [proc(1, Some(r"C:\a\screenbound.exe")), proc(2, Some(r"C:\a\screenbound.exe"))];
        assert_eq!(quit_targets(99, &procs).len(), 1);
    }

    #[test]
    fn disabling_standalone_autostart_clears_the_flag_in_its_shell_settings() {
        let dir = std::env::temp_dir().join(format!("desqu-takeover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(desqu_shell::plugin::SETTINGS_FILE);
        let on = settings::ShellSettings { start_with_system: true, ..Default::default() };
        settings::save(&path, &on).unwrap();

        // A run value name nobody has: removing it is not an error.
        disable_standalone_autostart(&dir, "DesquTakeoverTestNoSuchValue").unwrap();
        assert!(!settings::load(&path).unwrap().start_with_system);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
