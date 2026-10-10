//! Workspaces: tie desktop items (shortcuts, files, folders) to a virtual desktop. An owned item is
//! shown on its desktop and hidden on the others via the shared `desqu-desktop` layer (always
//! restored). Only the virtual desktop registry layout is used; no window is touched.

mod commands;
pub mod core;
pub mod engine;
mod platform;
pub mod plugin;

pub use plugin::{init, ModuleOptions, WorkspacesModule};

/// Version of this module crate (kept in step with the Workspaces app version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
