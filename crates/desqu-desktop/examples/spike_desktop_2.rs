//! P8.1 spike, part 2: off-screen parking, change notifications, and where a window can sit
//! relative to the desktop icons.
//!
//! SAFETY: the only desktop item touched is the temporary file this program creates and deletes.
//! The test windows are small, created and destroyed here; nothing is minimized or reparented
//! permanently and Win+D / Explorer restarts are deliberately NOT exercised.
//!
//! Run: `cargo run -p desqu-desktop --example spike_desktop_2`
#![cfg(windows)]

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use windows::core::{w, Interface, BOOL, GUID, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetDC, GetPixel, ReleaseDC, CLR_INVALID};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadDirectoryChangesW, FILE_FLAG_BACKUP_SEMANTICS, FILE_LIST_DIRECTORY,
    FILE_NOTIFY_CHANGE_FILE_NAME, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, IServiceProvider, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, IFolderView2, IShellBrowser, IShellItem, IShellWindows, SHChangeNotifyDeregister,
    SHChangeNotifyRegister, SHChangeNotifyEntry, SHCreateItemFromIDList, SHGetKnownFolderIDList,
    SHGetKnownFolderPath, ShellWindows, KF_FLAG_DEFAULT, SIGDN_NORMALDISPLAY, SVGIO_ALLVIEW, SVSI_POSITIONITEM,
    SWC_DESKTOP, SWFO_NEEDDISPATCH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumChildWindows, EnumWindows, FindWindowW,
    GetClassNameW, GetForegroundWindow, GetMessageW, GetParent, GetSystemMetrics, GetWindow, GetWindowRect,
    IsWindowVisible, PeekMessageW, PostMessageW, RegisterClassW, SendMessageTimeoutW, SetWindowPos,
    ShowWindow, TranslateMessage, CW_USEDEFAULT, GW_HWNDNEXT, HWND_BOTTOM, HWND_TOP, MSG, PM_REMOVE,
    WindowFromPoint, SMTO_NORMAL, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_SHOWNOACTIVATE, WM_USER, WNDCLASSW, WS_CHILD, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_POPUP, WS_VISIBLE,
};

type R<T> = windows::core::Result<T>;

const WM_SHELL_CHANGE: u32 = WM_USER + 1;

unsafe fn desktop_view() -> R<IFolderView2> {
    let windows: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_ALL)?;
    let mut hwnd = 0i32;
    let dispatch = windows.FindWindowSW(&VARIANT::from(0i32), &VARIANT::default(), SWC_DESKTOP, &mut hwnd, SWFO_NEEDDISPATCH)?;
    let provider: IServiceProvider = dispatch.cast()?;
    let browser: IShellBrowser = provider.QueryService(&GUID::from_u128(0x4c96be40_915c_11cf_99d3_00aa004ae837))?;
    browser.QueryActiveShellView()?.cast::<IFolderView2>()
}

unsafe fn desktop_dir() -> PathBuf {
    let p = SHGetKnownFolderPath(&FOLDERID_Desktop, KF_FLAG_DEFAULT, None).unwrap();
    let s = p.to_string().unwrap();
    CoTaskMemFree(Some(p.0 as *const _));
    PathBuf::from(s)
}

unsafe fn find_item(view: &IFolderView2, prefix: &str) -> Option<i32> {
    let count = view.ItemCount(SVGIO_ALLVIEW).ok()?;
    for i in 0..count {
        let pidl = view.Item(i).ok()?;
        let name = SHCreateItemFromIDList::<IShellItem>(pidl as *const ITEMIDLIST)
            .ok()
            .and_then(|it| it.GetDisplayName(SIGDN_NORMALDISPLAY).ok())
            .map(|n| {
                let s = n.to_string().unwrap_or_default();
                CoTaskMemFree(Some(n.0 as *const _));
                s
            })
            .unwrap_or_default();
        CoTaskMemFree(Some(pidl as *const _));
        if name.starts_with(prefix) {
            return Some(i);
        }
    }
    None
}

fn wait_item(view: &IFolderView2, prefix: &str, present: bool, timeout: Duration) -> Option<Duration> {
    let t = Instant::now();
    while t.elapsed() < timeout {
        if unsafe { find_item(view, prefix) }.is_some() == present {
            return Some(t.elapsed());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

fn pump(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

// ---------------------------------------------------------------- Part A: off-screen parking

fn part_a_offscreen() -> R<()> {
    println!("\n== Q3 (cont.)  Can an item be parked off-screen? (temp item only)");
    let view = unsafe { desktop_view() }?;
    let dir = unsafe { desktop_dir() };
    let name = format!("desqu-spike2-{}.txt", std::process::id());
    let path = dir.join(&name);
    std::fs::write(&path, b"temporary file created by the Desqu desktop spike; safe to delete").unwrap();
    let Some(took) = wait_item(&view, "desqu-spike2-", true, Duration::from_secs(10)) else {
        println!("temp item never appeared; skipping");
        let _ = std::fs::remove_file(&path);
        return Ok(());
    };
    println!("(temp item appeared after {} ms)", took.as_millis());

    unsafe {
        let virt = (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        );
        println!("virtual screen: x {} y {} w {} h {}", virt.0, virt.1, virt.2, virt.3);

        let idx = find_item(&view, "desqu-spike2-").unwrap();
        let pidl = view.Item(idx)?;
        let original = view.GetItemPosition(pidl)?;
        println!("original position: ({}, {})", original.x, original.y);

        for (label, target) in [
            ("far right/below the virtual screen", POINT { x: virt.0 + virt.2 + 6000, y: virt.1 + virt.3 + 6000 }),
            ("negative (left/above the virtual screen)", POINT { x: virt.0 - 6000, y: virt.1 - 6000 }),
            ("very far (30000, 30000)", POINT { x: 30000, y: 30000 }),
        ] {
            let items = [pidl as *const ITEMIDLIST];
            let pts = [target];
            let r = view.SelectAndPositionItems(1, items.as_ptr(), Some(pts.as_ptr()), SVSI_POSITIONITEM.0 as u32);
            std::thread::sleep(Duration::from_millis(400));
            let idx2 = find_item(&view, "desqu-spike2-").unwrap_or(idx);
            let p2 = view.Item(idx2)?;
            let now = view.GetItemPosition(p2).unwrap_or_default();
            CoTaskMemFree(Some(p2 as *const _));
            let on_screen = now.x >= virt.0 && now.x < virt.0 + virt.2 && now.y >= virt.1 && now.y < virt.1 + virt.3;
            println!(
                "{label}: asked ({}, {}) -> call ok={} -> reads back ({}, {})  => {}",
                target.x,
                target.y,
                r.is_ok(),
                now.x,
                now.y,
                if on_screen { "ended up ON screen (clamped/snapped)" } else { "stayed OFF screen" }
            );
        }
        // Put it back before deleting.
        let items = [pidl as *const ITEMIDLIST];
        let pts = [original];
        let _ = view.SelectAndPositionItems(1, items.as_ptr(), Some(pts.as_ptr()), SVSI_POSITIONITEM.0 as u32);
        CoTaskMemFree(Some(pidl as *const _));
    }
    let _ = std::fs::remove_file(&path);
    let gone = wait_item(&view, "desqu-spike2-", false, Duration::from_secs(10));
    println!("temp file removed from the view: {}", gone.is_some());
    Ok(())
}

// ---------------------------------------------------------------- Part B: change notifications

unsafe extern "system" fn notify_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn part_b_notifications() -> R<()> {
    println!("\n== Q6  Change notifications (event-driven only)");
    let dir = unsafe { desktop_dir() };

    // ReadDirectoryChangesW on its own thread; reports the time of the first FILE_NAME change.
    let (tx, rx) = mpsc::channel::<Duration>();
    let watch_dir = dir.clone();
    let started = Instant::now();
    let _watcher = std::thread::spawn(move || unsafe {
        let wide: Vec<u16> = watch_dir.to_string_lossy().encode_utf16().chain(Some(0)).collect();
        let handle = CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_LIST_DIRECTORY.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            None,
        )
        .unwrap();
        let mut buf = vec![0u8; 8192];
        let mut returned = 0u32;
        let _ = ReadDirectoryChangesW(handle, buf.as_mut_ptr().cast(), buf.len() as u32, false, FILE_NOTIFY_CHANGE_FILE_NAME, Some(&mut returned), None, None);
        let _ = tx.send(started.elapsed());
    });

    // SHChangeNotifyRegister with a message window on this thread.
    unsafe {
        let hinst = GetModuleHandleW(None)?;
        let class = w!("DesquSpikeNotify");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(notify_proc),
            hInstance: hinst.into(),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(Default::default(), class, w!("spike"), Default::default(), 0, 0, 0, 0, None, None, Some(hinst.into()), None)?;
        let desktop_pidl = SHGetKnownFolderIDList(&FOLDERID_Desktop, 0, None)?;
        let entry = SHChangeNotifyEntry { pidl: desktop_pidl, fRecursive: BOOL(0) };
        // SHCNRF_ShellLevel (0x2) | SHCNRF_InterruptLevel (0x1) | SHCNRF_NewDelivery (0x8000); all events
        let id = SHChangeNotifyRegister(hwnd, windows::Win32::UI::Shell::SHCNRF_ShellLevel | windows::Win32::UI::Shell::SHCNRF_InterruptLevel | windows::Win32::UI::Shell::SHCNRF_NewDelivery, 0x7FFFFFFF, WM_SHELL_CHANGE, 1, &entry);
        println!("SHChangeNotifyRegister id = {id} (0 = failed)");

        let temp = dir.join(format!("desqu-spike3-{}.txt", std::process::id()));
        let t0 = Instant::now();
        std::fs::write(&temp, b"temporary; safe to delete").unwrap();
        let mut shell_latency = None;
        let end = Instant::now() + Duration::from_secs(8);
        // Poll the message queue only to receive the event; this is the spike's way of waiting.
        while Instant::now() < end && shell_latency.is_none() {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_SHELL_CHANGE {
                    shell_latency = Some(t0.elapsed());
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let dir_latency = rx.recv_timeout(Duration::from_secs(2)).ok().map(|t| t.saturating_sub(t0.duration_since(started)));
        let _ = std::fs::remove_file(&temp);
        println!(
            "file created -> ReadDirectoryChangesW reported after {} ms; SHChangeNotify message after {} ms",
            dir_latency.map_or("never".into(), |d| d.as_millis().to_string()),
            shell_latency.map_or("never".into(), |d| d.as_millis().to_string())
        );
        let _ = SHChangeNotifyDeregister(id);
        CoTaskMemFree(Some(desktop_pidl as *const _));
        let _ = DestroyWindow(hwnd);
    }
    Ok(())
}

// ---------------------------------------------------------------- Part C: window placement

unsafe fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = GetClassNameW(hwnd, &mut buf);
    String::from_utf16_lossy(&buf[..n as usize])
}

unsafe extern "system" fn print_child(hwnd: HWND, _l: LPARAM) -> BOOL {
    println!("      child {:?} class={}", hwnd.0, class_of(hwnd));
    BOOL(1)
}

unsafe extern "system" fn print_top(hwnd: HWND, _l: LPARAM) -> BOOL {
    let class = class_of(hwnd);
    if class == "WorkerW" || class == "Progman" {
        println!("  top-level {:?} class={} visible={}", hwnd.0, class, IsWindowVisible(hwnd).as_bool());
        let _ = EnumChildWindows(Some(hwnd), Some(print_child), LPARAM(0));
    }
    BOOL(1)
}

unsafe extern "system" fn swatch_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe fn make_swatch(rect: RECT, parent: Option<HWND>) -> R<HWND> {
    let hinst = GetModuleHandleW(None)?;
    let class = w!("DesquSpikeSwatch");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(swatch_proc),
        hInstance: hinst.into(),
        lpszClassName: class,
        hbrBackground: windows::Win32::Graphics::Gdi::CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x00FF00FF)), // magenta (BGR)
        ..Default::default()
    };
    RegisterClassW(&wc);
    let style = if parent.is_some() { WS_CHILD | WS_VISIBLE } else { WS_POPUP };
    CreateWindowExW(
        WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        class,
        w!("vial-spike"),
        style,
        rect.left,
        rect.top,
        rect.right - rect.left,
        rect.bottom - rect.top,
        parent,
        None,
        Some(hinst.into()),
        None,
    )
}

unsafe fn top_at(x: i32, y: i32) -> String {
    let h = WindowFromPoint(POINT { x, y });
    if h.0.is_null() {
        "none".into()
    } else {
        format!("{} {:?}", class_of(h), h.0)
    }
}

/// Finds a w x h rect whose corners and centre show only desktop windows (Progman, its list view,
/// WorkerW), i.e. no application window covers it.
unsafe fn find_bare_rect(w: i32, h: i32) -> Option<RECT> {
    let is_desktop = |x: i32, y: i32| {
        let hwnd = WindowFromPoint(POINT { x, y });
        !hwnd.0.is_null() && matches!(class_of(hwnd).as_str(), "Progman" | "SysListView32" | "WorkerW" | "SHELLDLL_DefView")
    };
    let left0 = GetSystemMetrics(SM_XVIRTUALSCREEN);
    let top0 = GetSystemMetrics(SM_YVIRTUALSCREEN);
    let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
    let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
    let mut y = top0 + 40;
    while y + h < top0 + height - 80 {
        let mut x = left0 + 40;
        while x + w < left0 + width - 40 {
            let r = RECT { left: x, top: y, right: x + w, bottom: y + h };
            let pts = [(x + 5, y + 5), (x + w - 5, y + 5), (x + 5, y + h - 5), (x + w - 5, y + h - 5), (x + w / 2, y + h / 2)];
            if pts.iter().all(|(px, py)| is_desktop(*px, *py)) {
                return Some(r);
            }
            x += 100;
        }
        y += 100;
    }
    None
}

unsafe fn pixel(x: i32, y: i32) -> Option<u32> {
    let dc = GetDC(None);
    let c = GetPixel(dc, x, y);
    ReleaseDC(None, dc);
    (c.0 != CLR_INVALID).then_some(c.0)
}

/// Samples a few points inside the rect; reports how many are magenta (0x00FF00FF, BGR).
unsafe fn magenta_samples(r: &RECT) -> String {
    let pts = [
        ((r.left + r.right) / 2, (r.top + r.bottom) / 2),
        (r.left + 20, r.top + 20),
        (r.right - 20, r.bottom - 20),
        (r.left + 20, r.bottom - 20),
    ];
    let mut hits = 0;
    let mut seen = Vec::new();
    for (x, y) in pts {
        let c = pixel(x, y);
        if c == Some(0x00FF00FF) {
            hits += 1;
        }
        seen.push(c.map_or("invalid".to_string(), |v| format!("{v:06X}")));
    }
    format!("{hits}/4 magenta (colours seen: {})", seen.join(", "))
}

unsafe fn describe(h: HWND) -> String {
    let mut r = RECT::default();
    let _ = GetWindowRect(h, &mut r);
    format!(
        "hwnd {:?} visible={} rect=({},{})-({},{}) parent={:?}",
        h.0,
        IsWindowVisible(h).as_bool(),
        r.left,
        r.top,
        r.right,
        r.bottom,
        GetParent(h).ok().map(|p| p.0)
    )
}

fn part_c_windows() -> R<()> {
    println!("\n== Q4  Where can a vial window sit relative to the icons?");
    unsafe {
        println!("desktop window hierarchy:");
        let _ = EnumWindows(Some(print_top), LPARAM(0));
        let progman = FindWindowW(w!("Progman"), PCWSTR::null())?;
        println!("Progman = {:?}", progman.0);

        let Some(rect) = find_bare_rect(300, 200) else {
            println!("no 300x200 patch of bare desktop found (application windows cover it); cannot test placement");
            return Ok(());
        };
        println!("testing on a bare desktop patch ({},{})-({},{})", rect.left, rect.top, rect.right, rect.bottom);
        let (cx, cy) = ((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2);
        println!("before: top-most window at the centre = {}", top_at(cx, cy));

        // 1. Bottom-most normal window.
        let fg_before = GetForegroundWindow();
        let a = make_swatch(rect, None)?;
        let _ = SetWindowPos(a, Some(HWND_BOTTOM), rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top, SWP_NOACTIVATE | SWP_SHOWWINDOW);
        let _ = ShowWindow(a, SW_SHOWNOACTIVATE);
        let _ = windows::Win32::Graphics::Gdi::UpdateWindow(a);
        pump(800);
        let fg_after = GetForegroundWindow();
        println!("1) bottom-most popup: {}", describe(a));
        println!(
            "   top-most window at the centre now = {}  (our swatch = {:?}); pixels: {}; foreground window unchanged = {}",
            top_at(cx, cy),
            a.0,
            magenta_samples(&rect),
            fg_before == fg_after
        );
        let mut chain = Vec::new();
        let mut cur = GetWindow(a, GW_HWNDNEXT).ok();
        for _ in 0..4 {
            if let Some(h) = cur {
                chain.push(class_of(h));
                cur = GetWindow(h, GW_HWNDNEXT).ok();
            }
        }
        println!("   windows below it in z-order (first 4 classes): {chain:?}");

        // Control: the same colour at a spot the swatch does not cover must NOT be magenta.
        let outside = pixel(rect.right + 200, rect.bottom + 200);
        println!("   control pixel outside the swatch = {:?} (must not be FF00FF)", outside.map(|v| format!("{v:06X}")));
        let _ = DestroyWindow(a);
        pump(300);

        // 2. A true child window of the host of SHELLDLL_DefView (above the icons), via 0x052C.
        let mut result = 0usize;
        let _ = SendMessageTimeoutW(progman, 0x052C, WPARAM(0xD), LPARAM(0x1), SMTO_NORMAL, 1000, Some(&mut result));
        pump(300);
        struct Ctx(Option<HWND>);
        unsafe extern "system" fn find_host(hwnd: HWND, l: LPARAM) -> BOOL {
            let ctx = &mut *(l.0 as *mut Ctx);
            let defview = windows::Win32::UI::WindowsAndMessaging::FindWindowExW(Some(hwnd), None, w!("SHELLDLL_DefView"), PCWSTR::null());
            if defview.is_ok() {
                ctx.0 = Some(hwnd);
                return BOOL(0);
            }
            BOOL(1)
        }
        let mut ctx = Ctx(None);
        let _ = EnumWindows(Some(find_host), LPARAM(&mut ctx as *mut Ctx as isize));
        match ctx.0 {
            Some(host) => {
                println!("SHELLDLL_DefView is hosted by {:?} (class {})", host.0, class_of(host));
                let mut host_rect = RECT::default();
                let _ = GetWindowRect(host, &mut host_rect);
                let local = RECT {
                    left: rect.left - host_rect.left,
                    top: rect.top - host_rect.top,
                    right: rect.right - host_rect.left,
                    bottom: rect.bottom - host_rect.top,
                };
                match make_swatch(local, Some(host)) {
                    Ok(b) => {
                        let _ = SetWindowPos(b, Some(HWND_TOP), local.left, local.top, local.right - local.left, local.bottom - local.top, SWP_NOACTIVATE | SWP_SHOWWINDOW);
                        let _ = windows::Win32::Graphics::Gdi::UpdateWindow(b);
                        pump(800);
                        println!("2) true child of the DefView host, HWND_TOP: {}", describe(b));
                        println!(
                            "   top-most window at the centre now = {}  (our swatch = {:?}); pixels: {}",
                            top_at(cx, cy),
                            b.0,
                            magenta_samples(&rect)
                        );
                        let _ = DestroyWindow(b);
                    }
                    Err(e) => println!("2) could not create a child window of the host: {e}"),
                }
            }
            None => println!("2) could not find the SHELLDLL_DefView host window"),
        }
        pump(300);
        let _ = PostMessageW(None, WM_USER, WPARAM(0), LPARAM(0));
    }
    Ok(())
}


// ---------------------------------------------------------------- Part D: parking via the list view

const LVM_SETITEMPOSITION: u32 = 0x1000 + 15;

unsafe fn desktop_listview() -> Option<HWND> {
    let progman = FindWindowW(w!("Progman"), PCWSTR::null()).ok()?;
    let defview = windows::Win32::UI::WindowsAndMessaging::FindWindowExW(Some(progman), None, w!("SHELLDLL_DefView"), PCWSTR::null()).ok()?;
    windows::Win32::UI::WindowsAndMessaging::FindWindowExW(Some(defview), None, w!("SysListView32"), PCWSTR::null()).ok()
}

fn part_d_listview_park() -> R<()> {
    println!("
== Q3 (cont.)  Parking an item with LVM_SETITEMPOSITION on the desktop list view (temp item only)");
    let view = unsafe { desktop_view() }?;
    let dir = unsafe { desktop_dir() };
    let name = format!("desqu-spike4-{}.txt", std::process::id());
    let path = dir.join(&name);
    std::fs::write(&path, b"temporary file created by the Desqu desktop spike; safe to delete").unwrap();
    if wait_item(&view, "desqu-spike4-", true, Duration::from_secs(10)).is_none() {
        println!("temp item never appeared; skipping");
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }
    unsafe {
        let Some(lv) = desktop_listview() else {
            println!("could not find the desktop SysListView32");
            let _ = std::fs::remove_file(&path);
            return Ok(());
        };
        let idx = find_item(&view, "desqu-spike4-").unwrap();
        let pidl = view.Item(idx)?;
        let original = view.GetItemPosition(pidl)?;
        CoTaskMemFree(Some(pidl as *const _));
        println!("list view {:?}; temp item index {idx}; original position ({}, {})", lv.0, original.x, original.y);

        let pack = |x: i32, y: i32| LPARAM((((y as i16 as u16 as u32) << 16) | (x as i16 as u16 as u32)) as isize);
        for (label, x, y) in [("far negative (-3000, -3000)", -3000, -3000), ("far negative (-20000, -20000)", -20000, -20000)] {
            let mut ret = 0usize;
            let ok = SendMessageTimeoutW(lv, LVM_SETITEMPOSITION, WPARAM(idx as usize), pack(x, y), SMTO_NORMAL, 2000, Some(&mut ret));
            std::thread::sleep(Duration::from_millis(400));
            let idx2 = find_item(&view, "desqu-spike4-").unwrap_or(idx);
            let p2 = view.Item(idx2)?;
            let now = view.GetItemPosition(p2).unwrap_or_default();
            CoTaskMemFree(Some(p2 as *const _));
            println!(
                "{label}: message ok={} (result {ret}) -> reads back ({}, {})  => {}",
                ok.0 != 0,
                now.x,
                now.y,
                if now.x < 0 || now.y < 0 { "PARKED off-screen (negative position kept)" } else { "not parked (clamped to the visible grid)" }
            );
        }
        // Restore, then confirm it is back on the grid.
        let mut ret = 0usize;
        let _ = SendMessageTimeoutW(lv, LVM_SETITEMPOSITION, WPARAM(idx as usize), pack(original.x, original.y), SMTO_NORMAL, 2000, Some(&mut ret));
        std::thread::sleep(Duration::from_millis(400));
        let idx3 = find_item(&view, "desqu-spike4-").unwrap_or(idx);
        let p3 = view.Item(idx3)?;
        let back = view.GetItemPosition(p3).unwrap_or_default();
        CoTaskMemFree(Some(p3 as *const _));
        println!("restored to ({}, {}) -> reads back ({}, {})", original.x, original.y, back.x, back.y);
    }
    let _ = std::fs::remove_file(&path);
    let gone = wait_item(&view, "desqu-spike4-", false, Duration::from_secs(10));
    println!("temp file removed from the view: {}", gone.is_some());
    Ok(())
}

fn main() -> R<()> {
    println!("Desqu desktop spike (part 2)");
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    }
    // `spike_desktop_2 [a|b|c]` runs one part; no argument runs all of them.
    let which = std::env::args().nth(1).unwrap_or_default();
    if which.is_empty() || which == "a" {
        part_a_offscreen()?;
    }
    if which.is_empty() || which == "b" {
        part_b_notifications()?;
    }
    if which.is_empty() || which == "c" {
        part_c_windows()?;
    }
    if which.is_empty() || which == "d" {
        part_d_listview_park()?;
    }
    let _ = (GetMessageW, CW_USEDEFAULT);
    Ok(())
}
