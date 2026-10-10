//! Helper for the crash-recovery test: opens the layer, hides one item, reports where Explorer
//! now says it is, then sleeps until the test kills this process.
//!
//! Usage: `hide_and_hang <journal path> <item id>`
#![cfg(windows)]

use std::time::Duration;

fn main() {
    let mut args = std::env::args().skip(1);
    let journal = args.next().expect("journal path");
    let id = args.next().expect("item id");

    let layer = desqu_desktop::open("crash-test", std::path::Path::new(&journal)).expect("open the layer");
    layer.hide(std::slice::from_ref(&id)).expect("hide");
    let item = layer.items().expect("items").into_iter().find(|i| i.id == id).expect("item in the view");
    println!("HIDDEN {} {}", item.position.x, item.position.y);
    // Flush so the parent sees it before it kills us.
    use std::io::Write;
    let _ = std::io::stdout().flush();

    std::thread::sleep(Duration::from_secs(300));
    drop(layer);
}
