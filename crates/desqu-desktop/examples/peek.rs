//! Prints where the desktop shows an item (`x y`) or `none`. Read-only, takes no lock, so it works
//! while a Desqu app owns the layer. Used by the smoke tests.
//!
//! Usage: `peek <item id>`
#![cfg(windows)]

fn main() {
    let id = std::env::args().nth(1).expect("item id");
    match desqu_desktop::item_position(&id) {
        Ok(Some(p)) => println!("{} {}", p.x, p.y),
        Ok(None) => println!("none"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    }
}
