use ::windows::Win32::Foundation::HWND;
use ::windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, IsWindowVisible, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, HWND_BOTTOM,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE, WS_EX_APPWINDOW,
    WS_EX_TOOLWINDOW,
};

/// Makes a window a tool window: it stays out of the taskbar *and* Alt+Tab. Tauri's
/// `skip_taskbar` alone leaves it in Alt+Tab, and its windows carry `WS_EX_APPWINDOW`, which forces
/// a taskbar button even on a tool window, so that flag is cleared too. Call it after the window
/// has been shown: Tauri's own window layer rewrites the extended style while showing it.
///
/// The window is hidden and shown again (without activating it) so the shell re-evaluates it, and
/// put back at the bottom of the z-order so it stays above the desktop icons only.
pub(crate) fn mark_as_tool_window(hwnd: isize) -> Result<(), String> {
    let hwnd = HWND(hwnd as *mut std::ffi::c_void);
    // SAFETY: `hwnd` is a window of this process, owned by Tauri, that is still alive.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let wanted = (ex | WS_EX_TOOLWINDOW.0 as isize) & !(WS_EX_APPWINDOW.0 as isize);
        if wanted == ex {
            return Ok(());
        }
        let was_visible = IsWindowVisible(hwnd).as_bool();
        if was_visible {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, wanted);
        let framed = SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED);
        if was_visible {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        let _ = SetWindowPos(hwnd, Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        framed.map_err(|e| e.to_string())
    }
}

// ------------------------------------------------------------------------- the desktop marquee

use std::cell::RefCell;
use std::sync::mpsc;
use std::thread::JoinHandle;

use ::windows::Win32::Foundation::{LPARAM, LRESULT, POINT, WPARAM};
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::System::Threading::GetCurrentThreadId;
use ::windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetClassNameW, GetMessageW, GetParent, PostThreadMessageW, SetWindowsHookExW,
    UnhookWindowsHookEx, WindowFromPoint, MSG, MSLLHOOKSTRUCT, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_QUIT,
};

/// Called with the press and release points of a drag that started on the desktop icon view.
pub(crate) type MarqueeHandler = Box<dyn Fn((i32, i32), (i32, i32)) + Send>;

struct HookState {
    down: Option<(i32, i32)>,
    on_marquee: MarqueeHandler,
}

thread_local! {
    // The low-level hook procedure runs on the thread that installed it.
    static HOOK: RefCell<Option<HookState>> = const { RefCell::new(None) };
}

fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 64];
    // SAFETY: valid window handle and a local buffer.
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Whether the point is on the desktop's icon view (not on a normal window above it).
fn on_desktop_view(x: i32, y: i32) -> bool {
    // SAFETY: plain window queries.
    unsafe {
        let hwnd = WindowFromPoint(POINT { x, y });
        if hwnd.0.is_null() || class_of(hwnd) != "SysListView32" {
            return false;
        }
        GetParent(hwnd).is_ok_and(|p| class_of(p) == "SHELLDLL_DefView")
    }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        // SAFETY: for WH_MOUSE_LL `lparam` points to a MSLLHOOKSTRUCT valid for this call.
        let pt = unsafe { (*(lparam.0 as *const MSLLHOOKSTRUCT)).pt };
        match wparam.0 as u32 {
            WM_LBUTTONDOWN => {
                let on_desktop = on_desktop_view(pt.x, pt.y);
                HOOK.with(|h| {
                    if let Some(state) = h.borrow_mut().as_mut() {
                        state.down = on_desktop.then_some((pt.x, pt.y));
                    }
                });
            }
            WM_LBUTTONUP => HOOK.with(|h| {
                if let Some(state) = h.borrow_mut().as_mut() {
                    if let Some(down) = state.down.take() {
                        (state.on_marquee)(down, (pt.x, pt.y));
                    }
                }
            }),
            _ => {}
        }
    }
    // SAFETY: always pass the event on.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Watches left-button presses and releases system-wide, only to see a drag that starts on the
/// desktop icon view. The callback runs inside the hook, so it must only hand the data over.
pub(crate) struct MouseWatcher {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl MouseWatcher {
    pub(crate) fn start(on_marquee: MarqueeHandler) -> Result<Self, String> {
        let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();
        let thread = std::thread::Builder::new()
            .name("vials-mouse-hook".into())
            .spawn(move || {
                HOOK.with(|h| *h.borrow_mut() = Some(HookState { down: None, on_marquee }));
                // SAFETY: the hook is removed below on this same thread, before it ends.
                unsafe {
                    let module = GetModuleHandleW(None).unwrap_or_default();
                    let hook = match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), Some(module.into()), 0) {
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
                HOOK.with(|h| *h.borrow_mut() = None);
            })
            .map_err(|e| e.to_string())?;
        let thread_id = ready_rx.recv().map_err(|e| e.to_string())??;
        Ok(Self { thread_id, thread: Some(thread) })
    }
}

impl Drop for MouseWatcher {
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
