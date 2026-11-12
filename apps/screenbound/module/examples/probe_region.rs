//! Spike for browser zone mode (PLAN.md step P10.C1). Run it with a browser window in front:
//!
//!   cargo run -p screenbound-module --example probe_region
//!
//! It waits 4 s (focus the browser), then prints the window rect, the measured page rect, and the
//! window rect after a 60 px top clip. It lifts the clip after 3 s. Record the output in
//! apps/screenbound/docs/fullscreen-research.md, together with the browser and its version.

use std::time::Duration;

use screenbound_module::core::geometry::Rect;
use screenbound_module::platform;

fn main() {
    let platform = platform::create();
    println!("focus a browser window now...");
    std::thread::sleep(Duration::from_secs(4));

    let Some(window) = platform.foreground_window() else {
        println!("no foreground window");
        return;
    };
    let identity = platform.identity(window).map(|i| i.process_name).unwrap_or_default();
    let state = match platform.state(window) {
        Ok(s) => s,
        Err(e) => {
            println!("could not read the window: {e}");
            return;
        }
    };
    println!("window {window} ({identity}): bounds {} visible {} show {:?}", state.bounds, state.visible_bounds, state.show_state);

    let page = platform.largest_child_rect(window);
    println!("largest child (page?) rect: {page:?}");
    if let Some(page) = page {
        let inset_top = page.top - state.bounds.top;
        println!("page starts {inset_top} px below the window top (tabs and toolbar)");
    }

    let clip = Rect::from_xywh(0, 60, state.bounds.width(), state.bounds.height() - 60);
    match platform.set_region(window, Some(clip)) {
        Ok(()) => println!("set_region accepted: clip {clip}"),
        Err(e) => println!("set_region refused: {e}"),
    }
    std::thread::sleep(Duration::from_secs(3));
    match platform.state(window) {
        Ok(after) => println!("after 3 s: bounds {} (should be unchanged)", after.bounds),
        Err(e) => println!("could not re-read: {e}"),
    }
    let _ = platform.set_region(window, None);
    println!("clip lifted");
}
