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

    /// One monitor whose brightness can be set from here (an external one that speaks DDC/CI).
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Brightness {
        /// Which monitor in the order Windows lists them now.
        pub index: usize,
        pub name: String,
        pub min: u32,
        pub current: u32,
        pub max: u32,
    }

    /// The monitors that answer to brightness, with where they are now. Slow (each is asked over its cable).
    pub fn monitors_brightness() -> Vec<Brightness> {
        use windows::Win32::Devices::Display::{DestroyPhysicalMonitors, GetMonitorBrightness, GetNumberOfPhysicalMonitorsFromHMONITOR, GetPhysicalMonitorsFromHMONITOR, PHYSICAL_MONITOR};
        let mut out = Vec::new();
        for (index, handle) in handles().into_iter().enumerate() {
            // SAFETY: the physical monitor array is made here and released below.
            unsafe {
                let mut count = 0u32;
                if GetNumberOfPhysicalMonitorsFromHMONITOR(handle, &mut count).is_err() || count == 0 {
                    continue;
                }
                let mut list = vec![PHYSICAL_MONITOR::default(); count as usize];
                if GetPhysicalMonitorsFromHMONITOR(handle, &mut list).is_err() {
                    continue;
                }
                if let Some(first) = list.first() {
                    let (mut min, mut cur, mut max) = (0u32, 0u32, 0u32);
                    if GetMonitorBrightness(first.hPhysicalMonitor, &mut min, &mut cur, &mut max) != 0 {
                        let description = first.szPhysicalMonitorDescription;
                        let name = String::from_utf16_lossy(&description).trim_end_matches('\0').to_string();
                        out.push(Brightness { index, name, min, current: cur, max });
                    }
                }
                let _ = DestroyPhysicalMonitors(&list);
            }
        }
        out
    }

    /// Sets the brightness of the monitor at `index` (as listed by [`monitors_brightness`]) to `percent`.
    pub fn set_brightness(index: usize, percent: u32) -> Result<(), String> {
        use windows::Win32::Devices::Display::{DestroyPhysicalMonitors, GetMonitorBrightness, GetNumberOfPhysicalMonitorsFromHMONITOR, GetPhysicalMonitorsFromHMONITOR, SetMonitorBrightness, PHYSICAL_MONITOR};
        let handle = *handles().get(index).ok_or("that monitor is gone")?;
        // SAFETY: as above.
        unsafe {
            let mut count = 0u32;
            GetNumberOfPhysicalMonitorsFromHMONITOR(handle, &mut count).map_err(|e| e.to_string())?;
            let mut list = vec![PHYSICAL_MONITOR::default(); count as usize];
            GetPhysicalMonitorsFromHMONITOR(handle, &mut list).map_err(|e| e.to_string())?;
            let first = list.first().ok_or("no physical monitor")?.hPhysicalMonitor;
            let (mut min, mut cur, mut max) = (0u32, 0u32, 0u32);
            let result = if GetMonitorBrightness(first, &mut min, &mut cur, &mut max) == 0 { Err("this monitor does not take brightness".to_string()) } else { Ok(super::raw_from_percent(percent, min, max)) };
            let result = result.and_then(|raw| if SetMonitorBrightness(first, raw) == 0 { Err("the monitor refused".to_string()) } else { Ok(()) });
            let _ = DestroyPhysicalMonitors(&list);
            result
        }
    }

    fn handles() -> Vec<windows::Win32::Graphics::Gdi::HMONITOR> {
        use windows::core::BOOL;
        use windows::Win32::Foundation::RECT;
        use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};
        unsafe extern "system" fn each(m: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
            // SAFETY: `data` is the `&mut Vec` passed below and outlives the enumeration.
            unsafe { (*(data.0 as *mut Vec<HMONITOR>)).push(m) };
            BOOL(1)
        }
        let mut list: Vec<HMONITOR> = Vec::new();
        // SAFETY: the callback only pushes into the list that outlives the call.
        unsafe {
            let _ = EnumDisplayMonitors(None, None, Some(each), LPARAM(&mut list as *mut _ as isize));
        }
        list
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

/// The raw value for a brightness in percent, given the range the monitor reports.
pub fn raw_from_percent(percent: u32, min: u32, max: u32) -> u32 {
    let (min, max) = (min.min(max), max);
    min + (max - min) * percent.min(100) / 100
}

/// The percent for a raw value (0 when the range is empty).
pub fn percent_from_raw(raw: u32, min: u32, max: u32) -> u32 {
    if max <= min {
        return 0;
    }
    (raw.clamp(min, max) - min) * 100 / (max - min)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_is_converted_between_percent_and_the_range_of_the_monitor() {
        assert_eq!(raw_from_percent(0, 0, 100), 0);
        assert_eq!(raw_from_percent(50, 0, 100), 50);
        assert_eq!(raw_from_percent(50, 10, 90), 50);
        assert_eq!(raw_from_percent(250, 0, 100), 100, "never above the top");
        assert_eq!(percent_from_raw(50, 10, 90), 50);
        assert_eq!(percent_from_raw(5, 10, 90), 0);
        assert_eq!(percent_from_raw(3, 3, 3), 0, "an empty range");
    }

    /// Asks the real monitors (changes nothing): run by hand with `--ignored`.
    #[cfg(windows)]
    #[test]
    #[ignore = "asks the real monitors"]
    fn the_real_monitors_are_asked_for_their_brightness() {
        let monitors = monitors_brightness();
        println!("{monitors:?}");
    }
}
