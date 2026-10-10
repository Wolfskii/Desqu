//! Shared shell for Desqu apps (standalone and the collection app): settings, launch arguments,
//! single instance, autostart, tray, window lifecycle and the per-module lock.

pub mod args;
pub mod autostart;
pub mod instance;
pub mod json;
pub mod logging;
pub mod module;
pub mod mutex;
pub mod platform;
pub mod plugin;
pub mod settings;
pub mod tray;
pub mod window;

pub use plugin::{current as current_settings, init};
