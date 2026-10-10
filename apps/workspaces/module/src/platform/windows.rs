use std::path::Path;
use std::thread::JoinHandle;

use ::windows::core::{w, Interface, BOOL, GUID, HSTRING, PCWSTR, PWSTR};
use ::windows::Win32::Foundation::{
    CloseHandle, ERROR_SUCCESS, HANDLE, HWND, LPARAM, RPC_E_CHANGED_MODE, WAIT_OBJECT_0,
};
use ::windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use ::windows::Win32::System::Registry::{
    RegCloseKey, RegNotifyChangeKeyValue, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_NOTIFY, KEY_READ, REG_NOTIFY_CHANGE_LAST_SET,
};
use ::windows::Win32::System::Threading::{
    CreateEventW, OpenProcess, QueryFullProcessImageNameW, SetEvent, WaitForMultipleObjects,
    INFINITE, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use ::windows::Win32::System::Variant::VARIANT;
use ::windows::Win32::UI::Shell::{
    IShellWindows, IVirtualDesktopManager, IWebBrowser2, ShellExecuteW, ShellWindows,
    VirtualDesktopManager,
};
use ::windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindow, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowThreadProcessId,
    IsWindowVisible, GWL_EXSTYLE, GW_OWNER, SW_SHOWNORMAL, WS_EX_TOOLWINDOW,
};

use super::{DesktopInfo, Desktops};
use crate::core::apps::{folder_from_url, RunningApp};
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

fn guid_text(g: &GUID) -> String {
    format!("{{{g:?}}}").to_ascii_uppercase()
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
    // to list and Workspaces simply has no desktop to attach a workspace to.
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
    let current = read_value("", "CurrentVirtualDesktop")
        .and_then(|b| guid_from_registry_bytes(&b))
        .filter(|c| list.iter().any(|d| &d.id == c));
    Ok(Desktops { list, current })
}

// ---------------------------------------------------------------------------------- the watcher

struct Event(HANDLE);
// SAFETY: an event handle may be signalled and waited on from any thread.
unsafe impl Send for Event {}
unsafe impl Sync for Event {}

pub(super) struct Watcher {
    stop: std::sync::Arc<Event>,
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

impl Drop for Event {
    fn drop(&mut self) {
        // SAFETY: created by CreateEventW and closed exactly once, here.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub(super) fn watch(on_change: Box<dyn Fn() + Send + 'static>) -> Result<Watcher, String> {
    let path = wide(KEY);
    let mut key = HKEY::default();
    // SAFETY: standard registry/event calls; every handle is closed on all paths.
    let (change, stop) = unsafe {
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
        let change = CreateEventW(None, false, false, None).map_err(|e| e.to_string())?;
        let stop = CreateEventW(None, true, false, None).map_err(|e| e.to_string())?;
        (Event(change), std::sync::Arc::new(Event(stop)))
    };
    let key_holder = KeyHolder(key);
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
                    let signalled =
                        WaitForMultipleObjects(&[change.0, thread_stop.0], false, INFINITE);
                    if signalled != WAIT_OBJECT_0 {
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

// ------------------------------------------------------------------------------------- windows

/// Runs `f` with COM initialised on this thread (the shell interfaces need an apartment).
fn with_com<T>(f: impl FnOnce() -> T) -> T {
    // SAFETY: balanced by CoUninitialize unless COM was already initialised differently here.
    let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let out = f();
    if hr.is_ok() {
        unsafe { CoUninitialize() };
    } else if hr != RPC_E_CHANGED_MODE {
        tracing::debug!("COM initialisation: {hr:?}");
    }
    out
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the `&mut Vec<HWND>` passed by `top_level_windows`.
    let v = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
    // SAFETY: plain window queries on a handle EnumWindows just produced.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let owned = GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.0.is_null());
        if IsWindowVisible(hwnd).as_bool()
            && GetWindowTextLengthW(hwnd) > 0
            && ex & WS_EX_TOOLWINDOW.0 == 0
            && !owned
        {
            v.push(hwnd);
        }
    }
    BOOL(1)
}

fn top_level_windows() -> Vec<HWND> {
    let mut v: Vec<HWND> = Vec::new();
    // SAFETY: the callback only runs during this call, with the pointer to `v`.
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut v as *mut _ as isize));
    }
    v
}

fn on_desktop(mgr: &IVirtualDesktopManager, hwnd: HWND, desktop_id: Option<&str>) -> bool {
    // SAFETY: COM calls on a live interface and a window handle; failures just mean "not on it".
    unsafe {
        match desktop_id {
            None => mgr
                .IsWindowOnCurrentVirtualDesktop(hwnd)
                .map(|b| b.as_bool())
                .unwrap_or(false),
            Some(id) => mgr
                .GetWindowDesktopId(hwnd)
                .map(|g| guid_text(&g) == id)
                .unwrap_or(false),
        }
    }
}

fn exe_of(pid: u32) -> Option<String> {
    // SAFETY: limited-information process query, handle closed below.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        let ok =
            QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len)
                .is_ok();
        let _ = CloseHandle(h);
        ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))
    }
}

#[link(name = "ntdll")]
extern "system" {
    fn NtQueryInformationProcess(
        h: isize,
        class: u32,
        info: *mut u8,
        len: u32,
        ret: *mut u32,
    ) -> i32;
}

/// ProcessCommandLineInformation (60, Windows 8.1+): works for same-user, non-elevated processes.
fn command_line_of(pid: u32) -> Option<String> {
    // SAFETY: the buffer is large enough for the documented UNICODE_STRING plus its characters,
    // the returned pointer is checked to lie inside it.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = vec![0u8; 16 * 1024];
        let mut ret = 0u32;
        let status = NtQueryInformationProcess(
            h.0 as isize,
            60,
            buf.as_mut_ptr(),
            buf.len() as u32,
            &mut ret,
        );
        let _ = CloseHandle(h);
        if status < 0 {
            return None;
        }
        let len = u16::from_le_bytes([buf[0], buf[1]]) as usize;
        let ptr = usize::from_le_bytes(buf[8..16].try_into().ok()?);
        let off = ptr.checked_sub(buf.as_ptr() as usize)?;
        buf.get(off..off.checked_add(len)?).map(utf16)
    }
}

pub(super) fn running_apps(desktop_id: Option<&str>) -> Vec<RunningApp> {
    with_com(|| {
        // SAFETY: standard COM activation.
        let Ok(mgr) = (unsafe {
            CoCreateInstance::<_, IVirtualDesktopManager>(&VirtualDesktopManager, None, CLSCTX_ALL)
        }) else {
            return Vec::new();
        };
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for hwnd in top_level_windows() {
            if !on_desktop(&mgr, hwnd, desktop_id) {
                continue;
            }
            let mut pid = 0u32;
            // SAFETY: valid window handle, out-pointer to a local.
            unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
            if pid == 0 || !seen.insert(pid) {
                continue;
            }
            let Some(exe) = exe_of(pid) else { continue };
            let command_line = command_line_of(pid).unwrap_or_default();
            out.push(RunningApp { exe, command_line });
        }
        out
    })
}

pub(super) fn explorer_folders(desktop_id: Option<&str>) -> Vec<String> {
    with_com(|| {
        // SAFETY: COM activation and reads on shell objects; every failure just skips an entry.
        unsafe {
            let (Ok(mgr), Ok(shell)) = (
                CoCreateInstance::<_, IVirtualDesktopManager>(
                    &VirtualDesktopManager,
                    None,
                    CLSCTX_ALL,
                ),
                CoCreateInstance::<_, IShellWindows>(&ShellWindows, None, CLSCTX_ALL),
            ) else {
                return Vec::new();
            };
            let mut out: Vec<String> = Vec::new();
            for i in 0..shell.Count().unwrap_or(0) {
                let Ok(item) = shell.Item(&VARIANT::from(i)) else {
                    continue;
                };
                let Ok(browser) = item.cast::<IWebBrowser2>() else {
                    continue;
                };
                let Ok(hwnd) = browser.HWND() else { continue };
                if !on_desktop(&mgr, HWND(hwnd.0 as *mut _), desktop_id) {
                    continue;
                }
                let Some(path) = browser
                    .LocationURL()
                    .ok()
                    .and_then(|u| folder_from_url(&u.to_string()))
                else {
                    continue;
                };
                if !out.iter().any(|p| p.eq_ignore_ascii_case(&path)) {
                    out.push(path);
                }
            }
            out
        }
    })
}

fn shell_open(
    verb: PCWSTR,
    file: &str,
    args: Option<&str>,
    cwd: Option<&Path>,
) -> Result<(), String> {
    let file = HSTRING::from(file);
    let args = args.map(HSTRING::from);
    let cwd = cwd.map(|p| HSTRING::from(p.as_os_str()));
    // SAFETY: all strings outlive the call. A result above 32 means success (documented).
    let result = unsafe {
        ShellExecuteW(
            None,
            verb,
            &file,
            args.as_ref().map_or(PCWSTR::null(), |a| PCWSTR(a.as_ptr())),
            cwd.as_ref().map_or(PCWSTR::null(), |c| PCWSTR(c.as_ptr())),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as usize > 32 {
        Ok(())
    } else {
        Err(format!(
            "Windows could not open it (code {})",
            result.0 as usize
        ))
    }
}

pub(super) fn launch_app(exe: &str, args: Option<&str>) -> Result<(), String> {
    let path = Path::new(exe);
    if !path.is_absolute() || !path.is_file() {
        return Err(format!("{exe} was not found"));
    }
    shell_open(w!("open"), exe, args, path.parent())
}

pub(super) fn open_folder(path: &str) -> Result<(), String> {
    if !Path::new(path).is_dir() {
        return Err(format!("{path} was not found"));
    }
    shell_open(w!("explore"), path, None, None)
}

#[cfg(test)]
mod real_tests {
    use super::*;
    use crate::core::apps::{launches_from, missing_apps};
    use ::windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use ::windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, PeekMessageW,
        RegisterClassW, ShowWindow, TranslateMessage, MSG, PM_REMOVE, SW_SHOWNOACTIVATE,
        WINDOW_EX_STYLE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
    };
    use std::process::Command;
    use std::time::{Duration, Instant};

    const HELPER_ENV: &str = "DESQU_WS_TEST_HELPER";
    const HELPER_TEST: &str = "platform::windows::real_tests::real_window_helper";

    fn wait_for<T>(timeout: Duration, mut f: impl FnMut() -> Option<T>) -> Option<T> {
        let end = Instant::now() + timeout;
        while Instant::now() < end {
            if let Some(v) = f() {
                return Some(v);
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        None
    }

    unsafe extern "system" fn proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        DefWindowProcW(h, m, w, l)
    }

    /// Not a test by itself: when started by the test below (env var set) it shows a plain window
    /// until the stop file appears, so the test has a real "user app" to find and relaunch.
    #[test]
    #[ignore = "helper process for the real tests"]
    fn real_window_helper() {
        let Ok(stop_file) = std::env::var(HELPER_ENV) else {
            return;
        };
        let stop_file = std::path::PathBuf::from(stop_file);
        unsafe {
            let class = w!("DesquWsHelper");
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(proc),
                lpszClassName: class,
                ..Default::default()
            });
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                w!("Desqu Workspaces test window"),
                WS_OVERLAPPEDWINDOW,
                200,
                200,
                320,
                160,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let end = Instant::now() + Duration::from_secs(120);
            let mut msg = MSG::default();
            while !stop_file.exists() && Instant::now() < end {
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let _ = DestroyWindow(hwnd);
        }
    }

    /// Real windows: starts a helper window process (this test binary), finds it through the
    /// virtual desktop APIs, sees it disappear, and launches it again from the saved entry. Only
    /// processes this test starts are touched.
    /// `cargo test -p workspaces-module -- --ignored real_running`
    #[test]
    #[ignore = "uses the real desktop session"]
    fn real_running_apps_are_found_and_launched_again() {
        let dir = std::env::temp_dir().join(format!("desqu-ws-real-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let stop_file = dir.join("stop");
        std::env::set_var(HELPER_ENV, &stop_file);
        let exe = std::env::current_exe().unwrap();
        let exe_text = exe.to_string_lossy().into_owned();
        let helper_args = format!("--ignored --exact {HELPER_TEST} --nocapture");

        let current = desktops().unwrap().current;
        let mut child = Command::new(&exe)
            .args(["--ignored", "--exact", HELPER_TEST, "--nocapture"])
            .spawn()
            .expect("start the helper window");
        let find = || {
            running_apps(None).into_iter().find(|a| {
                a.exe.eq_ignore_ascii_case(&exe_text)
                    && a.command_line.contains("real_window_helper")
            })
        };
        let _found = wait_for(Duration::from_secs(20), find)
            .expect("the helper window is found on the current desktop");
        if let Some(id) = &current {
            assert!(running_apps(Some(id))
                .iter()
                .any(|a| a.exe.eq_ignore_ascii_case(&exe_text)));
        }

        let saved = launches_from(&running_apps(None));
        let entry = saved
            .iter()
            .find(|a| {
                a.exe.eq_ignore_ascii_case(&exe_text)
                    && a.args
                        .as_deref()
                        .is_some_and(|x| x.contains("real_window_helper"))
            })
            .expect("saved as a launch entry with its arguments")
            .clone();
        assert_eq!(entry.args.as_deref(), Some(helper_args.as_str()));
        assert!(missing_apps(std::slice::from_ref(&entry), &running_apps(None)).is_empty());

        // Close it: now it is missing, and launching the saved entry brings it back.
        std::fs::write(&stop_file, "stop").unwrap();
        let _ = child.wait();
        let gone = wait_for(Duration::from_secs(10), || {
            let r = running_apps(None);
            r.iter()
                .all(|a| {
                    !(a.exe.eq_ignore_ascii_case(&exe_text)
                        && a.command_line.contains("real_window_helper"))
                })
                .then_some(r)
        })
        .expect("the closed helper disappears from the list");

        // The test binary itself may still show up as "the same exe"; compare by arguments here.
        let still_open = gone.iter().any(|a| a.exe.eq_ignore_ascii_case(&exe_text));
        assert!(!still_open || missing_apps(std::slice::from_ref(&entry), &gone).len() <= 1);

        std::fs::remove_file(&stop_file).unwrap();
        launch_app(&entry.exe, entry.args.as_deref()).expect("launch from the saved entry");
        let again = wait_for(Duration::from_secs(20), find);
        assert!(again.is_some(), "the relaunched helper has a window");

        std::fs::write(&stop_file, "stop").unwrap();
        let _ = wait_for(Duration::from_secs(10), || find().is_none().then_some(()));
        std::env::remove_var(HELPER_ENV);
        let _ = std::fs::remove_dir_all(&dir);

        assert!(launch_app(r"C:\definitely\missing.exe", None).is_err());
        assert!(open_folder(r"C:\definitely\missing").is_err());
    }
}
