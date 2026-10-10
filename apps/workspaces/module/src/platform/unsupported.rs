//! No backend: virtual desktops are only implemented for Windows.

use super::Desktops;
use crate::core::apps::RunningApp;

pub(super) struct Watcher;

const MSG: &str = "Workspaces is only available on Windows";

pub(super) fn desktops() -> Result<Desktops, String> {
    Err(MSG.into())
}

pub(super) fn watch(_on_change: Box<dyn Fn() + Send + 'static>) -> Result<Watcher, String> {
    Err(MSG.into())
}

pub(super) fn running_apps(_desktop_id: Option<&str>) -> Vec<RunningApp> {
    Vec::new()
}

pub(super) fn explorer_folders(_desktop_id: Option<&str>) -> Vec<String> {
    Vec::new()
}

pub(super) fn launch_app(_exe: &str, _args: Option<&str>) -> Result<(), String> {
    Err(MSG.into())
}

pub(super) fn open_folder(_path: &str) -> Result<(), String> {
    Err(MSG.into())
}
