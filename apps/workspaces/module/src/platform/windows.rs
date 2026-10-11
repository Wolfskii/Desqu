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

use ::windows::core::GUID;
use ::windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use ::windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use ::windows::Win32::System::Threading::GetCurrentThreadId;
use ::windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use ::windows::Win32::UI::Shell::{IVirtualDesktopManager, VirtualDesktopManager};
use ::windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetMessageW, PostThreadMessageW, EVENT_SYSTEM_FOREGROUND, MSG,
    WINEVENT_OUTOFCONTEXT, WM_QUIT,
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
    let from_registry = read_value("", "CurrentVirtualDesktop")
        .and_then(|b| guid_from_registry_bytes(&b))
        .filter(|c| list.iter().any(|d| &d.id == c));
    // The registry value follows the desktop on screen within a fraction of a second (observed with
    // `vdwatch` on build 26300). If it is missing, the desktop of the window you are working in is
    // the next best answer (public IVirtualDesktopManager).
    let current =
        from_registry.or_else(|| foreground_desktop().filter(|c| list.iter().any(|d| &d.id == c)));
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

// ------------------------------------------------------------------- the foreground window

fn guid_text(g: &GUID) -> String {
    format!("{{{g:?}}}").to_ascii_uppercase()
}

/// Runs `f` with COM initialised on this thread.
fn with_com<T>(f: impl FnOnce() -> T) -> T {
    // SAFETY: balanced by CoUninitialize when this call initialised COM.
    let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    let out = f();
    if initialized {
        unsafe { CoUninitialize() };
    }
    out
}

/// The virtual desktop the foreground window is on. `None` for windows that belong to no single
/// desktop (the taskbar, pinned windows) or that the shell does not track.
fn desktop_of(mgr: &IVirtualDesktopManager, hwnd: HWND) -> Option<String> {
    if hwnd.0.is_null() {
        return None;
    }
    // SAFETY: COM call on a live interface with a window handle; failure just means "unknown".
    let id = unsafe { mgr.GetWindowDesktopId(hwnd) }.ok()?;
    (id != GUID::zeroed()).then(|| guid_text(&id))
}

fn foreground_desktop() -> Option<String> {
    with_com(|| {
        // SAFETY: standard COM activation and a foreground-window query.
        let mgr = unsafe {
            CoCreateInstance::<_, IVirtualDesktopManager>(&VirtualDesktopManager, None, CLSCTX_ALL)
        }
        .ok()?;
        desktop_of(&mgr, unsafe { GetForegroundWindow() })
    })
}

struct ForegroundState {
    mgr: IVirtualDesktopManager,
    last: Option<String>,
    on_change: Box<dyn Fn()>,
}

thread_local! {
    // The WinEvent callback runs on the thread that installed the hook.
    static FOREGROUND: std::cell::RefCell<Option<ForegroundState>> = const { std::cell::RefCell::new(None) };
}

unsafe extern "system" fn foreground_proc(
    _: HWINEVENTHOOK,
    _: u32,
    hwnd: HWND,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    FOREGROUND.with(|f| {
        if let Some(state) = f.borrow_mut().as_mut() {
            // Only a change of desktop matters; every Alt+Tab fires this event.
            if let Some(now) = desktop_of(&state.mgr, hwnd) {
                if state.last.as_ref() != Some(&now) {
                    state.last = Some(now);
                    (state.on_change)();
                }
            }
        }
    });
}

/// Tells when the focused window moves to another virtual desktop, which is what happens right
/// after a switch. Event-driven (a WinEvent hook); complements the registry watcher because the
/// registry value is not guaranteed to change when the desktop does.
pub(super) struct ForegroundWatcher {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl ForegroundWatcher {
    pub(super) fn start(on_change: Box<dyn Fn() + Send + 'static>) -> Result<Self, String> {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<u32, String>>();
        let thread = std::thread::Builder::new()
            .name("workspaces-foreground".into())
            .spawn(move || {
                with_com(|| {
                    // SAFETY: the hook is removed below on this same thread, before it ends.
                    unsafe {
                        let mgr = match CoCreateInstance::<_, IVirtualDesktopManager>(
                            &VirtualDesktopManager,
                            None,
                            CLSCTX_ALL,
                        ) {
                            Ok(m) => m,
                            Err(e) => {
                                let _ = ready_tx.send(Err(e.to_string()));
                                return;
                            }
                        };
                        let last = desktop_of(&mgr, GetForegroundWindow());
                        FOREGROUND.with(|f| {
                            *f.borrow_mut() = Some(ForegroundState {
                                mgr,
                                last,
                                on_change,
                            })
                        });
                        let hook = SetWinEventHook(
                            EVENT_SYSTEM_FOREGROUND,
                            EVENT_SYSTEM_FOREGROUND,
                            None,
                            Some(foreground_proc),
                            0,
                            0,
                            WINEVENT_OUTOFCONTEXT,
                        );
                        if hook.0.is_null() {
                            let _ =
                                ready_tx.send(Err("could not watch the foreground window".into()));
                            return;
                        }
                        let _ = ready_tx.send(Ok(GetCurrentThreadId()));
                        let mut msg = MSG::default();
                        while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
                        let _ = UnhookWinEvent(hook);
                        FOREGROUND.with(|f| *f.borrow_mut() = None);
                    }
                })
            })
            .map_err(|e| e.to_string())?;
        let thread_id = ready_rx.recv().map_err(|e| e.to_string())??;
        Ok(Self {
            thread_id,
            thread: Some(thread),
        })
    }
}

impl Drop for ForegroundWatcher {
    fn drop(&mut self) {
        // SAFETY: posts WM_QUIT to the watcher thread so its message loop ends.
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

// ----------------------------------------------------------- Ctrl+V on the desktop after Ctrl+X

use std::cell::RefCell;

use ::windows::Win32::Foundation::{HGLOBAL, LRESULT};
use ::windows::Win32::System::DataExchange::{
    CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW,
};
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use ::windows::Win32::System::Ole::{CF_HDROP, DROPEFFECT_MOVE};
use ::windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
use ::windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
use ::windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetClassNameW, KBDLLHOOKSTRUCT, SetWindowsHookExW, UnhookWindowsHookEx,
    WH_KEYBOARD_LL, WM_KEYDOWN,
};

pub(super) type PasteHandler = Box<dyn Fn(Vec<String>) + Send>;

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
        let effect_format = RegisterClipboardFormatW(::windows::core::w!("Preferred DropEffect"));
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

/// Calls the handler with the files that were cut when Ctrl+V is pressed on the desktop. Event-driven
/// (a low-level keyboard hook that only looks at the V key).
pub(super) struct PasteWatcher {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl PasteWatcher {
    pub(super) fn start(on_paste: PasteHandler) -> Result<Self, String> {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<u32, String>>();
        let thread = std::thread::Builder::new()
            .name("workspaces-paste-hook".into())
            .spawn(move || {
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
            })
            .map_err(|e| e.to_string())?;
        let thread_id = ready_rx.recv().map_err(|e| e.to_string())??;
        Ok(Self { thread_id, thread: Some(thread) })
    }
}

impl Drop for PasteWatcher {
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
