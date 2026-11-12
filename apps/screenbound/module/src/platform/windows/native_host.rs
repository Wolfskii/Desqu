//! Registers the native messaging host for each browser: two manifest files under
//! `%LOCALAPPDATA%\Desqu\browser-host` and one per-user registry key per browser that reads them.
//! Per user only (HKCU): no admin rights, and nothing outside the user's profile is touched.

use std::ffi::c_void;
use std::io;
use std::path::{Path, PathBuf};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegGetValueW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
};

use crate::bridge::manifest::HOST_NAME;

/// Chromium-based browsers read `HKCU\<base>\<host name>`. Brave and Vivaldi use the Chromium rules.
pub const CHROMIUM_BASES: &[&str] = &[
    r"Software\Google\Chrome\NativeMessagingHosts",
    r"Software\Chromium\NativeMessagingHosts",
    r"Software\Microsoft\Edge\NativeMessagingHosts",
    r"Software\BraveSoftware\Brave-Browser\NativeMessagingHosts",
    r"Software\Vivaldi\NativeMessagingHosts",
    r"Software\Opera Software\NativeMessagingHosts",
];

/// Firefox-based browsers read `HKCU\<base>\<host name>`.
pub const FIREFOX_BASES: &[&str] = &[
    r"Software\Mozilla\NativeMessagingHosts",
    r"Software\LibreWolf\NativeMessagingHosts",
    r"Software\Waterfox\NativeMessagingHosts",
];

const CHROMIUM_FILE: &str = "chromium.json";
const FIREFOX_FILE: &str = "firefox.json";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn check(code: WIN32_ERROR, what: &str) -> io::Result<()> {
    if code == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::other(format!("{what} failed (error {})", code.0)))
    }
}

fn key_path(base: &str) -> String {
    format!(r"{base}\{HOST_NAME}")
}

/// `%LOCALAPPDATA%\Desqu\browser-host`. Per user, so it survives reinstalls of either app.
pub fn manifest_dir() -> io::Result<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| io::Error::other("LOCALAPPDATA is not set"))?;
    Ok(PathBuf::from(local).join("Desqu").join("browser-host"))
}

/// Writes only when the text differs, so repeated starts do not touch the disk.
fn write_if_changed(path: &Path, text: &str) -> io::Result<()> {
    if std::fs::read_to_string(path).is_ok_and(|old| old == text) {
        return Ok(());
    }
    std::fs::write(path, text)
}

fn set_default_value(key: &str, data: &str) -> io::Result<()> {
    let name = wide(key);
    let mut handle = HKEY(std::ptr::null_mut());
    unsafe {
        check(
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(name.as_ptr()),
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                None,
                &mut handle,
                None,
            ),
            "creating a registry key",
        )?;
        let value = wide(data);
        let bytes = std::slice::from_raw_parts(value.as_ptr() as *const u8, value.len() * 2);
        let result = RegSetValueExW(handle, PCWSTR::null(), None, REG_SZ, Some(bytes));
        let _ = RegCloseKey(handle);
        check(result, "setting a registry value")
    }
}

/// The default value of a key, or `None` when the key is missing.
fn default_value(key: &str) -> Option<String> {
    let name = wide(key);
    let mut buf = vec![0u16; 2048];
    let mut bytes = (buf.len() * 2) as u32;
    let code = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(name.as_ptr()),
            PCWSTR::null(),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut c_void),
            Some(&mut bytes),
        )
    };
    if code != ERROR_SUCCESS {
        return None;
    }
    let chars = (bytes as usize / 2).min(buf.len());
    let text = String::from_utf16_lossy(&buf[..chars]);
    Some(text.trim_end_matches('\0').to_string())
}

fn delete_tree(key: &str) -> io::Result<()> {
    let name = wide(key);
    let code = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(name.as_ptr())) };
    if code == ERROR_SUCCESS || code == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        check(code, "deleting a registry key")
    }
}

/// Writes both manifests and points every browser's key at the right one.
/// Returns the folder the manifests are in.
pub fn install(chromium_manifest: &str, firefox_manifest: &str) -> io::Result<PathBuf> {
    let dir = manifest_dir()?;
    std::fs::create_dir_all(&dir)?;
    let chromium_path = dir.join(CHROMIUM_FILE);
    let firefox_path = dir.join(FIREFOX_FILE);
    write_if_changed(&chromium_path, chromium_manifest)?;
    write_if_changed(&firefox_path, firefox_manifest)?;
    let chromium_data = chromium_path.to_string_lossy().into_owned();
    let firefox_data = firefox_path.to_string_lossy().into_owned();
    for base in CHROMIUM_BASES {
        set_default_value(&key_path(base), &chromium_data)?;
    }
    for base in FIREFOX_BASES {
        set_default_value(&key_path(base), &firefox_data)?;
    }
    Ok(dir)
}

/// Removes every key and the manifest folder. Missing pieces are not an error.
pub fn uninstall() -> io::Result<()> {
    for base in CHROMIUM_BASES.iter().chain(FIREFOX_BASES) {
        delete_tree(&key_path(base))?;
    }
    if let Ok(dir) = manifest_dir() {
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// True when Chrome's key points at our manifest, and that manifest names this host exe.
/// A moved or reinstalled app shows up as "not installed" until registration runs again.
pub fn is_installed(host_exe: &Path) -> bool {
    let Some(base) = CHROMIUM_BASES.first() else {
        return false;
    };
    let Ok(dir) = manifest_dir() else {
        return false;
    };
    let manifest = dir.join(CHROMIUM_FILE);
    let points_here = default_value(&key_path(base))
        .is_some_and(|value| Path::new(&value) == manifest);
    if !points_here {
        return false;
    }
    let Ok(text) = std::fs::read_to_string(&manifest) else {
        return false;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    json.get("path").and_then(|p| p.as_str()) == Some(host_exe.to_string_lossy().as_ref())
}
