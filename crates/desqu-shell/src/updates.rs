//! In-app updates: check a per-product feed, tell the UI, and install on request.
//!
//! The Windows updater ends the process without guaranteeing `RunEvent::Exit`, so installing
//! first downloads, then stops every running module (restoring managed windows), then installs.
//! If installing fails the modules that were running are started again.

use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::module;
use crate::plugin;

pub const EVENT_UPDATE_AVAILABLE: &str = "desqu://update-available";
/// Where portable builds send the user instead of installing.
pub const RELEASES_URL: &str = "https://github.com/Wolfskii/Desqu/releases";

const FIRST_CHECK_DELAY: Duration = Duration::from_secs(30);
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const STOP_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
    /// Publish date, unix seconds.
    pub date: Option<i64>,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    /// `false` when this build has no updater configuration.
    pub enabled: bool,
    pub available: Option<UpdateInfo>,
    /// Unix seconds of the last completed check.
    pub last_check: Option<i64>,
    pub last_error: Option<String>,
    pub releases_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoAction {
    Nothing,
    Notify,
    Install,
}

/// What a background check does once it found an update. Portable builds can never install.
pub fn auto_action(found: bool, install_automatically: bool, portable: bool) -> AutoAction {
    match (found, install_automatically && !portable) {
        (false, _) => AutoAction::Nothing,
        (true, true) => AutoAction::Install,
        (true, false) => AutoAction::Notify,
    }
}

pub struct UpdateState {
    enabled: bool,
    available: Mutex<Option<(UpdateInfo, Update)>>,
    last_check: Mutex<Option<i64>>,
    last_error: Mutex<Option<String>>,
}

impl UpdateState {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            available: Mutex::new(None),
            last_check: Mutex::new(None),
            last_error: Mutex::new(None),
        }
    }

    pub fn status(&self) -> UpdateStatus {
        UpdateStatus {
            enabled: self.enabled,
            available: lock(&self.available).as_ref().map(|(info, _)| info.clone()),
            last_check: *lock(&self.last_check),
            last_error: lock(&self.last_error).clone(),
            releases_url: RELEASES_URL.into(),
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

pub fn status<R: Runtime>(app: &AppHandle<R>) -> UpdateStatus {
    app.try_state::<UpdateState>()
        .map(|s| s.status())
        .unwrap_or_default()
}

pub async fn check<R: Runtime>(app: &AppHandle<R>) -> Result<Option<UpdateInfo>, String> {
    let state = app
        .try_state::<UpdateState>()
        .ok_or("updates are not available")?;
    if !state.enabled {
        return Err("Updates are not configured for this build".into());
    }
    let result = match app.updater() {
        Ok(updater) => updater.check().await.map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    };
    *lock(&state.last_check) = Some(now_secs());
    match result {
        Ok(Some(update)) => {
            let info = UpdateInfo {
                version: update.version.clone(),
                notes: update.body.clone(),
                date: update.date.map(|d| d.unix_timestamp()),
            };
            *lock(&state.last_error) = None;
            *lock(&state.available) = Some((info.clone(), update));
            let _ = app.emit(EVENT_UPDATE_AVAILABLE, info.clone());
            Ok(Some(info))
        }
        Ok(None) => {
            *lock(&state.last_error) = None;
            *lock(&state.available) = None;
            Ok(None)
        }
        Err(e) => {
            // A missing feed (nothing published yet) is a normal "no update", but keep the reason.
            *lock(&state.last_error) = Some(e.clone());
            Err(e)
        }
    }
}

/// Downloads the update found by [`check`], stops running modules, installs, then restarts.
pub async fn install<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    if plugin::is_portable_build() {
        return Err("This is a portable build: download the new version instead".into());
    }
    let state = app
        .try_state::<UpdateState>()
        .ok_or("updates are not available")?;
    let update = lock(&state.available)
        .as_ref()
        .map(|(_, u)| u.clone())
        .ok_or("No update found yet; check for updates first")?;

    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(|e| format!("download failed: {e}"))?;

    let stopped = module::stop_running(app, STOP_TIMEOUT);
    if let Err(e) = update.install(bytes) {
        module::start_ids(app, &stopped);
        return Err(format!("install failed: {e}"));
    }
    app.restart()
}

/// Background checks: once shortly after startup, then daily, if the user wants them.
/// A daily timer is not window polling, so it does not break the "event-driven only" rule.
pub fn spawn_auto_check<R: Runtime>(app: AppHandle<R>) {
    let _ = std::thread::Builder::new()
        .name("desqu-update-check".into())
        .spawn(move || {
            std::thread::sleep(FIRST_CHECK_DELAY);
            loop {
                let settings = plugin::current(&app);
                if settings.updates.check_automatically {
                    let found = tauri::async_runtime::block_on(check(&app));
                    let action = auto_action(
                        matches!(found, Ok(Some(_))),
                        settings.updates.install_automatically,
                        plugin::is_portable_build(),
                    );
                    if action == AutoAction::Install {
                        if let Err(e) = tauri::async_runtime::block_on(install(&app)) {
                            tracing::warn!("automatic update failed: {e}");
                        }
                    }
                }
                std::thread::sleep(CHECK_INTERVAL);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_found_does_nothing() {
        assert_eq!(auto_action(false, true, false), AutoAction::Nothing);
    }

    #[test]
    fn found_notifies_unless_auto_install_is_on() {
        assert_eq!(auto_action(true, false, false), AutoAction::Notify);
        assert_eq!(auto_action(true, true, false), AutoAction::Install);
    }

    #[test]
    fn portable_builds_only_ever_notify() {
        assert_eq!(auto_action(true, true, true), AutoAction::Notify);
    }

    #[test]
    fn status_serializes_camel_case_and_defaults_to_disabled() {
        let json = serde_json::to_value(UpdateStatus::default()).unwrap();
        assert_eq!(json["enabled"], false);
        assert!(json["lastCheck"].is_null());
        assert!(json["releasesUrl"].is_string());
    }

    #[test]
    fn fresh_state_has_nothing_available() {
        let state = UpdateState::new(true);
        let status = state.status();
        assert!(status.enabled && status.available.is_none() && status.last_check.is_none());
    }
}
