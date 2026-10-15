//! How fast does the desktop react when a file gets (or loses) the hidden attribute and the shell
//! is told with `SHChangeNotify`? Only a temporary file this program creates and deletes is touched.
//!
//! Run: `cargo run -p desqu-desktop --example attr_probe`
#![cfg(windows)]

use std::time::{Duration, Instant};

use windows::core::HSTRING;
use windows::Win32::Storage::FileSystem::{
    GetFileAttributesW, SetFileAttributesW, FILE_ATTRIBUTE_HIDDEN, FILE_FLAGS_AND_ATTRIBUTES,
};
use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_ATTRIBUTES, SHCNF_FLUSH, SHCNF_PATHW};

fn wait(present: bool, id: &str) -> Option<Duration> {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(15) {
        let here = desqu_desktop::item_position(id).ok().flatten().is_some();
        if here == present {
            return Some(started.elapsed());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    None
}

fn set_hidden(path: &str, hidden: bool) {
    let wide = HSTRING::from(path);
    unsafe {
        let attrs = GetFileAttributesW(&wide);
        let next = if hidden { attrs | FILE_ATTRIBUTE_HIDDEN.0 } else { attrs & !FILE_ATTRIBUTE_HIDDEN.0 };
        SetFileAttributesW(&wide, FILE_FLAGS_AND_ATTRIBUTES(next)).expect("SetFileAttributesW");
        SHChangeNotify(SHCNE_ATTRIBUTES, SHCNF_PATHW | SHCNF_FLUSH, Some(wide.as_ptr().cast()), None);
    }
}

fn main() {
    let desktop = std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap()).join("Desktop");
    let path = desktop.join(format!("desqu-attr-probe-{}.txt", std::process::id()));
    let id = path.to_string_lossy().to_string();
    std::fs::write(&path, "temporary file created by attr_probe; safe to delete").unwrap();
    println!("appeared after {:?}", wait(true, &id));
    let before = desqu_desktop::item_position(&id).ok().flatten();
    println!("position before: {before:?}");
    set_hidden(&id, true);
    println!("hidden: gone from the view after {:?}", wait(false, &id));
    set_hidden(&id, false);
    println!("unhidden: back in the view after {:?}", wait(true, &id));
    println!("position after: {:?}", desqu_desktop::item_position(&id).ok().flatten());
    let _ = std::fs::remove_file(&path);
}
