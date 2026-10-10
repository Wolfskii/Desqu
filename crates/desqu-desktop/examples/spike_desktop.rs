//! P8.1 spike, part 1: what can we observe about the desktop icon layer on this machine?
//!
//! SAFETY: this only ever *moves or hides one temporary file that it creates itself* on the
//! desktop (`desqu-spike-<pid>.txt`) and deletes it again. It reads, but never changes, the
//! user's own items.
//!
//! Run: `cargo run -p desqu-desktop --example spike_desktop`
#![cfg(windows)]

use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use windows::core::{Interface, GUID, PCWSTR};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::Foundation::POINT;
use windows::Win32::Storage::FileSystem::{SetFileAttributesW, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_NORMAL};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, IServiceProvider, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::HiDpi::{
    GetDpiForSystem, GetThreadDpiAwarenessContext, SetProcessDpiAwarenessContext,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_PublicDesktop, IFolderView2, IShellBrowser, IShellItem, IShellItemImageFactory,
    IShellWindows, SHCreateItemFromIDList, SHGetKnownFolderPath, ShellWindows, KF_FLAG_DEFAULT, SIGDN_NORMALDISPLAY,
    SIIGBF_ICONONLY, SVGIO_ALLVIEW, SVSI_POSITIONITEM, SWC_DESKTOP, SWFO_NEEDDISPATCH,
};
use windows::Win32::UI::WindowsAndMessaging::RegisterWindowMessageW;

type R<T> = windows::core::Result<T>;

fn known_folder(id: &GUID) -> Option<PathBuf> {
    unsafe {
        let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s.map(PathBuf::from)
    }
}

fn q1_folders() -> (Option<PathBuf>, Option<PathBuf>) {
    println!("\n== Q1  Desktop folders");
    let user = known_folder(&FOLDERID_Desktop);
    let public = known_folder(&FOLDERID_PublicDesktop);
    for (label, path) in [("FOLDERID_Desktop", &user), ("FOLDERID_PublicDesktop", &public)] {
        match path {
            Some(p) => {
                let entries: Vec<_> = std::fs::read_dir(p).map(|d| d.flatten().collect()).unwrap_or_default();
                let hidden = entries
                    .iter()
                    .filter(|e| e.metadata().map(|m| m.file_attributes() & 0x2 != 0).unwrap_or(false))
                    .count();
                println!(
                    "{label}: {}  ({} entries, {} with the hidden attribute, OneDrive redirected: {})",
                    p.display(),
                    entries.len(),
                    hidden,
                    p.to_string_lossy().to_lowercase().contains("onedrive")
                );
            }
            None => println!("{label}: <unavailable>"),
        }
    }
    (user, public)
}

unsafe fn find_desktop_view() -> R<IFolderView2> {
    let windows: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_ALL)?;
    let location = VARIANT::from(0i32); // CSIDL_DESKTOP
    let empty = VARIANT::default();
    let mut hwnd = 0i32;
    let dispatch = windows.FindWindowSW(&location, &empty, SWC_DESKTOP, &mut hwnd, SWFO_NEEDDISPATCH)?;
    let provider: IServiceProvider = dispatch.cast()?;
    // SID_STopLevelBrowser
    let sid = GUID::from_u128(0x4c96be40_915c_11cf_99d3_00aa004ae837);
    let browser: IShellBrowser = provider.QueryService(&sid)?;
    let view = browser.QueryActiveShellView()?;
    view.cast::<IFolderView2>()
}

unsafe fn item_name(pidl: *const ITEMIDLIST) -> String {
    match SHCreateItemFromIDList::<IShellItem>(pidl) {
        Ok(item) => match item.GetDisplayName(SIGDN_NORMALDISPLAY) {
            Ok(n) => {
                let s = n.to_string().unwrap_or_default();
                CoTaskMemFree(Some(n.0 as *const _));
                s
            }
            Err(_) => "<no name>".into(),
        },
        Err(_) => "<no item>".into(),
    }
}

unsafe fn items(view: &IFolderView2) -> R<Vec<(String, POINT)>> {
    let count = view.ItemCount(SVGIO_ALLVIEW)?;
    let mut out = Vec::new();
    for i in 0..count {
        let pidl = view.Item(i)?;
        let name = item_name(pidl);
        let pos = view.GetItemPosition(pidl).unwrap_or_default();
        CoTaskMemFree(Some(pidl as *const _));
        out.push((name, pos));
    }
    Ok(out)
}

fn flags_text(flags: u32) -> String {
    let known = [
        (0x1, "AUTOARRANGE"),
        (0x2, "ABBREVIATEDNAMES"),
        (0x4, "SNAPTOGRID"),
        (0x8, "OWNERDATA"),
        (0x10, "BESTFITWINDOW"),
        (0x20, "DESKTOP"),
        (0x40, "SINGLESEL"),
        (0x80, "NOSUBFOLDERS"),
        (0x100, "TRANSPARENT"),
        (0x200, "NOCLIENTEDGE"),
        (0x400, "NOSCROLL"),
        (0x800, "ALIGNLEFT"),
        (0x1000, "NOICONS"),
        (0x2000, "SHOWSELALWAYS"),
        (0x4000, "NOVISIBLE"),
        (0x8000, "SINGLECLICKACTIVATE"),
        (0x10000, "NOWEBVIEW"),
        (0x20000, "HIDEFILENAMES"),
        (0x40000, "CHECKSELECT"),
    ];
    let names: Vec<_> = known.iter().filter(|(bit, _)| flags & bit != 0).map(|(_, n)| *n).collect();
    format!("0x{flags:X} [{}]", names.join("|"))
}

fn wait_for_count(view: &IFolderView2, want: i32, timeout: Duration) -> (bool, Duration) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        let n = unsafe { view.ItemCount(SVGIO_ALLVIEW) }.unwrap_or(-1);
        if n == want {
            return (true, start.elapsed());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    (false, start.elapsed())
}

fn q2_and_q3(desktop: &Path) -> R<()> {
    println!("\n== Q2  Desktop IFolderView (read positions, move one temp item)");
    let view = unsafe { find_desktop_view() }?;
    let flags = unsafe { view.GetCurrentFolderFlags() }?;
    let mode = unsafe { view.GetCurrentViewMode() }?;
    println!("view mode: {mode}  folder flags: {}", flags_text(flags));
    println!(
        "auto arrange: {}   snap to grid (align to grid): {}",
        flags & 0x1 != 0,
        flags & 0x4 != 0
    );

    let before = unsafe { items(&view) }?;
    println!("desktop items shown: {}", before.len());
    for (name, pos) in before.iter().take(5) {
        println!("  sample: {:<40.40} at ({}, {})", name, pos.x, pos.y);
    }
    if before.len() > 5 {
        println!("  ... {} more (names not printed)", before.len() - 5);
    }
    let (min_x, max_x) = before.iter().fold((i32::MAX, i32::MIN), |(lo, hi), (_, p)| (lo.min(p.x), hi.max(p.x)));
    let (min_y, max_y) = before.iter().fold((i32::MAX, i32::MIN), |(lo, hi), (_, p)| (lo.min(p.y), hi.max(p.y)));
    println!("position range: x {min_x}..{max_x}, y {min_y}..{max_y}");

    // ---- temp item
    let temp_name = format!("desqu-spike-{}.txt", std::process::id());
    let temp_path = desktop.join(&temp_name);
    std::fs::write(&temp_path, b"temporary file created by the Desqu desktop spike; safe to delete")
        .expect("create temp file");
    let want = before.len() as i32 + 1;
    let (appeared, took) = wait_for_count(&view, want, Duration::from_secs(5));
    println!("temp item appeared in the view: {appeared} (after {} ms)", took.as_millis());
    if !appeared {
        let _ = std::fs::remove_file(&temp_path);
        return Ok(());
    }

    unsafe {
        let all = view.ItemCount(SVGIO_ALLVIEW)?;
        let mut idx = None;
        for i in 0..all {
            let pidl = view.Item(i)?;
            let name = item_name(pidl);
            CoTaskMemFree(Some(pidl as *const _));
            if name.starts_with("desqu-spike-") {
                idx = Some(i);
                break;
            }
        }
        let idx = idx.expect("temp item index");
        let pidl = view.Item(idx)?;
        let original = view.GetItemPosition(pidl)?;
        println!("temp item original position: ({}, {})", original.x, original.y);

        // Move it somewhere clearly different (never touching other items).
        let target = POINT { x: original.x + 300, y: original.y + 200 };
        let pidls = [pidl as *const ITEMIDLIST];
        let points = [target];
        let result = view.SelectAndPositionItems(1, pidls.as_ptr(), Some(points.as_ptr()), SVSI_POSITIONITEM.0 as u32);
        std::thread::sleep(Duration::from_millis(300));
        let pidl2 = view.Item(idx)?;
        let after = view.GetItemPosition(pidl2).unwrap_or_default();
        println!(
            "SelectAndPositionItems -> {:?}; wanted ({}, {}), now ({}, {})  => position write {}",
            result.is_ok(),
            target.x,
            target.y,
            after.x,
            after.y,
            if (after.x - target.x).abs() <= 80 && (after.y - target.y).abs() <= 80 {
                "WORKS"
            } else if flags & 0x1 != 0 {
                "ignored (auto arrange is on)"
            } else {
                "did not move as asked"
            }
        );
        CoTaskMemFree(Some(pidl2 as *const _));
        CoTaskMemFree(Some(pidl as *const _));
    }

    println!("\n== Q3  Hiding options (tested on the temp item only)");
    let wide: Vec<u16> = temp_path.to_string_lossy().encode_utf16().chain(Some(0)).collect();
    unsafe { SetFileAttributesW(PCWSTR(wide.as_ptr()), FILE_ATTRIBUTE_HIDDEN) }?;
    let (hidden_gone, took) = wait_for_count(&view, before.len() as i32, Duration::from_secs(5));
    println!(
        "FILE_ATTRIBUTE_HIDDEN: item {} from the view (after {} ms). This modifies the file's attributes on disk.",
        if hidden_gone { "DISAPPEARED" } else { "stayed visible" },
        took.as_millis()
    );
    let _ = unsafe { SetFileAttributesW(PCWSTR(wide.as_ptr()), FILE_ATTRIBUTE_NORMAL) };
    let _ = std::fs::remove_file(&temp_path);
    let (gone, _) = wait_for_count(&view, before.len() as i32, Duration::from_secs(5));
    println!("temp file deleted and removed from the view: {gone}");
    Ok(())
}

fn q5_icons() -> R<()> {
    println!("\n== Q5  Icon extraction cost (IShellItemImageFactory, 48x48 icon only)");
    let view = unsafe { find_desktop_view() }?;
    let count = unsafe { view.ItemCount(SVGIO_ALLVIEW) }?;
    if count == 0 {
        println!("no items to measure");
        return Ok(());
    }
    let mut pidls = Vec::new();
    for i in 0..count {
        pidls.push(unsafe { view.Item(i) }?);
    }
    let total = 200usize;
    let start = Instant::now();
    let mut ok = 0;
    let mut first = Duration::ZERO;
    for n in 0..total {
        let pidl = pidls[n % pidls.len()];
        let t = Instant::now();
        let got = unsafe {
            SHCreateItemFromIDList::<IShellItem>(pidl)
                .and_then(|item| item.cast::<IShellItemImageFactory>())
                .and_then(|factory| factory.GetImage(windows::Win32::Foundation::SIZE { cx: 48, cy: 48 }, SIIGBF_ICONONLY))
        };
        if n == 0 {
            first = t.elapsed();
        }
        if let Ok(bitmap) = got {
            ok += 1;
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::DeleteObject(bitmap.into());
            }
        }
    }
    let elapsed = start.elapsed();
    println!(
        "{ok}/{total} icons in {} ms (first {} ms, ~{:.1} ms each; {} distinct items cycled)",
        elapsed.as_millis(),
        first.as_millis(),
        elapsed.as_secs_f64() * 1000.0 / total as f64,
        pidls.len()
    );
    for pidl in pidls {
        unsafe { CoTaskMemFree(Some(pidl as *const _)) };
    }
    Ok(())
}

fn q7_dpi() {
    println!("\n== Q7  Per-monitor DPI");
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        println!(
            "process DPI awareness context set to per-monitor v2; thread context = {:?}; system DPI = {}",
            GetThreadDpiAwarenessContext().0,
            GetDpiForSystem()
        );
    }
    println!("(positions above are what IFolderView reports to a per-monitor-v2 caller; compare with monitor bounds in the research doc)");
}

fn q8_taskbar_created() {
    println!("\n== Q8  Explorer restart detection");
    let name: Vec<u16> = "TaskbarCreated".encode_utf16().chain(Some(0)).collect();
    let id = unsafe { RegisterWindowMessageW(PCWSTR(name.as_ptr())) };
    println!("RegisterWindowMessage(\"TaskbarCreated\") = 0x{id:X} (non-zero = usable). Not triggered: restarting Explorer would disturb the session.");
}

fn main() -> R<()> {
    println!("Desqu desktop spike (part 1)");
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()? };
    q7_dpi();
    let (user, _public) = q1_folders();
    if let Some(desktop) = user {
        q2_and_q3(&desktop)?;
    }
    q5_icons()?;
    q8_taskbar_created();
    Ok(())
}
