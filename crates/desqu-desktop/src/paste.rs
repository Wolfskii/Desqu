//! Ctrl+V on the desktop after Ctrl+X: Explorer does nothing when files that were cut are pasted into
//! the folder they already are in, so a module that groups desktop items (Vials, Workspaces) has to
//! notice the paste itself. Event-driven: a low-level keyboard hook that only looks at the V key.

use std::cell::RefCell;
use std::thread::JoinHandle;

use windows::Win32::Foundation::{HGLOBAL, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::{
    CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::{CF_HDROP, DROPEFFECT_MOVE};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetClassNameW, GetForegroundWindow, GetMessageW, PostThreadMessageW, SetWindowsHookExW,
    UnhookWindowsHookEx, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_QUIT,
};

use crate::layer::{DesktopError, DesktopResult};

type PasteHandler = Box<dyn Fn(Vec<String>) + Send>;

thread_local! {
    // The low-level hook procedure runs on the thread that installed it.
    static PASTE: RefCell<Option<PasteHandler>> = const { RefCell::new(None) };
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
                    PASTE.with(|p| {
                        if let Some(handler) = p.borrow().as_ref() {
                            handler(paths);
                        }
                    });
                }
            }
        }
    }
    // SAFETY: always pass the key on.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Keeps the Ctrl+V hook alive; dropping it removes the hook.
pub struct CutPasteWatcher {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

/// Calls `on_paste` (inside the hook, so only hand the paths over) with the files that were cut when
/// Ctrl+V is pressed on the desktop.
pub fn watch_cut_paste(on_paste: Box<dyn Fn(Vec<String>) + Send + 'static>) -> DesktopResult<CutPasteWatcher> {
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<u32, String>>();
    let thread = std::thread::Builder::new().name("desqu-paste-hook".into()).spawn(move || {
        PASTE.with(|p| *p.borrow_mut() = Some(on_paste));
        // SAFETY: the hook is removed below on this same thread, before it ends.
        unsafe {
            let module = GetModuleHandleW(None).unwrap_or_default();
            let hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), Some(module.into()), 0) {
                Ok(hook) => hook,
                Err(e) => {
                    let _ = ready_tx.send(Err(e.to_string()));
                    return;
                }
            };
            let _ = ready_tx.send(Ok(GetCurrentThreadId()));
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
            let _ = UnhookWindowsHookEx(hook);
        }
        PASTE.with(|p| *p.borrow_mut() = None);
    })?;
    let thread_id = ready_rx
        .recv()
        .map_err(|e| DesktopError::Native { op: "paste hook", message: e.to_string() })?
        .map_err(|message| DesktopError::Native { op: "paste hook", message })?;
    Ok(CutPasteWatcher { thread_id, thread: Some(thread) })
}

impl Drop for CutPasteWatcher {
    fn drop(&mut self) {
        // SAFETY: posts WM_QUIT to the hook thread so its message loop ends.
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
