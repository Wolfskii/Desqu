//! P9.1 spike: what Windows lets an unelevated process do with virtual desktops.
//!
//! SAFETY: read-only except for two windows this program creates itself (a test window and a
//! winver.exe it starts and closes). The test window is moved to ANOTHER virtual desktop and back;
//! the user's desktops are never created, removed, renamed or switched, and no foreign window is
//! moved (the foreign-window move is attempted only on the winver.exe started here).
//!
//! Run: `cargo run -p desqu-desktop --example spike_vdesktop`
#![cfg(windows)]

use std::process::Command;
use std::time::{Duration, Instant};

use windows::core::{w, BOOL, GUID, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HWND, LPARAM, LRESULT, WAIT_OBJECT_0, WPARAM};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Registry::{
    RegCloseKey, RegNotifyChangeKeyValue, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_NOTIFY,
    KEY_READ, REG_NOTIFY_CHANGE_LAST_SET,
};
use windows::Win32::System::Threading::{
    CreateEventW, OpenProcess, QueryFullProcessImageNameW, WaitForSingleObject, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::core::Interface;
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Shell::{IShellWindows, IVirtualDesktopManager, IWebBrowser2, ShellWindows, VirtualDesktopManager};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, EnumWindows, GetClassNameW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible, PostMessageW, RegisterClassW, ShowWindow, SW_SHOWNOACTIVATE,
    WM_CLOSE, WNDCLASSW, WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW,
};

const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\VirtualDesktops";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn guid_from_bytes(b: &[u8]) -> GUID {
    GUID::from_values(
        u32::from_le_bytes(b[0..4].try_into().unwrap()),
        u16::from_le_bytes(b[4..6].try_into().unwrap()),
        u16::from_le_bytes(b[6..8].try_into().unwrap()),
        b[8..16].try_into().unwrap(),
    )
}

unsafe fn read_value(subkey: &str, name: &str) -> Option<Vec<u8>> {
    let mut key = HKEY::default();
    let path = wide(&format!(r"{KEY}{subkey}"));
    if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(path.as_ptr()), None, KEY_READ, &mut key) != ERROR_SUCCESS {
        return None;
    }
    let n = wide(name);
    let mut len = 0u32;
    let mut out = None;
    if RegQueryValueExW(key, PCWSTR(n.as_ptr()), None, None, None, Some(&mut len)) == ERROR_SUCCESS {
        let mut buf = vec![0u8; len as usize];
        if RegQueryValueExW(key, PCWSTR(n.as_ptr()), None, None, Some(buf.as_mut_ptr()), Some(&mut len))
            == ERROR_SUCCESS
        {
            buf.truncate(len as usize);
            out = Some(buf);
        }
    }
    let _ = RegCloseKey(key);
    out
}

fn utf16(b: &[u8]) -> String {
    let u: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    String::from_utf16_lossy(&u).trim_end_matches('\0').to_string()
}

fn fmt(g: &GUID) -> String {
    format!("{{{g:?}}}").to_uppercase()
}

unsafe fn ids() -> Vec<GUID> {
    read_value("", "VirtualDesktopIDs")
        .map(|b| b.chunks_exact(16).map(guid_from_bytes).collect())
        .unwrap_or_default()
}

unsafe fn current() -> Option<GUID> {
    read_value("", "CurrentVirtualDesktop").filter(|b| b.len() >= 16).map(|b| guid_from_bytes(&b))
}

unsafe fn exe_of(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut buf = vec![0u16; 1024];
    let mut len = buf.len() as u32;
    let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
    let _ = CloseHandle(h);
    ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))
}

#[link(name = "ntdll")]
extern "system" {
    fn NtQueryInformationProcess(h: isize, class: u32, info: *mut u8, len: u32, ret: *mut u32) -> i32;
}

/// ProcessCommandLineInformation = 60 (Windows 8.1+): the command line of a same-user process.
unsafe fn command_line_of(hwnd: HWND) -> Result<String, String> {
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).map_err(|e| format!("OpenProcess: {e}"))?;
    let mut buf = vec![0u8; 8192];
    let mut ret = 0u32;
    let status = NtQueryInformationProcess(h.0 as isize, 60, buf.as_mut_ptr(), buf.len() as u32, &mut ret);
    let _ = CloseHandle(h);
    if status < 0 {
        return Err(format!("NTSTATUS {status:#x}"));
    }
    // UNICODE_STRING { u16 len, u16 max, ptr } then the characters follow in the same buffer.
    let len = u16::from_le_bytes([buf[0], buf[1]]) as usize;
    let ptr = usize::from_le_bytes(buf[8..16].try_into().unwrap());
    let base = buf.as_ptr() as usize;
    let off = ptr.checked_sub(base).ok_or("pointer outside buffer")?;
    Ok(utf16(&buf[off..off + len]))
}

unsafe extern "system" fn collect_visible(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let v = &mut *(lparam.0 as *mut Vec<HWND>);
    if IsWindowVisible(hwnd).as_bool() {
        v.push(hwnd);
    }
    BOOL(1)
}

unsafe fn visible_windows() -> Vec<HWND> {
    let mut v: Vec<HWND> = Vec::new();
    let _ = EnumWindows(Some(collect_visible), LPARAM(&mut v as *mut _ as isize));
    v
}

unsafe fn title(h: HWND) -> String {
    let mut b = [0u16; 128];
    let n = GetWindowTextW(h, &mut b);
    String::from_utf16_lossy(&b[..n as usize])
}

unsafe extern "system" fn wndproc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    DefWindowProcW(h, m, w, l)
}

fn main() {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
        println!("== 1. Desktops from the registry (no undocumented API)");
        let all = ids();
        for g in &all {
            let name = read_value(&format!(r"\Desktops\{}", fmt(g)), "Name").map(|b| utf16(&b));
            println!("   {}  name={:?}", fmt(g), name);
        }
        let cur = current();
        println!("   current = {:?}", cur.as_ref().map(fmt));
        println!("   current is one of the listed ids: {}", cur.is_some_and(|c| all.contains(&c)));

        println!("== 2. Event-driven change detection: RegNotifyChangeKeyValue on the same key");
        let mut key = HKEY::default();
        let p = wide(KEY);
        let opened = RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(p.as_ptr()), None, KEY_NOTIFY, &mut key);
        println!("   open for notify: {:?}", opened);
        let ev = CreateEventW(None, false, false, None).unwrap();
        let reg = RegNotifyChangeKeyValue(key, true, REG_NOTIFY_CHANGE_LAST_SET, Some(ev), true);
        println!("   registered asynchronously: {:?}", reg);
        // Nothing is switched here: only the registration is proven, not a delivered event.
        let waited = WaitForSingleObject(ev, 1500);
        println!("   fired without any switch in 1.5 s (expected: no): {}", waited == WAIT_OBJECT_0);
        let _ = RegCloseKey(key);
        let _ = CloseHandle(ev);

        println!("== 3. Public IVirtualDesktopManager on other processes' windows");
        let mgr: IVirtualDesktopManager = CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_ALL).unwrap();
        let wins = visible_windows();
        let (mut on_cur, mut ok_id, mut failed) = (0, 0, 0);
        let mut seen = std::collections::BTreeSet::new();
        for h in &wins {
            match mgr.IsWindowOnCurrentVirtualDesktop(*h) {
                Ok(b) if b.as_bool() => on_cur += 1,
                Ok(_) => {}
                Err(_) => failed += 1,
            }
            if let Ok(id) = mgr.GetWindowDesktopId(*h) {
                ok_id += 1;
                seen.insert(fmt(&id));
            }
        }
        println!(
            "   {} visible top-level windows: {} on the current desktop, GetWindowDesktopId ok for {}, errors {}",
            wins.len(),
            on_cur,
            ok_id,
            failed
        );
        println!("   distinct desktop ids reported: {}", seen.len());
        for s in &seen {
            let known = all.iter().any(|g| &fmt(g) == s);
            println!("   reported id {s} (in the registry list: {known})");
        }

        println!("== 4. Moving windows to another desktop (only windows created here)");
        let other = all.iter().find(|g| Some(**g) != cur).copied();
        let class = w!("DesquVdSpike");
        let wc = WNDCLASSW { lpfnWndProc: Some(wndproc), lpszClassName: class, ..Default::default() };
        RegisterClassW(&wc);
        let own = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("Desqu vd spike"),
            WS_OVERLAPPEDWINDOW,
            100,
            100,
            200,
            100,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let _ = ShowWindow(own, SW_SHOWNOACTIVATE);
        // The shell learns about a new window a moment later; before that it answers "element not found".
        let t0 = Instant::now();
        while mgr.GetWindowDesktopId(own).is_err() && t0.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(50));
        }
        println!("   shell knew the new window after {} ms", t0.elapsed().as_millis());
        println!("   own window desktop: {:?}", mgr.GetWindowDesktopId(own).as_ref().map(fmt));
        if let Some(target) = other {
            let r = mgr.MoveWindowToDesktop(own, &target);
            let after = mgr.GetWindowDesktopId(own).map(|g| fmt(&g));
            println!(
                "   MoveWindowToDesktop(own -> other): {:?}; now on {:?}; on current desktop: {:?}",
                r,
                after,
                mgr.IsWindowOnCurrentVirtualDesktop(own).map(|b| b.as_bool())
            );
            if let Some(back) = cur {
                println!("   moved back: {:?}", mgr.MoveWindowToDesktop(own, &back));
            }
        } else {
            println!("   only one desktop exists: cannot test");
        }
        let _ = DestroyWindow(own);

        let mut child = Command::new("winver.exe").spawn().expect("start winver");
        let start = Instant::now();
        let mut winver = None;
        while start.elapsed() < Duration::from_secs(10) && winver.is_none() {
            winver = visible_windows().into_iter().find(|h| {
                let mut pid = 0u32;
                GetWindowThreadProcessId(*h, Some(&mut pid));
                pid == child.id() && !title(*h).is_empty()
            });
            std::thread::sleep(Duration::from_millis(200));
        }
        if let Some(h) = winver {
            let mut cls = [0u16; 64];
            let n = GetClassNameW(h, &mut cls);
            println!("   foreign test window: class={} title={:?}", String::from_utf16_lossy(&cls[..n as usize]), title(h));
            println!("   foreign GetWindowDesktopId: {:?}", mgr.GetWindowDesktopId(h).map(|g| fmt(&g)));
            if let Some(target) = other {
                println!("   foreign MoveWindowToDesktop(other): {:?}", mgr.MoveWindowToDesktop(h, &target));
                println!("   still on current desktop: {:?}", mgr.IsWindowOnCurrentVirtualDesktop(h).map(|b| b.as_bool()));
            }
            println!("== 5. Launch info of a foreign window (same user, unelevated)");
            println!("   exe: {:?}", exe_of(h));
            println!("   command line: {:?}", command_line_of(h));
            let _ = PostMessageW(Some(h), WM_CLOSE, Default::default(), Default::default());
        } else {
            println!("   winver window did not appear");
        }
        println!("== 6. Open Explorer folder windows via IShellWindows");
        match CoCreateInstance::<_, IShellWindows>(&ShellWindows, None, CLSCTX_ALL) {
            Ok(sw) => {
                // Open one folder window of our own so there is something to read (and close it again).
                let probe_dir = std::env::temp_dir().join("desqu-vd-spike-folder");
                let _ = std::fs::create_dir_all(&probe_dir);
                let _ = Command::new("explorer.exe").arg(&probe_dir).spawn();
                let t = Instant::now();
                while sw.Count().unwrap_or(0) == 0 && t.elapsed() < Duration::from_secs(8) {
                    std::thread::sleep(Duration::from_millis(250));
                }
                let n = sw.Count().unwrap_or(0);
                println!("   {n} shell windows");
                for i in 0..n {
                    let Ok(d) = sw.Item(&VARIANT::from(i)) else { continue };
                    let Ok(b) = d.cast::<IWebBrowser2>() else { continue };
                    let url = b.LocationURL().map(|s| s.to_string()).unwrap_or_default();
                    let hwnd = b.HWND().map(|h| HWND(h.0 as *mut _)).ok();
                    let on_desktop = hwnd.and_then(|h| mgr.GetWindowDesktopId(h).ok()).map(|g| fmt(&g));
                    println!("   {url}  (desktop {on_desktop:?})");
                    if url.contains("desqu-vd-spike-folder") {
                        let _ = b.Quit();
                    }
                }
            }
            Err(e) => println!("   IShellWindows unavailable: {e}"),
        }
        let _ = std::fs::remove_dir(std::env::temp_dir().join("desqu-vd-spike-folder"));
        std::thread::sleep(Duration::from_millis(500));
        let _ = child.kill();
        let _ = child.wait();
    }
}
