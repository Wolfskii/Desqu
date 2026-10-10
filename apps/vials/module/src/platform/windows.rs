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
