//! Atomic JSON persistence shared by settings files.

use std::path::Path;

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum JsonError {
    #[error("I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("not valid JSON: {0}")]
    Parse(#[from] serde_json::Error),
}

/// Writes via temp file + rename so a crash mid-write cannot leave a truncated file.
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), JsonError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
