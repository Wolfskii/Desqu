//! No backend: every call reports that the layer is unsupported.

use std::path::Path;
use std::sync::Arc;

use crate::layer::{DesktopError, DesktopLayer, DesktopResult};
use crate::model::Point;

pub(crate) fn open(_owner: &str, _journal_path: &Path) -> DesktopResult<Arc<dyn DesktopLayer>> {
    Err(DesktopError::Unsupported)
}

pub(crate) fn recover_from(_journal_path: &Path) -> DesktopResult<usize> {
    Err(DesktopError::Unsupported)
}

pub(crate) fn item_position(_item_id: &str) -> DesktopResult<Option<Point>> {
    Err(DesktopError::Unsupported)
}

pub(crate) fn selection_count() -> DesktopResult<usize> {
    Err(DesktopError::Unsupported)
}

pub(crate) fn icons_visible() -> DesktopResult<bool> {
    Err(DesktopError::Unsupported)
}

pub(crate) fn nudge_timing(_item_id: &str, _rounds: usize) -> DesktopResult<std::time::Duration> {
    Err(DesktopError::Unsupported)
}

pub(crate) fn desktop_folders() -> Vec<std::path::PathBuf> {
    Vec::new()
}
