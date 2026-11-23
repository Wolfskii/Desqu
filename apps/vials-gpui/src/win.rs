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
    GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, HWND_BOTTOM, SWP_NOACTIVATE, SW_SHOWNORMAL, WS_EX_NOACTIVATE,
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
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) as isize);
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

/// Opens a file, folder or shortcut like a double click on the desktop.
pub fn open_path(path: &str) {
    let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    // SAFETY: NUL-terminated buffers that outlive the call.
    unsafe {
        ShellExecuteW(None, w!("open"), PCWSTR(wide.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}
