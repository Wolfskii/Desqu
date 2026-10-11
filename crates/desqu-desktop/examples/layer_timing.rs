//! Diagnostic: how long does Explorer take to accept position writes on this desktop? Creates ONE
//! temporary file on the desktop, waits for it to show up, writes its own position back to it a
//! number of times (it does not move), prints the cost per write and deletes the file.
//!
//! Run: `cargo run -p desqu-desktop --example layer_timing`
#![cfg(windows)]

use std::time::{Duration, Instant};

fn main() {
    let desktop = std::path::PathBuf::from(std::env::var("USERPROFILE").expect("USERPROFILE")).join("Desktop");
    let file = desktop.join(format!("desqu-layer-timing-{}.txt", std::process::id()));
    std::fs::write(&file, "temporary file for a timing test; safe to delete").expect("create the temp file");
    let id = file.to_string_lossy().into_owned();

    let end = Instant::now() + Duration::from_secs(30);
    let result = loop {
        match desqu_desktop::nudge_timing(&id, 100) {
            Ok(d) => break Ok(d),
            Err(e) if Instant::now() < end => {
                let _ = e;
                std::thread::sleep(Duration::from_millis(500));
            }
            Err(e) => break Err(e),
        }
    };
    let _ = std::fs::remove_file(&file);
    match result {
        Ok(d) => println!("100 position writes took {} ms ({:.2} ms each)", d.as_millis(), d.as_secs_f64() * 10.0),
        Err(e) => println!("failed: {e}"),
    }
}
