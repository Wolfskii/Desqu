//! No backend: every call reports that the layer is unsupported.

use std::path::Path;
use std::sync::Arc;

use crate::layer::{DesktopError, DesktopLayer, DesktopResult};

pub(crate) fn open(_owner: &str, _journal_path: &Path) -> DesktopResult<Arc<dyn DesktopLayer>> {
    Err(DesktopError::Unsupported)
}

pub(crate) fn recover_from(_journal_path: &Path) -> DesktopResult<usize> {
    Err(DesktopError::Unsupported)
}
