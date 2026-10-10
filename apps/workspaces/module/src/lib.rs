//! Workspaces: a different desktop setup per virtual desktop. Each workspace can hide a chosen set
//! of desktop icons while its desktop is current (via the shared `desqu-desktop` layer, always
//! restored) and remembers the apps and folders to launch again. Foreign windows are never moved:
//! Windows does not allow it (see `docs/virtual-desktop-research.md`).

mod commands;
pub mod core;
pub mod engine;
mod platform;
pub mod plugin;

pub use plugin::{init, ModuleOptions, WorkspacesModule};

/// Version of this module crate (kept in step with the Workspaces app version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
