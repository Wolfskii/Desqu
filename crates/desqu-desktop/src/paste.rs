//! Ctrl+V on the desktop after Ctrl+X: Explorer does nothing when files that were cut are pasted into
//! the folder they already are in (it asks "source and destination have the same name"), so a module
//! that groups desktop items (Vials, Workspaces) has to notice the paste itself.
//!
//! One keyboard hook serves the whole process and calls every listener. A hook that swallows a key
//! must not be followed by a second hook for the same key (Windows would not pass it on), so the hook
//! is shared and makes the swallow decision once, after all listeners were told.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

use windows::Win32::Foundation::{HGLOBAL, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::{
    CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::{CF_HDROP, DROPEFFECT_MOVE};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetClassNameW, GetForegroundWindow, GetMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
    KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN,
};

use crate::layer::{DesktopError, DesktopResult};

type PasteHandler = Box<dyn Fn(Vec<String>) + Send>;

/// Everyone who asked to hear about cut pastes, by watcher id.
static LISTENERS: Mutex<Vec<(u64, PasteHandler)>> = Mutex::new(Vec::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
/// The desktop folders, lower case and without a trailing backslash (filled once, by the hook thread).
static FOLDERS: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// The hook thread's start result, kept for every later call.
static HOOK: OnceLock<Result<(), String>> = OnceLock::new();

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn path_key(path: &str) -> String {
    path.trim().replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

/// True when every path sits directly in one of `folders`: pasting such files into one of those
/// folders is what Explorer refuses with "source and destination have the same name".
pub fn all_in_folders(paths: &[String], folders: &[String]) -> bool {
    let folders: Vec<String> = folders.iter().map(|f| path_key(f)).collect();
    !paths.is_empty()
        && paths.iter().all(|p| {
            let key = path_key(p);
            key.rsplit_once('\\').is_some_and(|(parent, _)| folders.iter().any(|f| f == parent))
        })
}

fn foreground_is_desktop() -> bool {
    // SAFETY: plain window queries.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return false;
        }
        let mut buf = [0u16; 32];
        let n = GetClassNameW(hwnd, &mut buf);
        let class = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
        class == "Progman" || class == "WorkerW"
    }
}

/// The files on the clipboard when they were CUT (not copied), as paths. `None` for anything else.
fn cut_files_on_clipboard() -> Option<Vec<String>> {
    // SAFETY: the clipboard is opened and closed in this function; locked memory is only read while
    // locked, and the HDROP handle is only used while the clipboard is open.
    unsafe {
        if IsClipboardFormatAvailable(CF_HDROP.0 as u32).is_err() {
            return None;
        }
        let effect_format = RegisterClipboardFormatW(windows::core::w!("Preferred DropEffect"));
        OpenClipboard(None).ok()?;
        let result = (|| {
            let effect = GetClipboardData(effect_format).ok()?;
            let ptr = GlobalLock(HGLOBAL(effect.0)) as *const u32;
            if ptr.is_null() {
                return None;
            }
            let moved = *ptr & DROPEFFECT_MOVE.0 != 0;
            let _ = GlobalUnlock(HGLOBAL(effect.0));
            if !moved {
                return None;
            }
            let drop = GetClipboardData(CF_HDROP.0 as u32).ok()?;
            let hdrop = HDROP(drop.0);
            let count = DragQueryFileW(hdrop, u32::MAX, None);
            let mut paths = Vec::new();
            for i in 0..count {
                let len = DragQueryFileW(hdrop, i, None) as usize;
                let mut buf = vec![0u16; len + 1];
                let got = DragQueryFileW(hdrop, i, Some(&mut buf)) as usize;
                paths.push(String::from_utf16_lossy(&buf[..got]));
            }
            Some(paths)
        })();
        let _ = CloseClipboard();
        result
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && wparam.0 as u32 == WM_KEYDOWN {
        // SAFETY: for WH_KEYBOARD_LL `lparam` points to a KBDLLHOOKSTRUCT valid for this call.
        let key = unsafe { (*(lparam.0 as *const KBDLLHOOKSTRUCT)).vkCode };
        if key == u32::from(b'V') {
            // SAFETY: plain key state queries.
            let down = |vk: u16| unsafe { GetAsyncKeyState(i32::from(vk)) } < 0;
            if down(VK_CONTROL.0) && !down(VK_SHIFT.0) && !down(VK_MENU.0) && foreground_is_desktop() {
                // Read the clipboard now, before Explorer handles the same key press (a finished
                // cut-and-paste clears it).
                if let Some(paths) = cut_files_on_clipboard() {
                    // When every file already sits on the desktop, Explorer's paste would only raise its
                    // "same name" dialog: the key is taken here instead, after the listeners were told.
                    let folders = lock(&FOLDERS).clone();
                    let swallow = all_in_folders(&paths, &folders);
                    tracing::info!(items = paths.len(), swallow, "Ctrl+V on the desktop after a cut");
                    for (_, handler) in lock(&LISTENERS).iter() {
                        handler(paths.clone());
                    }
                    if swallow {
                        return LRESULT(1);
                    }
                }
            }
        }
    }
    // SAFETY: always pass the key on.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Installs the process-wide hook on a thread of its own; it lives as long as the process.
fn start_hook_thread() -> Result<(), String> {
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
    std::thread::Builder::new()
        .name("desqu-paste-hook".into())
        .spawn(move || {
            let folders: Vec<String> = crate::platform::desktop_folders()
                .iter()
                .map(|p| path_key(&p.to_string_lossy()))
                .collect();
            tracing::info!(?folders, "watching Ctrl+V on the desktop");
            *lock(&FOLDERS) = folders;
            // SAFETY: the hook lives on this thread, whose message loop below keeps it working.
            unsafe {
                let module = GetModuleHandleW(None).unwrap_or_default();
                let hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), Some(module.into()), 0) {
                    Ok(hook) => hook,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.to_string()));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(()));
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
                let _ = UnhookWindowsHookEx(hook);
            }
        })
        .map_err(|e| e.to_string())?;
    ready_rx.recv().map_err(|e| e.to_string())?
}

/// Keeps one listener of the Ctrl+V hook; dropping it stops the listener.
pub struct CutPasteWatcher {
    id: u64,
}

/// Calls `on_paste` with the files that were cut when Ctrl+V is pressed on the desktop. The hook is
/// shared by every caller in the process, and each listener gets the paths.
pub fn watch_cut_paste(on_paste: Box<dyn Fn(Vec<String>) + Send + 'static>) -> DesktopResult<CutPasteWatcher> {
    HOOK.get_or_init(start_hook_thread)
        .clone()
        .map_err(|message| DesktopError::Native { op: "paste hook", message })?;
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    lock(&LISTENERS).push((id, on_paste));
    Ok(CutPasteWatcher { id })
}

impl Drop for CutPasteWatcher {
    fn drop(&mut self) {
        lock(&LISTENERS).retain(|(id, _)| *id != self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::all_in_folders;

    fn v(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn only_files_already_in_a_desktop_folder_count() {
        let folders = v(&[r"C:\Users\me\Desktop", r"C:\Users\Public\Desktop\"]);
        assert!(all_in_folders(&v(&[r"c:/users/ME/desktop/a.txt"]), &folders));
        assert!(all_in_folders(&v(&[r"C:\Users\Public\Desktop\b.lnk", r"C:\Users\me\Desktop\c.url"]), &folders));
        assert!(!all_in_folders(&v(&[r"C:\Users\me\Desktop\Games\x.txt"]), &folders), "a subfolder is not the desktop");
        assert!(!all_in_folders(&v(&[r"C:\Users\me\Desktop\a.txt", r"D:\docs\b.txt"]), &folders), "mixed: Explorer decides");
        assert!(!all_in_folders(&[], &folders));
    }
}
