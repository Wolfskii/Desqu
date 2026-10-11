//! No backend: virtual desktops are only implemented for Windows.

use super::Desktops;

pub(super) struct Watcher;

const MSG: &str = "Workspaces is only available on Windows";

pub(super) struct ForegroundWatcher;

impl ForegroundWatcher {
    pub(super) fn start(_on_change: Box<dyn Fn() + Send + 'static>) -> Result<Self, String> {
        Err(MSG.into())
    }
}

pub(super) type PasteHandler = Box<dyn Fn(Vec<String>) + Send>;

pub(super) struct PasteWatcher;

impl PasteWatcher {
    pub(super) fn start(_on_paste: PasteHandler) -> Result<Self, String> {
        Err(MSG.into())
    }
}

pub(super) fn desktops() -> Result<Desktops, String> {
    Err(MSG.into())
}

pub(super) fn watch(_on_change: Box<dyn Fn() + Send + 'static>) -> Result<Watcher, String> {
    Err(MSG.into())
}
