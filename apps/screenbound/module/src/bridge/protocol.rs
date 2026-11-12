//! Wire protocol between the browser extension (through the native host) and the engine.
//! JSON inside `desqu_ipc::frame` frames. Mirrored by `apps/browser-extension/src/shared/messages.ts`
//! and the fixtures in `apps/browser-extension/test-fixtures/protocol/`. Change all of them together.

use serde::{Deserialize, Serialize};

use crate::core::browser::SiteRules;
use crate::core::geometry::Rect;

pub const PROTOCOL_VERSION: u32 = 1;

/// Sent by the native host first, on every pipe connection.
/// Sent by the extension after the host reports the pipe is up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ClientMessage {
    #[serde(rename_all = "camelCase")]
    HostHello {
        browser_pid: u32,
        browser_process_name: String,
        browser_exe_path: String,
        host_version: String,
    },
    #[serde(rename_all = "camelCase")]
    Hello {
        id: String,
        protocol: u32,
        extension_version: String,
        browser: BrowserInfo,
        rules: SiteRules,
    },
    #[serde(rename_all = "camelCase")]
    SetRules { id: String, rules: SiteRules },
    #[serde(rename_all = "camelCase")]
    ExemptNext { id: String, tab_title: String },
    #[serde(rename_all = "camelCase")]
    ZoneEnter {
        id: String,
        tab_id: i64,
        tab_title: String,
        zone_id: Option<String>,
        /// Page viewport in physical screen pixels, when the page can measure it.
        viewport: Option<Rect>,
    },
    #[serde(rename_all = "camelCase")]
    ViewportReport {
        id: String,
        tab_id: i64,
        viewport: Rect,
    },
    #[serde(rename_all = "camelCase")]
    ZoneExit { id: String, tab_id: i64 },
    #[serde(rename_all = "camelCase")]
    FullscreenNative { tab_id: i64, host: String },
}

/// Sent by the engine to the extension. Replies carry `replyTo`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ServerMessage {
    /// Host-generated: the pipe to Desqu went up or down.
    BridgeStatus { connected: bool },
    #[serde(rename_all = "camelCase")]
    HelloAck {
        reply_to: String,
        protocol: u32,
        product: Product,
        app_version: String,
        zones: Vec<ZoneRef>,
        rules: SiteRules,
    },
    #[serde(rename_all = "camelCase")]
    Ack { reply_to: String },
    #[serde(rename_all = "camelCase")]
    ZoneApplied {
        reply_to: String,
        ok: bool,
        zone_name: Option<String>,
        error: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Error {
        reply_to: String,
        code: ErrorCode,
        message: String,
    },
    RulesChanged { rules: SiteRules },
    ZonesChanged { zones: Vec<ZoneRef> },
    #[serde(rename_all = "camelCase")]
    ZoneReleased { tab_id: i64, reason: ReleaseReason },
    #[serde(rename_all = "camelCase")]
    RequestViewport { tab_id: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Product {
    Desqu,
    Screenbound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    UnsupportedProtocol,
    BadRequest,
    WindowNotFound,
    ZoneUnavailable,
    Disabled,
    DesquUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReleaseReason {
    UserMoved,
    WindowGone,
    Released,
    Disabled,
}

#[cfg(test)]
mod tests {
    use super::*;
    use desqu_ipc::frame::{read_frame, write_frame};
    use serde_json::Value;

    // The same files the extension's tests read. Paths are relative to this file.
    const CLIENT: &[(&str, &str)] = &[
        ("hostHello", include_str!("../../../../browser-extension/test-fixtures/protocol/host-hello.json")),
        ("hello", include_str!("../../../../browser-extension/test-fixtures/protocol/hello.json")),
        ("zoneEnter", include_str!("../../../../browser-extension/test-fixtures/protocol/zone-enter.json")),
    ];
    const SERVER: &[(&str, &str)] = &[
        ("helloAck", include_str!("../../../../browser-extension/test-fixtures/protocol/hello-ack.json")),
        ("zoneApplied", include_str!("../../../../browser-extension/test-fixtures/protocol/zone-applied.json")),
        ("error", include_str!("../../../../browser-extension/test-fixtures/protocol/error.json")),
        ("zoneReleased", include_str!("../../../../browser-extension/test-fixtures/protocol/zone-released.json")),
        ("bridgeStatus", include_str!("../../../../browser-extension/test-fixtures/protocol/bridge-status.json")),
    ];

    #[test]
    fn every_client_fixture_round_trips_to_the_same_json() {
        for (name, text) in CLIENT {
            let json: Value = serde_json::from_str(text).unwrap();
            let msg: ClientMessage = serde_json::from_value(json.clone())
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(serde_json::to_value(&msg).unwrap(), json, "{name}");
        }
    }

    #[test]
    fn every_server_fixture_round_trips_to_the_same_json() {
        for (name, text) in SERVER {
            let json: Value = serde_json::from_str(text).unwrap();
            let msg: ServerMessage = serde_json::from_value(json.clone())
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(serde_json::to_value(&msg).unwrap(), json, "{name}");
        }
    }

    #[test]
    fn unknown_message_types_are_rejected() {
        assert!(serde_json::from_str::<ClientMessage>(r#"{"type":"nope"}"#).is_err());
        assert!(serde_json::from_str::<ServerMessage>(r#"{"type":"ack"}"#).is_err());
    }

    #[test]
    fn error_codes_use_the_names_the_extension_expects() {
        let json = serde_json::to_string(&ErrorCode::DesquUnavailable).unwrap();
        assert_eq!(json, r#""desquUnavailable""#);
        assert_eq!(serde_json::to_string(&ReleaseReason::UserMoved).unwrap(), r#""userMoved""#);
    }

    #[test]
    fn messages_survive_the_frame_layer() {
        let (_, hello) = CLIENT[1];
        let mut wire = Vec::new();
        write_frame(&mut wire, hello.as_bytes()).unwrap();
        let payload = read_frame(&mut std::io::Cursor::new(wire)).unwrap().unwrap();
        let msg: ClientMessage = serde_json::from_slice(&payload).unwrap();
        assert!(matches!(msg, ClientMessage::Hello { protocol: 1, .. }));
    }
}
