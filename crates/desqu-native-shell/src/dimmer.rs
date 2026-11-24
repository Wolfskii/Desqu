//! A focus dimmer: everything on every screen is darkened except the window you are working in. One dark, click-
//! through window covers each monitor with a hole cut where the window in front is, and follows it about ten times
//! a second. Dropping the [`Dimmer`] takes the dark away.

/// A rectangle as left, top, right, bottom.
pub type Rect = [i32; 4];

/// The part of `window` that is on `monitor`, in coordinates of the monitor's own top-left, or `None` when they
/// do not meet. This is the hole that is cut in the dark window of that monitor.
pub fn hole(monitor: Rect, window: Rect) -> Option<Rect> {
    let (l, t) = (monitor[0].max(window[0]), monitor[1].max(window[1]));
    let (r, b) = (monitor[2].min(window[2]), monitor[3].min(window[3]));
    (l < r && t < b).then_some([l - monitor[0], t - monitor[1], r - monitor[0], b - monitor[1]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hole_is_the_part_of_the_window_on_the_monitor_in_the_monitors_own_coordinates() {
        let monitor = [1920, 0, 3840, 1080];
        assert_eq!(hole(monitor, [2000, 100, 2800, 700]), Some([80, 100, 880, 700]));
        // A window across two monitors is cut at the edge.
        assert_eq!(hole(monitor, [1500, 100, 2100, 700]), Some([0, 100, 180, 700]));
        assert_eq!(hole(monitor, [0, 0, 1920, 1080]), None, "on the other monitor");
        assert_eq!(hole(monitor, [2000, 100, 2000, 700]), None, "no width");
    }
}

#[cfg(windows)]
pub use imp::Dimmer;

#[cfg(windows)]
mod imp {
    use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
    use std::sync::Arc;
    use std::thread::JoinHandle;
    use std::time::Duration;

    use windows::core::{w, BOOL};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
    use windows::Win32::Graphics::Gdi::{CombineRgn, CreateRectRgn, DeleteObject, EnumDisplayMonitors, GetMonitorInfoW, GetStockObject, SetWindowRgn, BLACK_BRUSH, HBRUSH, HDC, HGDIOBJ, HMONITOR, MONITORINFO, RGN_DIFF};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClassNameW, GetForegroundWindow, PeekMessageW, RegisterClassW, SetLayeredWindowAttributes, SetWindowPos,
        ShowWindow, TranslateMessage, HWND_TOPMOST, LWA_ALPHA, MSG, PM_REMOVE, SWP_NOACTIVATE, SW_HIDE, SW_SHOWNOACTIVATE, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
    };

    use super::{hole, Rect};

    /// The dimmer while it runs.
    pub struct Dimmer {
        stop: Arc<AtomicBool>,
        level: Arc<AtomicU8>,
        thread: Option<JoinHandle<()>>,
    }

    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        // SAFETY: default handling for everything.
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    fn monitors() -> Vec<Rect> {
        unsafe extern "system" fn each(m: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
            let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            // SAFETY: `info` is sized and `data` is the `&mut Vec` passed below.
            unsafe {
                if GetMonitorInfoW(m, &mut info).as_bool() {
                    let r = info.rcMonitor;
                    (*(data.0 as *mut Vec<Rect>)).push([r.left, r.top, r.right, r.bottom]);
                }
            }
            BOOL(1)
        }
        let mut list: Vec<Rect> = Vec::new();
        // SAFETY: the callback only pushes into the list that outlives the call.
        unsafe {
            let _ = EnumDisplayMonitors(None, None, Some(each), LPARAM(&mut list as *mut _ as isize));
        }
        list
    }

    /// Where the window in front is (its visible frame), unless it is the desktop or the taskbar.
    fn front_window() -> Option<Rect> {
        // SAFETY: plain queries.
        unsafe {
            let h = GetForegroundWindow();
            if h.0.is_null() {
                return None;
            }
            let mut class = [0u16; 64];
            let n = GetClassNameW(h, &mut class) as usize;
            let class = String::from_utf16_lossy(&class[..n]);
            if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
                return None;
            }
            let mut r = RECT::default();
            DwmGetWindowAttribute(h, DWMWA_EXTENDED_FRAME_BOUNDS, &mut r as *mut _ as *mut std::ffi::c_void, std::mem::size_of::<RECT>() as u32).ok()?;
            Some([r.left, r.top, r.right, r.bottom])
        }
    }

    impl Dimmer {
        /// Starts dimming; `level` is how dark, 0 (not at all) to 255.
        pub fn start(level: u8) -> Dimmer {
            let stop = Arc::new(AtomicBool::new(false));
            let level = Arc::new(AtomicU8::new(level));
            let (flag, dark) = (stop.clone(), level.clone());
            let thread = std::thread::Builder::new().name("desqu-dimmer".into()).spawn(move || run(flag, dark)).ok();
            Dimmer { stop, level, thread }
        }

        /// How dark it is now, 0 to 255.
        pub fn set_level(&self, level: u8) {
            self.level.store(level, Ordering::Relaxed);
        }
    }

    impl Drop for Dimmer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }

    fn run(stop: Arc<AtomicBool>, level: Arc<AtomicU8>) {
        // SAFETY: windows are made, used and destroyed on this thread, which owns their message queue.
        unsafe {
            let Ok(module) = GetModuleHandleW(None) else { return };
            let class = w!("DesquDimmer");
            let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: module.into(), lpszClassName: class, hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0), ..Default::default() };
            RegisterClassW(&wc);
            let mut covers: Vec<(Rect, HWND)> = Vec::new();
            let mut known: Vec<Rect> = Vec::new();
            while !stop.load(Ordering::Relaxed) {
                // The monitors can change: the dark windows follow.
                let now = monitors();
                if now != known {
                    for (_, h) in covers.drain(..) {
                        let _ = DestroyWindow(h);
                    }
                    for m in &now {
                        let ex = WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
                        if let Ok(h) = CreateWindowExW(ex, class, w!(""), WS_POPUP, m[0], m[1], m[2] - m[0], m[3] - m[1], None, None, Some(module.into()), None) {
                            covers.push((*m, h));
                        }
                    }
                    known = now;
                }
                let front = front_window();
                let alpha = level.load(Ordering::Relaxed);
                for (monitor, h) in &covers {
                    match front {
                        None => {
                            let _ = ShowWindow(*h, SW_HIDE);
                        }
                        Some(window) => {
                            let (w, hgt) = (monitor[2] - monitor[0], monitor[3] - monitor[1]);
                            let all = CreateRectRgn(0, 0, w, hgt);
                            if let Some(cut) = hole(*monitor, window) {
                                let part = CreateRectRgn(cut[0], cut[1], cut[2], cut[3]);
                                CombineRgn(Some(all), Some(all), Some(part), RGN_DIFF);
                                let _ = DeleteObject(HGDIOBJ(part.0));
                            }
                            // The window owns the region from here on.
                            SetWindowRgn(*h, Some(all), true);
                            let _ = SetLayeredWindowAttributes(*h, Default::default(), alpha, LWA_ALPHA);
                            let _ = ShowWindow(*h, SW_SHOWNOACTIVATE);
                            let _ = SetWindowPos(*h, Some(HWND_TOPMOST), monitor[0], monitor[1], w, hgt, SWP_NOACTIVATE);
                        }
                    }
                }
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            for (_, h) in covers {
                let _ = DestroyWindow(h);
            }
        }
    }
}
