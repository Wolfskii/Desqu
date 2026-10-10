//! Vials: containers on the desktop that show shortcuts, apps and folders. Items stay in the
//! Desktop folders; a vial refers to them and hides them on the desktop (parked off-screen and
//! always restored, via the shared `desqu-desktop` layer).

mod commands;
pub mod core;
pub mod engine;
pub mod plugin;

pub use plugin::{init, ModuleOptions, VialsModule};

/// Version of this module crate (kept in step with the Vials app version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
