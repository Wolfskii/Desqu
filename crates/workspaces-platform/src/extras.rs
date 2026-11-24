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
use ::windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_MENU};
use ::windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClassNameW, GetMessageW, PostThreadMessageW, RegisterClassW,
    TranslateMessage, UnregisterClassW, EVENT_SYSTEM_FOREGROUND, HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WINEVENT_OUTOFCONTEXT,
    WM_CLIPBOARDUPDATE, WM_QUIT, WNDCLASSW,
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

thread_local! {
    static CLIPBOARD: RefCell<Option<Box<dyn Fn(bool)>>> = const { RefCell::new(None) };
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
