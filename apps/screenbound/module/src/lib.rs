//! Screenbound as a reusable library: pure core, engine state machine, native platform layer,
//! and a Tauri plugin that exposes it to a UI.

#[cfg(feature = "tauri-plugin")]
mod commands;
pub mod bridge;
pub mod core;
pub mod engine;
#[cfg(feature = "tauri-plugin")]
pub mod legacy;
pub mod migrate;
pub mod platform;
#[cfg(feature = "tauri-plugin")]
pub mod plugin;

#[cfg(feature = "tauri-plugin")]
pub use plugin::{current_config, init, ModuleOptions, ScreenboundModule};

/// Version of this module crate (kept in step with the Screenbound app version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
