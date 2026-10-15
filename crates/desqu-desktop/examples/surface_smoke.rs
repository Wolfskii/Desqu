//! End-to-end check of the layer on the real desktop: hide and show one item the native way (the
//! hidden attribute), then the same in "surface" mode with the desktop's own icons switched off.
//!
//! SAFETY: the only item touched is a temporary file this program creates and deletes on the
//! desktop. Its hidden attribute is cleared again, and the desktop's icons (off for a few seconds in
//! the second part) are always switched back on, also when a check fails. Needs the layer lock, so
//! quit Desqu/Vials/Workspaces first.
//!
//! Run: `cargo run -p desqu-desktop --example surface_smoke`
#![cfg(windows)]

use std::time::{Duration, Instant};

fn wait_for(what: &str, timeout: Duration, mut f: impl FnMut() -> bool) -> bool {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    eprintln!("FAIL: timed out waiting for {what}");
    false
}

fn main() {
    let desktop = std::path::PathBuf::from(std::env::var("USERPROFILE").expect("USERPROFILE")).join("Desktop");
    let id = desktop.join(format!("desqu-surface-smoke-{}.txt", std::process::id()));
    let journal = std::env::temp_dir().join(format!("desqu-surface-smoke-{}.json", std::process::id()));
    let id_str = id.to_string_lossy().to_string();
    std::fs::write(&id, "temporary file created by the surface smoke test; safe to delete").expect("create the temp file");

    let mut ok = true;
    let layer = desqu_desktop::open("smoke", &journal).expect("open the layer (is Desqu running?)");
    let was_visible = desqu_desktop::icons_visible().expect("icons_visible");
    println!("desktop icons visible at the start: {was_visible}");

    ok &= wait_for("the temp file in the desktop view", Duration::from_secs(20), || {
        layer.items().map(|items| items.iter().any(|i| i.id == id_str)).unwrap_or(false)
    });

    if ok {
        // The native way: the file gets the hidden attribute and leaves the desktop view.
        let report = layer.hide(std::slice::from_ref(&id_str)).expect("hide");
        println!("native hide -> hidden {}, missing {}, failed {}", report.hidden.len(), report.missing.len(), report.failed.len());
        ok &= report.hidden == [id_str.clone()];
        ok &= wait_for("the item to leave the desktop view", Duration::from_secs(10), || {
            desqu_desktop::item_position(&id_str).ok().flatten().is_none()
        });
        let attr = std::fs::metadata(&id).map(|m| {
            use std::os::windows::fs::MetadataExt;
            m.file_attributes() & 0x2 != 0
        });
        println!("file has the hidden attribute: {attr:?}");
        ok &= attr.unwrap_or(false);
        let listed = layer.items().expect("items").iter().any(|i| i.id == id_str && i.parked);
        println!("still listed to the owners, flagged hidden: {listed}");
        ok &= listed && layer.hidden().expect("hidden") == [id_str.clone()];

        layer.show(std::slice::from_ref(&id_str)).expect("show");
        ok &= wait_for("the item to come back", Duration::from_secs(10), || {
            desqu_desktop::item_position(&id_str).ok().flatten().is_some()
        });
        let attr = std::fs::metadata(&id).map(|m| {
            use std::os::windows::fs::MetadataExt;
            m.file_attributes() & 0x2 != 0
        });
        let cleared = attr.map(|h| !h).unwrap_or(false);
        println!("hidden attribute cleared again: {cleared}");
        ok &= cleared;
    }

    if ok {
        layer.set_surface(true).expect("set_surface(true)");
        let off = !desqu_desktop::icons_visible().unwrap_or(true);
        println!("surface on: {}; desktop's own icons off: {off}", layer.surface_active());
        ok &= layer.surface_active() && off;

        let listed = layer.items().expect("items while the icons are off");
        println!("items listed while the icons are off: {}", listed.len());
        ok &= listed.iter().any(|i| i.id == id_str);

        let report = layer.hide(std::slice::from_ref(&id_str)).expect("hide");
        println!("hide -> hidden {:?}, missing {:?}, failed {:?}", report.hidden.len(), report.missing.len(), report.failed.len());
        ok &= report.hidden == [id_str.clone()];
        let flagged = layer.items().expect("items").iter().any(|i| i.id == id_str && i.parked);
        println!("item reported as hidden: {flagged}; claimed by this owner: {:?}", layer.hidden().expect("hidden").len());
        ok &= flagged && layer.hidden().expect("hidden") == [id_str.clone()];

        layer.show(std::slice::from_ref(&id_str)).expect("show");
        let shown = layer.items().expect("items").iter().any(|i| i.id == id_str && !i.parked);
        println!("item shown again: {shown}");
        ok &= shown;

        // A claim made now must survive switching back to the old way only as far as Windows allows.
        layer.set_surface(false).expect("set_surface(false)");
        let back = desqu_desktop::icons_visible().unwrap_or(false);
        println!("surface off: {}; desktop's own icons back: {back}", !layer.surface_active());
        ok &= !layer.surface_active() && back == was_visible;
    }

    // Whatever happened, leave the desktop as it was.
    drop(layer);
    if desqu_desktop::icons_visible().unwrap_or(true) != was_visible {
        eprintln!("FAIL: the desktop's icons were not restored");
        ok = false;
    }
    let _ = std::fs::remove_file(&id);
    let _ = std::fs::remove_file(&journal);
    println!("{}", if ok { "PASS" } else { "FAIL" });
    std::process::exit(if ok { 0 } else { 1 });
}
