//! Screenbound in GPUI: make fullscreen mean fullscreen inside the part of the screen you choose.
//! The app (`main.rs`) and the Desqu window share these views: Desqu shows them on Screenbound's page,
//! talking to a running Screenbound through a [`runtime::Backend`].

pub mod field;
pub mod manage;
pub mod manage_model;
pub mod model;
pub mod runtime;
pub mod view;

use desqu_native_shell::Identity;

/// Who this app is to Windows (names of its mutex, tray window and startup entry are derived from it).
pub const ID: Identity = Identity { name: "Screenbound", key: "ScreenboundGpui" };
/// Where the update manifest lives. Not published yet for this build.
pub const MANIFEST_URL: &str = "https://wolfskii.github.io/Desqu/updates/screenbound-gpui/latest.json";
