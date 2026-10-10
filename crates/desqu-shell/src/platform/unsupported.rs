//! Stubs for platforms without a backend.

pub fn remove_run_value(_name: &str) -> Result<bool, String> {
    Ok(false)
}

/// No cross-process lock without a backend: every caller "wins".
pub struct LockHandle;

pub fn acquire_named_lock(_name: &str) -> Result<Option<LockHandle>, String> {
    Ok(Some(LockHandle))
}

use super::{ProcessInfo, UninstallEntry};

pub fn find_processes(_exe_name: &str) -> Vec<ProcessInfo> {
    Vec::new()
}

pub fn read_run_value(_name: &str) -> Option<String> {
    None
}

pub fn read_uninstall_entry(_key_name: &str) -> Option<UninstallEntry> {
    None
}

pub fn launch_uninstaller(_uninstall_string: &str) -> Result<(), String> {
    Err("not supported on this platform".into())
}
