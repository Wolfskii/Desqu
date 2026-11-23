//! "Start with Windows": a per-user Run entry. It has its own name, so it never overwrites the entry of
//! the Tauri build of Vials (Windows compares value names case-insensitively), and it is never made
//! for a debug build: such a build starts from the build folder and, in the Tauri app, loaded a dev
//! server that is not running at sign-in.

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE,
    REG_SZ, RRF_RT_REG_SZ,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub const VALUE_NAME: &str = "VialsGpui";

/// The command the Run entry holds: the app, in live mode, flagged as a sign-in launch.
pub fn command_for(exe: &str) -> String {
    format!("\"{exe}\" --live --autostart")
}

/// A build folder or a debug build is not somewhere to start from at every sign-in.
pub fn is_startable(exe: &str) -> bool {
    let lower = exe.to_lowercase().replace('/', "\\");
    !(lower.contains("\\target\\debug\\") || lower.contains("\\target\\release\\deps\\"))
}

pub fn is_enabled() -> bool {
    is_enabled_as(VALUE_NAME)
}

pub fn set(enabled: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    set_as(VALUE_NAME, exe.to_str().ok_or("the program path is not valid text")?, enabled)
}

pub fn is_enabled_as(name: &str) -> bool {
    let (key, value) = (HSTRING::from(RUN_KEY), HSTRING::from(name));
    let mut len = 0u32;
    // SAFETY: a size query only (no buffer given).
    let status = unsafe { RegGetValueW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()), PCWSTR(value.as_ptr()), RRF_RT_REG_SZ, None, None, Some(&mut len)) };
    status == ERROR_SUCCESS
}

pub fn set_as(name: &str, exe: &str, enabled: bool) -> Result<(), String> {
    let (key_path, value) = (HSTRING::from(RUN_KEY), HSTRING::from(name));
    if !enabled {
        let mut key = HKEY::default();
        // SAFETY: opens the Run key for writing and deletes only our own value.
        unsafe {
            let status = RegCreateKeyExW(HKEY_CURRENT_USER, PCWSTR(key_path.as_ptr()), None, PCWSTR::null(), REG_OPTION_NON_VOLATILE, KEY_WRITE, None, &mut key, None);
            if status != ERROR_SUCCESS {
                return Err(format!("cannot open the startup list: {status:?}"));
            }
            let status = RegDeleteValueW(key, PCWSTR(value.as_ptr()));
            let _ = RegCloseKey(key);
            return if status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND { Ok(()) } else { Err(format!("cannot remove the startup entry: {status:?}")) };
        }
    }
    if !is_startable(exe) {
        return Err("a build from the build folder cannot be set to start with Windows: install it first".into());
    }
    let data: Vec<u16> = command_for(exe).encode_utf16().chain(Some(0)).collect();
    let mut key = HKEY::default();
    // SAFETY: writes one string value into our own Run entry; the buffer outlives the call.
    unsafe {
        let status = RegCreateKeyExW(HKEY_CURRENT_USER, PCWSTR(key_path.as_ptr()), None, PCWSTR::null(), REG_OPTION_NON_VOLATILE, KEY_WRITE, None, &mut key, None);
        if status != ERROR_SUCCESS {
            return Err(format!("cannot open the startup list: {status:?}"));
        }
        let bytes = std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2);
        let status = RegSetValueExW(key, PCWSTR(value.as_ptr()), None, REG_SZ, Some(bytes));
        let _ = RegCloseKey(key);
        if status == ERROR_SUCCESS { Ok(()) } else { Err(format!("cannot add the startup entry: {status:?}")) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_and_build_folder_programs_are_not_startable() {
        assert!(is_startable(r"C:\Users\me\AppData\Local\Programs\Vials\vials-gpui.exe"));
        assert!(!is_startable(r"C:\code\Desqu\apps\vials-gpui\target\debug\vials-gpui.exe"));
        assert!(!is_startable(r"C:/code/x/target/debug/vials-gpui.exe"));
        assert!(!is_startable(r"C:\code\x\target\release\deps\vials_gpui.exe"));
    }

    #[test]
    fn the_command_quotes_the_path_and_flags_a_sign_in_launch() {
        assert_eq!(command_for(r"C:\Program Files\Vials\v.exe"), r#""C:\Program Files\Vials\v.exe" --live --autostart"#);
    }

    /// Writes and removes a throw-away value in the real Run key (not the app's own value).
    #[test]
    #[ignore = "touches the real startup list"]
    fn a_startup_entry_can_be_added_and_removed() {
        let name = format!("VialsGpuiTest{}", std::process::id());
        assert!(!is_enabled_as(&name));
        set_as(&name, r"C:\Users\me\AppData\Local\Programs\Vials\vials-gpui.exe", true).unwrap();
        let on = is_enabled_as(&name);
        set_as(&name, "", false).unwrap();
        assert!(on, "the entry exists after adding it");
        assert!(!is_enabled_as(&name), "and is gone after removing it");
        assert!(set_as(&name, r"C:\code\target\debug\vials-gpui.exe", true).is_err(), "a debug build is refused");
        assert!(!is_enabled_as(&name));
    }
}
