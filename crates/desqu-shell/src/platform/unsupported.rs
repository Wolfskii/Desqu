//! Stubs for platforms without a backend.

pub fn remove_run_value(_name: &str) -> Result<bool, String> {
    Ok(false)
}

/// No cross-process lock without a backend: every caller "wins".
pub struct LockHandle;

pub fn acquire_named_lock(_name: &str) -> Result<Option<LockHandle>, String> {
    Ok(Some(LockHandle))
}
