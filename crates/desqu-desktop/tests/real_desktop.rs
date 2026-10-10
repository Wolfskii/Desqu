//! Real-desktop test. It creates its own temporary files on the desktop and only ever hides,
//! restores and deletes those; no other item is touched. The desktop view lags the file system by
//! several seconds, so this takes about half a minute. Run with
//! `cargo test -p desqu-desktop --test real_desktop -- --ignored --nocapture`.
#![cfg(windows)]

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use desqu_desktop::{is_parked, DesktopEvent, DesktopLayer, Journal};

fn desktop() -> PathBuf {
    PathBuf::from(std::env::var("USERPROFILE").expect("USERPROFILE")).join("Desktop")
}

fn wait_for<T>(timeout: Duration, mut check: impl FnMut() -> Option<T>) -> Option<T> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Some(v) = check() {
            return Some(v);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    None
}

fn find(layer: &dyn DesktopLayer, id: &str) -> Option<desqu_desktop::DesktopItem> {
    layer.items().ok()?.into_iter().find(|i| i.id == id)
}

struct TempFile(PathBuf);

impl TempFile {
    fn create(name: &str) -> Self {
        let path = desktop().join(format!("{name}-{}.txt", std::process::id()));
        std::fs::write(&path, b"temporary file created by the desqu-desktop test; safe to delete").unwrap();
        Self(path)
    }
    fn id(&self) -> String {
        self.0.to_string_lossy().into_owned()
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
#[ignore = "requires an interactive Windows desktop"]
fn hide_share_restore_icons_events_and_recovery() {
    let journal_dir = std::env::temp_dir().join(format!("desqu-desktop-real-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&journal_dir);
    let journal = journal_dir.join("desktop-journal.json");

    let vials = desqu_desktop::open("vials", &journal).expect("open the desktop layer");
    // A second module in the same process shares the layer instead of fighting for the lock.
    let workspaces = desqu_desktop::open("workspaces", &journal).expect("same process shares the layer");

    assert!(vials.can_hide().unwrap(), "this test needs auto-arrange OFF on the desktop");

    // ---- events: a new file must be reported (event-driven, no polling on our side)
    let (tx, rx) = mpsc::channel();
    let subscription = vials
        .subscribe(Box::new(move |e| {
            let _ = tx.send(e);
        }))
        .unwrap();

    let file = TempFile::create("desqu-layer-test");
    let id = file.id();
    assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok(DesktopEvent::Changed), "no change event for a new file");

    // ---- the view lags the file system: wait until the item shows up
    let item = wait_for(Duration::from_secs(20), || find(vials.as_ref(), &id)).expect("item never appeared in the view");
    println!("item appeared at ({}, {}), kind {:?}", item.position.x, item.position.y, item.kind);
    assert!(!item.parked);
    let original = item.position;

    // ---- icons
    let png = vials.icon_png(&id, 32).expect("icon");
    assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);

    // ---- hide: Explorer's own view must report it parked, and the journal must exist
    let report = vials.hide(&[id.clone(), "C:\\no\\such\\item.txt".into()]).unwrap();
    assert_eq!(report.hidden, std::slice::from_ref(&id));
    assert_eq!(report.missing.len(), 1);
    let hidden = find(vials.as_ref(), &id).unwrap();
    assert!(is_parked(hidden.position), "view says ({}, {})", hidden.position.x, hidden.position.y);
    assert!(hidden.parked);
    assert_eq!(vials.hidden().unwrap(), std::slice::from_ref(&id));
    let j = Journal::load(&journal);
    assert_eq!(j.items.len(), 1);
    assert_eq!(j.items[0].original, original, "the journal must hold the real original position");

    // ---- two owners share the item: it stays hidden until the last one lets go
    workspaces.hide(std::slice::from_ref(&id)).unwrap();
    vials.show(std::slice::from_ref(&id)).unwrap();
    assert!(is_parked(find(vials.as_ref(), &id).unwrap().position), "workspaces still hides it");
    assert!(vials.hidden().unwrap().is_empty());
    assert_eq!(workspaces.hidden().unwrap(), std::slice::from_ref(&id));

    // ---- crash recovery of a foreign journal: restores and deletes it
    let foreign = journal_dir.join("foreign-journal.json");
    std::fs::copy(&journal, &foreign).unwrap();
    let restored = desqu_desktop::recover_from(&foreign).unwrap();
    assert_eq!(restored, 1);
    assert!(!foreign.exists(), "a recovered journal is deleted");
    let after_recover = find(vials.as_ref(), &id).unwrap();
    assert!(!is_parked(after_recover.position), "recovery should have moved it back");

    // ---- last owner lets go: the item is visible and the journal is gone
    workspaces.show_all().unwrap();
    let back = find(vials.as_ref(), &id).unwrap();
    assert!(!is_parked(back.position));
    assert!(!back.parked);
    assert!(!journal.exists(), "an empty journal must not leave a file");

    // ---- dropping the handles restores anything still hidden
    vials.hide(std::slice::from_ref(&id)).unwrap();
    assert!(is_parked(find(vials.as_ref(), &id).unwrap().position));
    drop(subscription);
    drop(vials);
    drop(workspaces);
    let fresh = desqu_desktop::open("check", &journal).unwrap();
    let final_state = wait_for(Duration::from_secs(10), || {
        let i = find(fresh.as_ref(), &id)?;
        (!is_parked(i.position)).then_some(i)
    });
    assert!(final_state.is_some(), "dropping the last handle must put the item back");

    drop(file);
    let _ = std::fs::remove_dir_all(&journal_dir);
}

/// A process that hid an item is killed without any chance to clean up (the equivalent of
/// `kill -9` / a crash / a power cut). The next process to open the layer must find the item
/// parked, restore it from the journal, and the lock must be free again.
#[test]
#[ignore = "requires an interactive Windows desktop"]
fn kill_while_hidden_then_the_next_start_restores_the_item() {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};

    let build = Command::new(env!("CARGO"))
        .args(["build", "-p", "desqu-desktop", "--example", "hide_and_hang"])
        .status()
        .expect("run cargo build");
    assert!(build.success());
    let exe = std::env::current_exe()
        .unwrap()
        .parent()
        .and_then(|deps| deps.parent())
        .unwrap()
        .join("examples")
        .join("hide_and_hang.exe");
    assert!(exe.exists(), "{} not built", exe.display());

    let journal_dir = std::env::temp_dir().join(format!("desqu-desktop-kill-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&journal_dir);
    let journal = journal_dir.join("desktop-journal.json");

    let file = TempFile::create("desqu-layer-kill");
    let id = file.id();

    // Wait for the item to be in the view before the child hides it (the view lags several seconds).
    {
        let probe = desqu_desktop::open("probe", &journal_dir.join("probe.json")).unwrap();
        wait_for(Duration::from_secs(20), || find(probe.as_ref(), &id)).expect("item never appeared in the view");
        // Dropping the probe releases the cross-process lock for the child.
    }

    let mut child = Command::new(&exe)
        .arg(&journal)
        .arg(&id)
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn the helper");
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
    println!("child said: {}", line.trim());
    let mut parts = line.split_whitespace();
    assert_eq!(parts.next(), Some("HIDDEN"), "helper failed: {line:?}");
    let x: i32 = parts.next().unwrap().parse().unwrap();
    let y: i32 = parts.next().unwrap().parse().unwrap();
    assert!(is_parked(desqu_desktop::Point { x, y }), "the child reported ({x}, {y}), expected parked");
    assert!(journal.exists(), "the journal must exist while an item is hidden");

    // kill -9: no destructors, no restore.
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(journal.exists(), "the journal survives the crash");

    // The next start: the lock is free again, startup recovery restores the item.
    let next = wait_for(Duration::from_secs(10), || desqu_desktop::open("next", &journal).ok())
        .expect("the layer lock was not released after the crash");
    let restored = wait_for(Duration::from_secs(10), || {
        let item = find(next.as_ref(), &id)?;
        (!is_parked(item.position)).then_some(item)
    });
    let restored = restored.expect("the item was not restored after the crash");
    println!("restored to ({}, {})", restored.position.x, restored.position.y);
    assert!(!journal.exists(), "recovery must clear the journal");

    drop(next);
    drop(file);
    let _ = std::fs::remove_dir_all(&journal_dir);
}
