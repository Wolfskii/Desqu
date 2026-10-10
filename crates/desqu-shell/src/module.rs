//! Contract between the shell and a utility module (Screenbound, Vials, Workspaces).

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Runtime};

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ModuleStatus {
    /// Nothing running: no threads, no hooks.
    Off,
    /// Running but not acting (e.g. Screenbound with `enabled = false`).
    Paused,
    On,
    /// The OS has no backend for this module.
    Unsupported,
    /// Another process (standalone app or Desqu) holds the module lock.
    OwnedElsewhere,
}

pub trait Module<R: Runtime>: Send + Sync {
    fn id(&self) -> &'static str;
    fn start(&self, app: &AppHandle<R>) -> Result<(), String>;
    /// Restores anything the module changed, then removes its threads and hooks.
    fn stop(&self, app: &AppHandle<R>, timeout: Duration);
    fn status(&self, app: &AppHandle<R>) -> ModuleStatus;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_camel_case_strings() {
        let json = |s| serde_json::to_string(&s).unwrap();
        assert_eq!(json(ModuleStatus::Off), "\"off\"");
        assert_eq!(json(ModuleStatus::OwnedElsewhere), "\"ownedElsewhere\"");
    }
}
