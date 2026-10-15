//! Checks that Explorer keeps listing the desktop's items while its own icon layer is switched off
//! (what "surface" mode relies on). It switches the icons off for about half a second and always
//! switches them back on; it reads items and never moves or changes any.
//!
//! Run: `cargo run -p desqu-desktop --example surface_probe`
#![cfg(windows)]

fn main() {
    match desqu_desktop::surface_probe() {
        Ok((before, during, selected, visible_after)) => {
            println!("items before: {before}; while the icons were off: {during}; selected then: {selected}");
            println!("icons visible again afterwards: {visible_after}");
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    }
}
