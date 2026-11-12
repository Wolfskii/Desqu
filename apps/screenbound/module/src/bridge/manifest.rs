//! Native messaging host manifests: what each browser reads to find the host and check who may use
//! it. Pure text; writing the files and the registry entries is the platform's job.

use serde_json::{json, Value};

/// Name the extension passes to `connectNative`. Must match the registry keys.
pub const HOST_NAME: &str = "io.github.wolfskii.desqu.browser";
/// Firefox add-on id (`browser_specific_settings.gecko.id` in the extension).
pub const FIREFOX_EXTENSION_ID: &str = "desqu-fullscreen@wolfskii.github.io";

/// The repo's single list of extension ids. Shared with the extension build.
const EXTENSION_IDS: &str = include_str!("../../../../browser-extension/extension-ids.json");

/// `chrome-extension://<id>/` for every Chromium id we ship: dev build and each store.
pub fn chromium_origins() -> Vec<String> {
    chromium_origins_from(EXTENSION_IDS)
}

fn chromium_origins_from(ids_json: &str) -> Vec<String> {
    let Ok(ids) = serde_json::from_str::<Value>(ids_json) else {
        return Vec::new();
    };
    let Some(chromium) = ids.get("chromium").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut origins: Vec<String> = chromium
        .values()
        .filter_map(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .map(|id| format!("chrome-extension://{id}/"))
        .collect();
    origins.sort();
    origins.dedup();
    origins
}

/// Manifest for Chromium-based browsers (Chrome, Edge, Brave, Vivaldi, Opera, Chromium, Arc).
pub fn chromium_manifest(host_exe: &str, origins: &[String]) -> String {
    pretty(&json!({
        "name": HOST_NAME,
        "description": "Desqu Fullscreen: relays the browser extension to Desqu or Screenbound",
        "path": host_exe,
        "type": "stdio",
        "allowed_origins": origins,
    }))
}

/// Manifest for Firefox-based browsers (Firefox, LibreWolf, Waterfox, Floorp).
pub fn firefox_manifest(host_exe: &str) -> String {
    pretty(&json!({
        "name": HOST_NAME,
        "description": "Desqu Fullscreen: relays the browser extension to Desqu or Screenbound",
        "path": host_exe,
        "type": "stdio",
        "allowed_extensions": [FIREFOX_EXTENSION_ID],
    }))
}

fn pretty(value: &Value) -> String {
    let mut text = serde_json::to_string_pretty(value).expect("json value serializes");
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins_come_from_every_non_empty_chromium_id() {
        let ids = r#"{"chromium":{"dev":"aaa","chromeWebStore":null,"edgeAddons":"bbb","opera":""},
                     "firefox":{"gecko":"x@y"}}"#;
        assert_eq!(
            chromium_origins_from(ids),
            vec!["chrome-extension://aaa/".to_string(), "chrome-extension://bbb/".to_string()]
        );
    }

    #[test]
    fn the_shipped_id_file_yields_the_dev_origin() {
        let origins = chromium_origins();
        assert!(
            origins.iter().any(|o| o == "chrome-extension://fpihmjjagmmhdoplgijjablfnhlfpjkm/"),
            "the dev id from extension-ids.json must be allowed: {origins:?}"
        );
    }

    #[test]
    fn a_broken_id_file_allows_nothing_rather_than_everything() {
        assert!(chromium_origins_from("not json").is_empty());
        assert!(chromium_origins_from(r#"{"firefox":{}}"#).is_empty());
    }

    #[test]
    fn chromium_manifest_names_the_host_and_its_origins() {
        let text = chromium_manifest(r"C:\Desqu\desqu-browser-host.exe", &["chrome-extension://aaa/".into()]);
        let json: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["name"], HOST_NAME);
        assert_eq!(json["path"], r"C:\Desqu\desqu-browser-host.exe");
        assert_eq!(json["type"], "stdio");
        assert_eq!(json["allowed_origins"][0], "chrome-extension://aaa/");
        assert!(json.get("allowed_extensions").is_none());
    }

    #[test]
    fn firefox_manifest_allows_only_our_add_on() {
        let text = firefox_manifest(r"C:\Desqu\desqu-browser-host.exe");
        let json: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["allowed_extensions"], json!([FIREFOX_EXTENSION_ID]));
        assert!(json.get("allowed_origins").is_none());
    }
}
