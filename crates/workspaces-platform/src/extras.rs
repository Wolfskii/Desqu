//! Windows-only watchers and shell entries Workspaces needs besides the virtual desktop list:
//! Task View (Win+Tab) opening and closing, files appearing on the clipboard, and the desktop's
//! "Paste on this workspace" right-click entry.

use std::cell::RefCell;
use std::thread::JoinHandle;

use ::windows::core::{w, HSTRING, PCWSTR};
use ::windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use ::windows::Win32::System::DataExchange::{AddClipboardFormatListener, IsClipboardFormatAvailable, RemoveClipboardFormatListener};
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use ::windows::Win32::System::Threading::GetCurrentThreadId;
use ::windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use ::windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, RegisterHotKey, SendInput, UnregisterHotKey, HOT_KEY_MODIFIERS, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, MOD_CONTROL, MOD_NOREPEAT, MOD_WIN, VIRTUAL_KEY, VK_CONTROL, VK_LEFT, VK_LWIN, VK_MENU, VK_RIGHT,
};
use ::windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClassNameW, GetMessageW, PostThreadMessageW, RegisterClassW,
    TranslateMessage, UnregisterClassW, EVENT_SYSTEM_FOREGROUND, HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WINEVENT_OUTOFCONTEXT,
    WM_CLIPBOARDUPDATE, WM_HOTKEY, WM_QUIT, WNDCLASSW,
};

// ------------------------------------------------------------------------------------ Task View

/// Windows classes of Task View (Win+Tab): Windows 11, and Windows 10.
fn is_task_view_class(class: &str) -> bool {
    matches!(class, "XamlExplorerHostIslandWindow" | "MultitaskingViewFrame")
}

fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 96];
    // SAFETY: valid window handle and a local buffer.
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

struct TaskViewState {
    open: bool,
    on_change: Box<dyn Fn(bool)>,
}

thread_local! {
    static TASK_VIEW: RefCell<Option<TaskViewState>> = const { RefCell::new(None) };
}

unsafe extern "system" fn task_view_proc(_: HWINEVENTHOOK, _: u32, hwnd: HWND, _: i32, _: i32, _: u32, _: u32) {
    // The Alt+Tab switcher is the same kind of window, but with Alt held: only Win+Tab, the taskbar's
    // Task View button and the touchpad gesture count.
    // SAFETY: plain key state query.
    let alt = unsafe { GetAsyncKeyState(i32::from(VK_MENU.0)) } as u16 & 0x8000 != 0;
    let now_open = !hwnd.0.is_null() && is_task_view_class(&class_of(hwnd)) && !alt;
    TASK_VIEW.with(|t| {
        if let Some(state) = t.borrow_mut().as_mut() {
            if state.open != now_open {
                state.open = now_open;
                (state.on_change)(now_open);
            }
        }
    });
}

/// Tells when Task View opens (`true`) and closes (`false`). Event-driven (a WinEvent hook).
pub struct TaskViewWatcher {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl TaskViewWatcher {
    pub fn start(on_change: Box<dyn Fn(bool) + Send + 'static>) -> Result<Self, String> {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<u32, String>>();
        let thread = std::thread::Builder::new()
            .name("workspaces-task-view".into())
            .spawn(move || {
                TASK_VIEW.with(|t| *t.borrow_mut() = Some(TaskViewState { open: false, on_change }));
                // SAFETY: the hook is removed below on this same thread, before it ends.
                unsafe {
                    let hook = SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, None, Some(task_view_proc), 0, 0, WINEVENT_OUTOFCONTEXT);
                    if hook.0.is_null() {
                        let _ = ready_tx.send(Err("could not watch the foreground window".into()));
                        return;
                    }
                    let _ = ready_tx.send(Ok(GetCurrentThreadId()));
                    let mut msg = MSG::default();
                    while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
                    let _ = UnhookWinEvent(hook);
                }
                TASK_VIEW.with(|t| *t.borrow_mut() = None);
            })
            .map_err(|e| e.to_string())?;
        let thread_id = ready_rx.recv().map_err(|e| e.to_string())??;
        Ok(Self { thread_id, thread: Some(thread) })
    }
}

impl Drop for TaskViewWatcher {
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

// ------------------------------------------------------------------------------------ clipboard

/// CF_HDROP: a list of files (what Copy and Cut on files put on the clipboard).
const CF_HDROP: u32 = 15;

/// Whether the clipboard holds files right now.
pub fn clipboard_has_files() -> bool {
    // SAFETY: plain query.
    unsafe { IsClipboardFormatAvailable(CF_HDROP).is_ok() }
}

/// What to call with whether the clipboard holds files.
type FilesCallback = Box<dyn Fn(bool)>;

thread_local! {
    static CLIPBOARD: RefCell<Option<FilesCallback>> = const { RefCell::new(None) };
}

unsafe extern "system" fn clipboard_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_CLIPBOARDUPDATE {
        let files = clipboard_has_files();
        CLIPBOARD.with(|c| {
            if let Some(f) = c.borrow().as_ref() {
                f(files);
            }
        });
        return LRESULT(0);
    }
    // SAFETY: default handling for everything else.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Tells whether the clipboard holds files each time the clipboard changes.
pub struct ClipboardWatcher {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl ClipboardWatcher {
    pub fn start(on_change: Box<dyn Fn(bool) + Send + 'static>) -> Result<Self, String> {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<u32, String>>();
        let thread = std::thread::Builder::new()
            .name("workspaces-clipboard".into())
            .spawn(move || {
                CLIPBOARD.with(|c| *c.borrow_mut() = Some(on_change));
                // SAFETY: a message-only window of this thread, destroyed below before the thread ends.
                unsafe {
                    let instance = GetModuleHandleW(None).unwrap_or_default();
                    let class = w!("DesquWorkspacesClipboard");
                    let wc = WNDCLASSW { lpfnWndProc: Some(clipboard_proc), hInstance: instance.into(), lpszClassName: class, ..Default::default() };
                    RegisterClassW(&wc);
                    let hwnd = match CreateWindowExW(WINDOW_EX_STYLE(0), class, PCWSTR::null(), WINDOW_STYLE(0), 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(instance.into()), None) {
                        Ok(h) => h,
                        Err(e) => {
                            let _ = ready_tx.send(Err(e.to_string()));
                            return;
                        }
                    };
                    if let Err(e) = AddClipboardFormatListener(hwnd) {
                        let _ = DestroyWindow(hwnd);
                        let _ = ready_tx.send(Err(e.to_string()));
                        return;
                    }
                    let _ = ready_tx.send(Ok(GetCurrentThreadId()));
                    let mut msg = MSG::default();
                    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                    let _ = RemoveClipboardFormatListener(hwnd);
                    let _ = DestroyWindow(hwnd);
                    let _ = UnregisterClassW(class, Some(instance.into()));
                }
                CLIPBOARD.with(|c| *c.borrow_mut() = None);
            })
            .map_err(|e| e.to_string())?;
        let thread_id = ready_rx.recv().map_err(|e| e.to_string())??;
        Ok(Self { thread_id, thread: Some(thread) })
    }
}

impl Drop for ClipboardWatcher {
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

// ------------------------------------------------------------------------------ the desktop menu

/// Sorts after Vials' "DesquVials" entry, so it is listed under "Create vial here".
const PASTE_VERB_KEY: &str = r"Software\Classes\DesktopBackground\Shell\DesquWorkspacesPaste";

fn set_string(key: HKEY, name: Option<&str>, value: &str) -> Result<(), String> {
    let name = name.map(HSTRING::from);
    let data: Vec<u8> = value.encode_utf16().chain(Some(0)).flat_map(u16::to_le_bytes).collect();
    // SAFETY: `key` is open for writing; the buffers outlive the call.
    unsafe { RegSetValueExW(key, name.as_ref().map_or(PCWSTR::null(), |n| PCWSTR(n.as_ptr())), None, REG_SZ, Some(&data)) }.ok().map_err(|e| e.to_string())
}

fn create_key(path: &str) -> Result<HKEY, String> {
    let path = HSTRING::from(path);
    let mut key = HKEY::default();
    // SAFETY: creates or opens a key under the current user's hive.
    unsafe { RegCreateKeyExW(HKEY_CURRENT_USER, PCWSTR(path.as_ptr()), None, PCWSTR::null(), REG_OPTION_NON_VOLATILE, KEY_WRITE, None, &mut key, None) }
        .ok()
        .map_err(|e| e.to_string())?;
    Ok(key)
}

/// Adds (`Some(exe)`) or removes (`None`) "Paste on this workspace" in the desktop's right-click menu.
/// It runs `exe --action=paste-here`, which the running app turns into a paste of the clipboard's
/// files onto the virtual desktop you are on. The entry follows the clipboard: it is there only
/// while files are copied or cut. On Windows 11 it is under "Show more options".
pub fn set_paste_entry(exe: Option<&str>) -> Result<(), String> {
    match exe {
        Some(exe) => {
            let key = create_key(PASTE_VERB_KEY)?;
            let result = set_string(key, None, "Paste on this workspace").and_then(|()| set_string(key, Some("Icon"), &format!("\"{exe}\",0")));
            // SAFETY: closes the key opened above.
            unsafe {
                let _ = RegCloseKey(key);
            }
            result?;
            let command = create_key(&format!(r"{PASTE_VERB_KEY}\command"))?;
            let result = set_string(command, None, &format!("\"{exe}\" --action=paste-here"));
            // SAFETY: closes the key opened above.
            unsafe {
                let _ = RegCloseKey(command);
            }
            result
        }
        None => {
            let path = HSTRING::from(PASTE_VERB_KEY);
            // SAFETY: deletes only our own verb key and its `command` subkey.
            let status = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(path.as_ptr())) };
            if status.is_ok() || status == ::windows::Win32::Foundation::ERROR_FILE_NOT_FOUND {
                Ok(())
            } else {
                Err(format!("could not remove the menu entry: {status:?}"))
            }
        }
    }
}

// ------------------------------------------------------------------------------------ hotkeys

/// Win+Ctrl+1..9: calls `on_digit(n)` with the number pressed. Needs no window, only a thread that reads
/// its message queue. Dropping it gives the keys back.
pub struct HotkeyWatcher {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl HotkeyWatcher {
    pub fn start(on_digit: Box<dyn Fn(u32) + Send + 'static>) -> Result<Self, String> {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<u32, String>>();
        let thread = std::thread::Builder::new()
            .name("workspaces-hotkeys".into())
            .spawn(move || {
                // SAFETY: the keys are registered and released on this thread, which owns the queue.
                unsafe {
                    let modifiers = HOT_KEY_MODIFIERS(MOD_WIN.0 | MOD_CONTROL.0 | MOD_NOREPEAT.0);
                    let mut taken = Vec::new();
                    for digit in 1..=9u32 {
                        if RegisterHotKey(None, digit as i32, modifiers, u32::from(b'0') + digit).is_ok() {
                            taken.push(digit);
                        }
                    }
                    if taken.is_empty() {
                        let _ = ready_tx.send(Err("Windows did not give Workspaces any Win+Ctrl+number key".into()));
                        return;
                    }
                    let _ = ready_tx.send(Ok(GetCurrentThreadId()));
                    let mut msg = MSG::default();
                    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                        if msg.message == WM_HOTKEY {
                            on_digit(msg.wParam.0 as u32);
                        }
                    }
                    for digit in taken {
                        let _ = UnregisterHotKey(None, digit as i32);
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        let thread_id = ready_rx.recv().map_err(|e| e.to_string())??;
        Ok(Self { thread_id, thread: Some(thread) })
    }
}

impl Drop for HotkeyWatcher {
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

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, time: 0, dwExtraInfo: 0 } },
    }
}

/// Presses Win+Ctrl+Right (or Left) `count` times: Windows' own way to move between virtual desktops.
pub fn press_desktop_keys(right: bool, count: u32) {
    let arrow = if right { VK_RIGHT } else { VK_LEFT };
    let size = std::mem::size_of::<INPUT>() as i32;
    // SAFETY: plain input events built right here.
    unsafe {
        SendInput(&[key(VK_LWIN, false), key(VK_CONTROL, false)], size);
        for _ in 0..count {
            SendInput(&[key(arrow, false), key(arrow, true)], size);
            // Windows needs a moment to take each step.
            std::thread::sleep(std::time::Duration::from_millis(70));
        }
        SendInput(&[key(VK_CONTROL, true), key(VK_LWIN, true)], size);
    }
}

/// Makes a window of ours a note on the screen: on top of everything, never takes the focus, and
/// mouse clicks go through it to what is below.
pub fn make_overlay(hwnd: isize) {
    use ::windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
    };
    let hwnd = HWND(hwnd as *mut std::ffi::c_void);
    // SAFETY: our own window.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let add = (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_TRANSPARENT).0 as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | add);
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_task_views_windows_count() {
        assert!(is_task_view_class("XamlExplorerHostIslandWindow"));
        assert!(is_task_view_class("MultitaskingViewFrame"));
        assert!(!is_task_view_class("Notepad"));
    }
}
