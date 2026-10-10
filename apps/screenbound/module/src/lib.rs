//! Screenbound as a reusable library: pure core, engine state machine, native platform layer,
//! and a Tauri plugin that exposes it to a UI.

mod commands;
pub mod core;
pub mod engine;
pub mod migrate;
pub mod platform;
pub mod plugin;

pub use plugin::{current_config, init, ModuleOptions, ScreenboundModule};
