//! Small Windows calls the Desqu tools share: the window in front, keeping one on top with a coloured border,
//! keeping the PC awake and the light or dark setting of Windows' apps.

#[cfg(windows)]
mod imp {
    use std::sync::mpsc::{channel, Sender};
    use std::thread::JoinHandle;

    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};
    use windows::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED};
    use windows::Win32::System::Registry::{RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_DWORD, RRF_RT_REG_DWORD};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowLongPtrW, SendMessageTimeoutW, SetWindowPos, GWL_EXSTYLE, HWND_BROADCAST, HWND_NOTOPMOST, HWND_TOPMOST, SMTO_ABORTIFHUNG,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WM_SETTINGCHANGE, WS_EX_TOPMOST,
    };

    fn hwnd(raw: isize) -> HWND {
        HWND(raw as *mut std::ffi::c_void)
    }

    /// Minutes since midnight on the clock of this PC.
    pub fn minute_of_day_now() -> u16 {
        use windows::Win32::System::SystemInformation::GetLocalTime;
        // SAFETY: a plain call without arguments but its result.
        let t = unsafe { GetLocalTime() };
        t.wHour * 60 + t.wMinute
    }

    /// The window the user is working in.
    pub fn foreground() -> Option<isize> {
        // SAFETY: a plain query.
        let h = unsafe { GetForegroundWindow() };
        (!h.0.is_null()).then_some(h.0 as isize)
    }

    pub fn is_topmost(window: isize) -> bool {
        // SAFETY: a plain query about a window handle (a stale one gives 0).
        unsafe { GetWindowLongPtrW(hwnd(window), GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0 != 0 }
    }

    /// Puts a window above all others (or back among them), without moving or activating it.
    pub fn set_topmost(window: isize, on: bool) {
        // SAFETY: a call on a window of another program; a stale handle just fails.
        unsafe {
            let _ = SetWindowPos(hwnd(window), Some(if on { HWND_TOPMOST } else { HWND_NOTOPMOST }), 0, 0, 0, 0, SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE);
        }
    }

    /// Colours the border of a window (Windows 11), `rgb` as `0xRRGGBB`, or gives it its own colour back with `None`.
    pub fn set_border_color(window: isize, rgb: Option<u32>) {
        // DWMWA_BORDER_COLOR is 34; the value is a COLORREF (0x00BBGGRR); 0xFFFFFFFF means "the usual".
        let value: u32 = match rgb {
            Some(c) => ((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF),
            None => 0xFFFF_FFFF,
        };
        // SAFETY: a call on a window of another program; a refusal is ignored.
        unsafe {
            let _ = DwmSetWindowAttribute(hwnd(window), DWMWINDOWATTRIBUTE(34), &value as *const _ as *const std::ffi::c_void, 4);
        }
    }

    /// Keeps the PC (and, with `display`, the screen) from going to sleep until it is dropped.
    pub struct KeepAwake {
        stop: Option<Sender<()>>,
        thread: Option<JoinHandle<()>>,
    }

    impl KeepAwake {
        pub fn start(display: bool) -> KeepAwake {
            let (stop, wait) = channel::<()>();
            // The request belongs to the thread that makes it: a thread of its own holds it.
            let thread = std::thread::Builder::new()
                .name("desqu-keep-awake".into())
                .spawn(move || {
                    let mut flags = ES_CONTINUOUS | ES_SYSTEM_REQUIRED;
                    if display {
                        flags |= ES_DISPLAY_REQUIRED;
                    }
                    // SAFETY: plain call; the state is reset below.
                    unsafe { SetThreadExecutionState(flags) };
                    let _ = wait.recv();
                    // SAFETY: as above.
                    unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
                })
                .ok();
            KeepAwake { stop: Some(stop), thread }
        }
    }

    impl Drop for KeepAwake {
        fn drop(&mut self) {
            drop(self.stop.take());
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }

    const PERSONALIZE: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");

    /// Whether Windows' apps use the light theme.
    pub fn apps_use_light() -> Option<bool> {
        let mut data = 0u32;
        let mut len = 4u32;
        // SAFETY: `data` is a 4-byte buffer and `len` says so.
        let status = unsafe { RegGetValueW(HKEY_CURRENT_USER, PERSONALIZE, w!("AppsUseLightTheme"), RRF_RT_REG_DWORD, None, Some(&mut data as *mut u32 as *mut std::ffi::c_void), Some(&mut len)) };
        (status.0 == 0).then_some(data != 0)
    }

    /// Switches Windows (its apps and its own surfaces) to the light or the dark theme and tells the programs that run.
    pub fn set_light(light: bool) -> Result<(), String> {
        let value: u32 = u32::from(light);
        for name in [w!("AppsUseLightTheme"), w!("SystemUsesLightTheme")] {
            // SAFETY: the value is a 4-byte integer that outlives the call.
            let status = unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, PERSONALIZE, name, REG_DWORD.0, Some(&value as *const u32 as *const std::ffi::c_void), 4) };
            if status.0 != 0 {
                return Err(format!("could not change the theme setting (error {})", status.0));
            }
        }
        // SAFETY: a broadcast with a string that outlives the call.
        unsafe {
            let _ = SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, WPARAM(0), LPARAM(w!("ImmersiveColorSet").as_ptr() as isize), SMTO_ABORTIFHUNG, 200, None);
        }
        Ok(())
    }
}

#[cfg(windows)]
pub use imp::*;
