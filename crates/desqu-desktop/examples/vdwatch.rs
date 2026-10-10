//! Diagnostic: prints a line whenever the registry's current virtual desktop or the desktop of the
//! foreground window changes, to compare what Windows publishes with what is on screen. Read-only.
//!
//! Run: `cargo run -p desqu-desktop --example vdwatch -- [seconds]`
#![cfg(windows)]

use std::time::{Duration, Instant};

use windows::core::{GUID, PCWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
};
use windows::Win32::UI::Shell::{IVirtualDesktopManager, VirtualDesktopManager};
use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow};

const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\VirtualDesktops";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn registry_current() -> String {
    let mut key = HKEY::default();
    let path = wide(KEY);
    let name = wide("CurrentVirtualDesktop");
    // SAFETY: plain registry read into a local buffer.
    unsafe {
        if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(path.as_ptr()), None, KEY_READ, &mut key) != ERROR_SUCCESS {
            return "(no key)".into();
        }
        let mut buf = [0u8; 16];
        let mut len = 16u32;
        let ok = RegQueryValueExW(key, PCWSTR(name.as_ptr()), None, None, Some(buf.as_mut_ptr()), Some(&mut len))
            == ERROR_SUCCESS;
        let _ = RegCloseKey(key);
        if !ok {
            return "(none)".into();
        }
        let g = GUID::from_values(
            u32::from_le_bytes(buf[0..4].try_into().unwrap()),
            u16::from_le_bytes(buf[4..6].try_into().unwrap()),
            u16::from_le_bytes(buf[6..8].try_into().unwrap()),
            buf[8..16].try_into().unwrap(),
        );
        format!("{g:?}")
    }
}

fn main() {
    let seconds: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(600);
    // SAFETY: COM on this thread for the whole run.
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
        let mgr: IVirtualDesktopManager = CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_ALL).unwrap();
        let end = Instant::now() + Duration::from_secs(seconds);
        let mut last = String::new();
        let start = Instant::now();
        while Instant::now() < end {
            let hwnd = GetForegroundWindow();
            let mut class = [0u16; 64];
            let n = GetClassNameW(hwnd, &mut class);
            let fg = match mgr.GetWindowDesktopId(hwnd) {
                Ok(g) => format!("{g:?}"),
                Err(e) => format!("err {:#X}", e.code().0),
            };
            let line = format!(
                "reg={} fg={} class={}",
                registry_current(),
                fg,
                String::from_utf16_lossy(&class[..n.max(0) as usize])
            );
            if line != last {
                println!("{:>7.2}s {line}", start.elapsed().as_secs_f32());
                last = line;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}
