//! Contract between the shell and a utility module (Screenbound, Vials, Workspaces).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

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

/// Modules registered by the app, so shell-level actions (updates) can stop and restart them.
pub struct ModuleRegistry<R: Runtime> {
    modules: Mutex<Vec<Arc<dyn Module<R>>>>,
}

impl<R: Runtime> Default for ModuleRegistry<R> {
    fn default() -> Self {
        Self { modules: Mutex::new(Vec::new()) }
    }
}

impl<R: Runtime> ModuleRegistry<R> {
    fn all(&self) -> Vec<Arc<dyn Module<R>>> {
        self.modules.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

pub fn register<R: Runtime>(app: &AppHandle<R>, module: impl Module<R> + 'static) {
    if let Some(registry) = app.try_state::<ModuleRegistry<R>>() {
        registry
            .modules
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Arc::new(module));
    }
}

/// Stops every module that is not already off; returns their ids so they can be started again.
pub fn stop_running<R: Runtime>(app: &AppHandle<R>, timeout: Duration) -> Vec<&'static str> {
    let Some(registry) = app.try_state::<ModuleRegistry<R>>() else {
        return Vec::new();
    };
    let mut stopped = Vec::new();
    for module in registry.all() {
        if module.status(app) != ModuleStatus::Off {
            module.stop(app, timeout);
            stopped.push(module.id());
        }
    }
    stopped
}

pub fn start_ids<R: Runtime>(app: &AppHandle<R>, ids: &[&'static str]) {
    let Some(registry) = app.try_state::<ModuleRegistry<R>>() else {
        return;
    };
    for module in registry.all() {
        if ids.contains(&module.id()) {
            if let Err(e) = module.start(app) {
                tracing::warn!("could not restart {}: {e}", module.id());
            }
        }
    }
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
