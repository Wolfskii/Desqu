//! Win32 helpers: Run-key cleanup and the per-module named lock.

use std::path::PathBuf;

use ::windows::core::{HSTRING, PCWSTR, PWSTR};
use ::windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, HANDLE,
};
use ::windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use ::windows::Win32::System::Threading::{
    CreateMutexW, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use ::windows::Win32::UI::Shell::ShellExecuteW;
use ::windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use ::windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
    KEY_SET_VALUE, REG_SZ,
};

use super::{ProcessInfo, UninstallEntry};

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

/// Opens a URL (or file) with the user's default handler.
pub fn open_url(url: &str) -> Result<(), String> {
    let url = HSTRING::from(url);
    // SAFETY: both strings outlive the call; a result above 32 means success.
    let rc = unsafe {
        ShellExecuteW(None, ::windows::core::w!("open"), PCWSTR(url.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL)
    };
    if rc.0 as usize > 32 {
        Ok(())
    } else {
        Err(format!("could not open {url}: code {}", rc.0 as usize))
    }
}

const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";

/// Reads a `REG_SZ` value from an HKCU key; `None` when the key, value or type is missing.
fn read_hkcu_string(subkey: &str, name: &str) -> Option<String> {
    let subkey = HSTRING::from(subkey);
    let mut key = HKEY::default();
    // SAFETY: `key` is a valid out-pointer and `subkey` outlives the call.
    let rc = unsafe {
        RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(subkey.as_ptr()), None, KEY_READ, &mut key)
    };
    if rc != ERROR_SUCCESS {
        return None;
    }
    let value = HSTRING::from(name);
    let mut kind = REG_SZ;
    let mut len: u32 = 0;
    // First call: ask for the size in bytes.
    // SAFETY: `key` is open; `len` is a valid out-pointer.
    let rc = unsafe {
        RegQueryValueExW(key, PCWSTR(value.as_ptr()), None, Some(&mut kind), None, Some(&mut len))
    };
    let mut result = None;
    if rc == ERROR_SUCCESS && kind == REG_SZ && len >= 2 {
        let mut buf = vec![0u16; len as usize / 2 + 1];
        let mut len2 = (buf.len() * 2) as u32;
        // SAFETY: `buf` has `len2` bytes of room.
        let rc = unsafe {
            RegQueryValueExW(
                key,
                PCWSTR(value.as_ptr()),
                None,
                Some(&mut kind),
                Some(buf.as_mut_ptr().cast()),
                Some(&mut len2),
            )
        };
        if rc == ERROR_SUCCESS {
            let chars = (len2 as usize / 2).min(buf.len());
            let text = String::from_utf16_lossy(&buf[..chars]);
            result = Some(text.trim_end_matches('\0').to_string());
        }
    }
    // SAFETY: `key` was opened above.
    unsafe {
        let _ = RegCloseKey(key);
    }
    result
}

/// Data of an HKCU Run value, e.g. the exe path a "start with system" entry launches.
pub fn read_run_value(name: &str) -> Option<String> {
    read_hkcu_string(RUN_KEY, name)
}

pub fn read_uninstall_entry(key_name: &str) -> Option<UninstallEntry> {
    let subkey = format!(r"{UNINSTALL_KEY}\{key_name}");
    let uninstall_string = read_hkcu_string(&subkey, "UninstallString");
    let install_location = read_hkcu_string(&subkey, "InstallLocation");
    let display_version = read_hkcu_string(&subkey, "DisplayVersion");
    if uninstall_string.is_none() && install_location.is_none() && display_version.is_none() {
        return None;
    }
    Some(UninstallEntry { display_version, install_location, uninstall_string })
}

/// Starts an uninstaller from an `UninstallString` value (`"C:\...\uninstall.exe"`, optionally
/// followed by arguments is not supported: NSIS writes just the quoted path).
pub fn launch_uninstaller(uninstall_string: &str) -> Result<(), String> {
    let path = uninstall_string.trim().trim_matches('"');
    if path.is_empty() {
        return Err("empty uninstall command".into());
    }
    std::process::Command::new(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not start {path}: {e}"))
}

fn exe_of(pid: u32) -> Option<PathBuf> {
    // SAFETY: plain query-only handle, closed below.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buf = vec![0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: `buf` has `len` characters of room.
    let ok = unsafe {
        QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len)
    };
    // SAFETY: `handle` was opened above.
    unsafe {
        let _ = CloseHandle(handle);
    }
    ok.ok()?;
    Some(PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
}

/// All running processes whose image name equals `exe_name` (case-insensitive).
pub fn find_processes(exe_name: &str) -> Vec<ProcessInfo> {
    // SAFETY: snapshot handle is closed below.
    let Ok(snapshot) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    // SAFETY: `entry.dwSize` is set; `snapshot` is a valid snapshot handle.
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok();
    while more {
        let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
        let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
        if name.eq_ignore_ascii_case(exe_name) {
            found.push(ProcessInfo { pid: entry.th32ProcessID, exe: exe_of(entry.th32ProcessID) });
        }
        // SAFETY: same preconditions as the first call.
        more = unsafe { Process32NextW(snapshot, &mut entry) }.is_ok();
    }
    // SAFETY: `snapshot` was created above.
    unsafe {
        let _ = CloseHandle(snapshot);
    }
    found
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

    #[test]
    fn finds_this_test_process_by_name() {
        let me = std::env::current_exe().unwrap();
        let name = me.file_name().unwrap().to_string_lossy().to_string();
        let found = find_processes(&name);
        assert!(found.iter().any(|p| p.pid == std::process::id()), "{found:?}");
        let mine = found.iter().find(|p| p.pid == std::process::id()).unwrap();
        assert_eq!(mine.exe.as_ref().map(|p| p.file_name().unwrap().to_owned()), Some(me.file_name().unwrap().to_owned()));
    }

    #[test]
    fn unknown_process_name_finds_nothing() {
        assert!(find_processes("definitely-not-a-process-4711.exe").is_empty());
    }

    #[test]
    fn reads_back_a_run_value_and_reports_missing_ones() {
        let name = format!("DesquShellRead{}", std::process::id());
        let run = format!(r"HKCU\{RUN_KEY}");
        assert_eq!(read_run_value(&name), None);
        assert!(reg(&["add", &run, "/v", &name, "/d", r"C:\x\app.exe --autostart", "/f"]));
        assert_eq!(read_run_value(&name).as_deref(), Some(r"C:\x\app.exe --autostart"));
        assert_eq!(remove_run_value(&name), Ok(true));
        assert_eq!(read_run_value(&name), None);
    }

    #[test]
    fn missing_uninstall_entry_is_none() {
        assert_eq!(read_uninstall_entry("Desqu-no-such-product-4711"), None);
    }
}
