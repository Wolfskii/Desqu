use std::sync::Arc;
use std::thread::JoinHandle;

use ::windows::core::PCWSTR;
use ::windows::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, WAIT_OBJECT_0};
use ::windows::Win32::System::Registry::{
    RegCloseKey, RegNotifyChangeKeyValue, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_NOTIFY, KEY_READ, REG_NOTIFY_CHANGE_LAST_SET,
};
use ::windows::Win32::System::Threading::{
    CreateEventW, SetEvent, WaitForMultipleObjects, INFINITE,
};

use super::{DesktopInfo, Desktops};
use crate::core::model::guid_from_registry_bytes;

const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\VirtualDesktops";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn utf16(b: &[u8]) -> String {
    let u: Vec<u16> = b
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&u)
        .trim_end_matches('\0')
        .to_string()
}

/// Reads one registry value under the virtual desktops key; `None` when it does not exist.
fn read_value(subkey: &str, name: &str) -> Option<Vec<u8>> {
    let mut key = HKEY::default();
    let path = wide(&format!("{KEY}{subkey}"));
    // SAFETY: plain registry reads with buffers sized by the first call; the key is closed below.
    unsafe {
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(path.as_ptr()),
            None,
            KEY_READ,
            &mut key,
        ) != ERROR_SUCCESS
        {
            return None;
        }
        let n = wide(name);
        let mut len = 0u32;
        let mut out = None;
        if RegQueryValueExW(key, PCWSTR(n.as_ptr()), None, None, None, Some(&mut len))
            == ERROR_SUCCESS
        {
            let mut buf = vec![0u8; len as usize];
            if RegQueryValueExW(
                key,
                PCWSTR(n.as_ptr()),
                None,
                None,
                Some(buf.as_mut_ptr()),
                Some(&mut len),
            ) == ERROR_SUCCESS
            {
                buf.truncate(len as usize);
                out = Some(buf);
            }
        }
        let _ = RegCloseKey(key);
        out
    }
}

pub(super) fn desktops() -> Result<Desktops, String> {
    // Windows writes the key once a second desktop has been used; before that there is nothing
    // to list and no desktop to attach a workspace to.
    let Some(ids) = read_value("", "VirtualDesktopIDs") else {
        return Ok(Desktops::default());
    };
    let list: Vec<DesktopInfo> = ids
        .chunks_exact(16)
        .filter_map(guid_from_registry_bytes)
        .map(|id| {
            // The subkeys also exist for removed desktops: only the ids above are live.
            let name = read_value(&format!(r"\Desktops\{id}"), "Name")
                .map(|b| utf16(&b))
                .unwrap_or_default();
            DesktopInfo { id, name }
        })
        .collect();
    let current = read_value("", "CurrentVirtualDesktop")
        .and_then(|b| guid_from_registry_bytes(&b))
        .filter(|c| list.iter().any(|d| &d.id == c));
    Ok(Desktops { list, current })
}

// ---------------------------------------------------------------------------------- the watcher

struct Event(HANDLE);
// SAFETY: an event handle may be signalled and waited on from any thread.
unsafe impl Send for Event {}
unsafe impl Sync for Event {}

impl Drop for Event {
    fn drop(&mut self) {
        // SAFETY: created by CreateEventW and closed exactly once, here.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

struct KeyHolder(HKEY);
// SAFETY: a registry key handle can be used from any thread.
unsafe impl Send for KeyHolder {}
impl Drop for KeyHolder {
    fn drop(&mut self) {
        // SAFETY: opened by RegOpenKeyExW, closed once here.
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

pub(super) struct Watcher {
    stop: Arc<Event>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        // SAFETY: the handle stays valid until the last `Arc` clone (the thread's) is dropped.
        unsafe {
            let _ = SetEvent(self.stop.0);
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub(super) fn watch(on_change: Box<dyn Fn() + Send + 'static>) -> Result<Watcher, String> {
    let path = wide(KEY);
    let mut key = HKEY::default();
    // SAFETY: standard registry/event calls; every handle is closed on all paths.
    let (key_holder, change, stop) = unsafe {
        let status = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(path.as_ptr()),
            None,
            KEY_NOTIFY,
            &mut key,
        );
        if status != ERROR_SUCCESS {
            return Err("this Windows build has no virtual desktop registry key yet".into());
        }
        let key_holder = KeyHolder(key);
        let change = CreateEventW(None, false, false, None).map_err(|e| e.to_string())?;
        let stop = CreateEventW(None, true, false, None).map_err(|e| e.to_string())?;
        (key_holder, Event(change), Arc::new(Event(stop)))
    };
    let thread_stop = stop.clone();
    let thread = std::thread::Builder::new()
        .name("workspaces-desktop-watch".into())
        .spawn(move || {
            let key_holder = key_holder;
            let change = change;
            loop {
                // SAFETY: the key and both events outlive the loop; the notification is re-armed
                // before every wait (it fires once per registration).
                unsafe {
                    let armed = RegNotifyChangeKeyValue(
                        key_holder.0,
                        true,
                        REG_NOTIFY_CHANGE_LAST_SET,
                        Some(change.0),
                        true,
                    );
                    if armed != ERROR_SUCCESS {
                        tracing::warn!(
                            "could not watch the virtual desktop registry key: {armed:?}"
                        );
                        break;
                    }
                    if WaitForMultipleObjects(&[change.0, thread_stop.0], false, INFINITE)
                        != WAIT_OBJECT_0
                    {
                        break;
                    }
                }
                on_change();
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(Watcher {
        stop,
        thread: Some(thread),
    })
}
