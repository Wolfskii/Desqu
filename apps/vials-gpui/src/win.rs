//! The few Win32 calls the GPUI window needs. Native code stays in this file.

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use vials_core::layout::MonitorRef;
use vials_core::model::PixelRect;
use windows::core::{w, BOOL, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, GetWindowRect, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, HWND_BOTTOM, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, ShowWindow, SW_HIDE, SW_SHOWNOACTIVATE, SW_SHOWNORMAL, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW,
};

pub fn hwnd(window: &gpui::Window) -> Option<HWND> {
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(HWND(h.hwnd.get() as *mut std::ffi::c_void)),
        _ => None,
    }
}

/// Every monitor in physical pixels, with its Windows name (`\.\DISPLAY2`) and DPI scale: what the
/// saved vials refer to.
pub fn monitors() -> Vec<MonitorRef> {
    unsafe extern "system" fn each(m: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        // SAFETY: `data` is the `&mut Vec` passed below and outlives the enumeration.
        let out = unsafe { &mut *(data.0 as *mut Vec<MonitorRef>) };
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        // SAFETY: `info` is correctly sized and initialised.
        if unsafe { GetMonitorInfoW(m, &mut info as *mut _ as *mut MONITORINFO) }.as_bool() {
            let (mut dx, mut dy) = (96u32, 96u32);
            // SAFETY: plain out-parameters.
            let _ = unsafe { GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) };
            let n = info.szDevice.iter().position(|&c| c == 0).unwrap_or(info.szDevice.len());
            let r = info.monitorInfo.rcMonitor;
            out.push(MonitorRef {
                name: String::from_utf16_lossy(&info.szDevice[..n]),
                bounds: PixelRect { left: r.left, top: r.top, right: r.right, bottom: r.bottom },
                primary: info.monitorInfo.dwFlags & 1 != 0,
                scale: f64::from(dx) / 96.0,
            });
        }
        BOOL(1)
    }
    let mut out: Vec<MonitorRef> = Vec::new();
    // SAFETY: the callback only uses `data` during this call.
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(each), LPARAM(&mut out as *mut _ as isize));
    }
    out
}

/// A vial window: a tool window (no Alt+Tab entry) that never takes focus, rounded like Windows 11's.
pub fn make_vial_window(hwnd: HWND) {
    // SAFETY: our own window; plain style and attribute writes.
    unsafe {
        leave_taskbar(hwnd, WS_EX_NOACTIVATE.0);
        let pref = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const _ as *const std::ffi::c_void,
            std::mem::size_of_val(&pref) as u32,
        );
    }
}

/// Puts the window at a physical-pixel rectangle, below every other window, without activating it.
pub fn place_at_bottom(hwnd: HWND, rect: &PixelRect) {
    // SAFETY: our own window.
    unsafe {
        let _ = SetWindowPos(hwnd, Some(HWND_BOTTOM), rect.left, rect.top, rect.width(), rect.height(), SWP_NOACTIVATE);
    }
}

/// A popup: rounded like Windows 11's, no Alt+Tab entry.
pub fn make_popup(hwnd: HWND) {
    // SAFETY: our own window.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_TOOLWINDOW.0 as isize);
        let pref = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const _ as *const std::ffi::c_void,
            std::mem::size_of_val(&pref) as u32,
        );
    }
}

/// Puts the window at a physical-pixel rectangle, above every other window.
pub fn place_on_top(hwnd: HWND, rect: &PixelRect) {
    // SAFETY: our own window.
    unsafe {
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), rect.left, rect.top, rect.width(), rect.height(), SWP_NOACTIVATE);
    }
}

/// Where the pointer is, in physical screen pixels.
pub fn cursor_position() -> Option<(i32, i32)> {
    vials_platform::cursor_position()
}

/// Lets the window be activated (and so receive keys) while the user works in it.
pub fn take_keyboard(hwnd: HWND) {
    // SAFETY: our own window.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !(WS_EX_NOACTIVATE.0 as isize));
    }
}

/// Back to a window that never takes focus, below everything else.
pub fn release_keyboard(hwnd: HWND) {
    // SAFETY: our own window.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE.0 as isize);
        let _ = SetWindowPos(hwnd, Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE);
    }
}

/// The window's current rectangle in physical pixels.
pub fn window_rect(hwnd: HWND) -> Option<PixelRect> {
    let mut r = RECT::default();
    // SAFETY: our own window and a valid out-pointer.
    unsafe { GetWindowRect(hwnd, &mut r) }.ok()?;
    Some(PixelRect { left: r.left, top: r.top, right: r.right, bottom: r.bottom })
}

/// [`window_rect`] for a handle kept as a plain number.
pub fn window_rect_of(raw: isize) -> Option<PixelRect> {
    window_rect(HWND(raw as *mut std::ffi::c_void))
}

/// Opens a file, folder or shortcut like a double click on the desktop.
pub fn open_path(path: &str) {
    let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    // SAFETY: NUL-terminated buffers that outlive the call.
    unsafe {
        ShellExecuteW(None, w!("open"), PCWSTR(wide.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

/// Gives a window the tool-window style and takes it off the taskbar and Task View. The taskbar only
/// re-reads a window's styles when it is shown, so the window is hidden and shown again (never activated).
fn leave_taskbar(hwnd: HWND, extra_style: u32) {
    // SAFETY: our own window.
    unsafe {
        let visible = ShowWindow(hwnd, SW_HIDE).as_bool();
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let ex = (ex | (WS_EX_TOOLWINDOW.0 | extra_style) as isize) & !(WS_EX_APPWINDOW.0 as isize);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
        if visible {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
    }
}
