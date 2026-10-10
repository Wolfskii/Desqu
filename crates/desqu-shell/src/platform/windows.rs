//! Win32 helpers: Run-key cleanup and the per-module named lock.

use ::windows::core::{HSTRING, PCWSTR};
use ::windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, HANDLE,
};
use ::windows::Win32::System::Threading::CreateMutexW;
use ::windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const STARTUP_APPROVED_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

/// Deletes one value from an HKCU key. `Ok(false)` when the key or value does not exist.
fn delete_hkcu_value(subkey: &str, name: &str) -> Result<bool, String> {
    let subkey = HSTRING::from(subkey);
    let mut key = HKEY::default();
    // SAFETY: `key` is a valid out-pointer and `subkey` outlives the call.
    let rc = unsafe {
        RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(subkey.as_ptr()), None, KEY_SET_VALUE, &mut key)
    };
    if rc == ERROR_FILE_NOT_FOUND {
        return Ok(false);
    }
    if rc != ERROR_SUCCESS {
        return Err(format!("opening {subkey} failed: {rc:?}"));
    }
    // SAFETY: `key` was opened above and is closed right after.
    let rc = unsafe { RegDeleteValueW(key, &HSTRING::from(name)) };
    unsafe {
        let _ = RegCloseKey(key);
    }
    if rc == ERROR_SUCCESS {
        Ok(true)
    } else if rc == ERROR_FILE_NOT_FOUND {
        Ok(false)
    } else {
        Err(format!("deleting {name} failed: {rc:?}"))
    }
}

/// Removes an autostart entry: the HKCU Run value and the matching StartupApproved value Windows
/// keeps for it. Returns whether the Run value existed.
pub fn remove_run_value(name: &str) -> Result<bool, String> {
    let existed = delete_hkcu_value(RUN_KEY, name)?;
    delete_hkcu_value(STARTUP_APPROVED_KEY, name)?;
    Ok(existed)
}

/// A named kernel mutex held for as long as this value lives. Its *existence* is the lock (it is
/// not "owned" by a thread), so it can be dropped from any thread.
pub struct LockHandle(HANDLE);

// SAFETY: a kernel handle value is valid process-wide; closing it from another thread is fine.
unsafe impl Send for LockHandle {}
unsafe impl Sync for LockHandle {}

impl Drop for LockHandle {
    fn drop(&mut self) {
        // SAFETY: the handle was created by `acquire_named_lock` and is closed exactly once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// `Ok(None)` when another holder (this or another process) already has the lock.
pub fn acquire_named_lock(name: &str) -> Result<Option<LockHandle>, String> {
    let name = HSTRING::from(name);
    // SAFETY: `name` outlives the call.
    let handle = unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) }
        .map_err(|e| format!("creating lock {name} failed: {e}"))?;
    // CreateMutexW succeeds and returns the existing mutex when the name is already taken.
    // SAFETY: read straight after the call, on the same thread.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return Ok(None);
    }
    Ok(Some(LockHandle(handle)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn reg(args: &[&str]) -> bool {
        Command::new("reg")
            .args(args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn removes_run_value_and_reports_missing_ones() {
        let name = format!("DesquShellTest{}", std::process::id());
        let run = format!(r"HKCU\{RUN_KEY}");
        assert!(reg(&["add", &run, "/v", &name, "/d", "x.exe --autostart", "/f"]));
        assert!(reg(&["query", &run, "/v", &name]));

        assert_eq!(remove_run_value(&name), Ok(true));
        assert!(!reg(&["query", &run, "/v", &name]));
        // Second call: already gone is not an error.
        assert_eq!(remove_run_value(&name), Ok(false));
    }
}
