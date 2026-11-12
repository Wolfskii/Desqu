//! Framing for messages between the browser extension, its native host and Desqu.
//!
//! Only byte framing lives here. Message shapes belong to the apps that speak them.

pub mod frame;

#[cfg(windows)]
pub mod pipe;
