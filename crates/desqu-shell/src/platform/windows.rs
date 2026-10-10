//! Win32 helpers (Run-key cleanup now; named mutex arrives with P3.8).

use ::windows::core::{HSTRING, PCWSTR};
use ::windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
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
