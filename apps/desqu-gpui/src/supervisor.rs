//! Starts, stops and talks to the module apps (Vials, Workspaces, Screenbound), each its own
//! process. A module that crashes does not take the others down; a module already running on its own
//! (started by hand) is recognised and left alone.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use desqu_native_shell::tray;

use crate::modules::Kind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Off,
    /// Started by Desqu and alive.
    Running,
    /// Running, but not started by Desqu (a standalone copy).
    RunningElsewhere,
    /// Desqu started it and it ended on its own.
    Stopped,
    /// Its program is not next to Desqu.
    Missing,
}

pub struct Supervisor {
    dir: PathBuf,
    children: Mutex<HashMap<Kind, Child>>,
    /// Modules whose child ended without being asked to.
    crashed: Mutex<Vec<Kind>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Where a module's output and exit code are kept: `%APPDATA%\io.github.wolfskii.desqu\logs\gpui-<module>.log`.
pub fn log_path(kind: Kind) -> PathBuf {
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default();
    appdata.join("io.github.wolfskii.desqu").join("logs").join(format!("gpui-{}.log", kind.name().to_lowercase()))
}

/// Opens the module's log for appending (with a line saying it started), or `None` if that is not possible.
fn open_log(kind: Kind) -> Option<std::fs::File> {
    use std::io::Write;
    let path = log_path(kind);
    std::fs::create_dir_all(path.parent()?).ok()?;
    // A log that grew past 1 MB starts over.
    if std::fs::metadata(&path).map(|m| m.len() > 1_000_000).unwrap_or(false) {
        let _ = std::fs::remove_file(&path);
    }
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path).ok()?;
    let _ = writeln!(file, "--- started {:?}", std::time::SystemTime::now());
    Some(file)
}

/// Whether some copy of the module's app is running: its single-instance mutex is taken.
fn running_anywhere(kind: Kind) -> bool {
    tray::single_instance(&kind.identity()).is_none()
}

impl Supervisor {
    /// `dir` is where the module apps are (next to Desqu's own program).
    pub fn new(dir: &Path) -> Self {
        Self { dir: dir.to_path_buf(), children: Mutex::new(HashMap::new()), crashed: Mutex::new(Vec::new()) }
    }

    fn exe(&self, kind: Kind) -> PathBuf {
        self.dir.join(kind.exe())
    }

    pub fn status(&self, kind: Kind) -> Status {
        let mut children = lock(&self.children);
        if let Some(child) = children.get_mut(&kind) {
            match child.try_wait() {
                Ok(None) => return Status::Running,
                ended => {
                    use std::io::Write;
                    if let (Ok(Some(code)), Ok(mut log)) = (ended, std::fs::OpenOptions::new().append(true).open(log_path(kind))) {
                        let _ = writeln!(log, "--- ended: {code}");
                    }
                    children.remove(&kind);
                    let mut crashed = lock(&self.crashed);
                    if !crashed.contains(&kind) {
                        crashed.push(kind);
                    }
                }
            }
        }
        if lock(&self.crashed).contains(&kind) && !running_anywhere(kind) {
            return Status::Stopped;
        }
        if running_anywhere(kind) {
            return Status::RunningElsewhere;
        }
        if !self.exe(kind).is_file() {
            return Status::Missing;
        }
        Status::Off
    }

    /// Starts the module's app (no tray icon of its own, no window). A copy that is already running is left alone.
    pub fn start(&self, kind: Kind) -> Result<(), String> {
        lock(&self.crashed).retain(|k| *k != kind);
        if lock(&self.children).contains_key(&kind) || running_anywhere(kind) {
            return Ok(());
        }
        let exe = self.exe(kind);
        if !exe.is_file() {
            return Err(format!("{} is not next to Desqu", kind.exe()));
        }
        // Its output (a panic message, an error) goes to a log file, so a crash can be explained afterwards.
        let mut command = Command::new(&exe);
        command.args(kind.start_args()).env("RUST_BACKTRACE", "1");
        if let Some(log) = open_log(kind) {
            if let Ok(err) = log.try_clone() {
                command.stderr(err);
            }
            command.stdout(log);
        }
        let child = command.spawn().map_err(|e| format!("could not start {}: {e}", kind.name()))?;
        lock(&self.children).insert(kind, child);
        Ok(())
    }

    /// Asks the module to quit (it puts everything back first), and ends it if it takes too long.
    pub fn stop(&self, kind: Kind) {
        lock(&self.crashed).retain(|k| *k != kind);
        let child = lock(&self.children).remove(&kind);
        let id = kind.identity();
        if child.is_none() && !running_anywhere(kind) {
            return;
        }
        let _ = tray::forward_action(&id, "quit");
        if let Some(mut child) = child {
            let deadline = Instant::now() + Duration::from_secs(6);
            while Instant::now() < deadline {
                if matches!(child.try_wait(), Ok(Some(_))) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            // It did not quit by itself: its journal restores what it had changed at its next start.
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    pub fn stop_all(&self) {
        for kind in crate::modules::ALL {
            self.stop(kind);
        }
    }
}
