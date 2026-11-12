//! End to end: the real host binary, talking stdio to a browser-side test, and a named pipe to a
//! stand-in for Desqu. Uses the real bridge pipe name, so no other Desqu may be running while this
//! test runs.

use std::process::{Command, Stdio};

use desqu_ipc::frame::{read_frame, write_frame};
use desqu_ipc::pipe::{PipeListener, BRIDGE_PIPE_NAME};
use serde_json::{json, Value};

fn json_of(frame: &[u8]) -> Value {
    serde_json::from_slice(frame).expect("the host sends JSON frames")
}

#[test]
fn the_host_relays_requests_to_desqu_and_replies_back_to_the_browser() {
    let mut listener = PipeListener::bind(BRIDGE_PIPE_NAME).expect("the bridge pipe name must be free");

    let mut host = Command::new(env!("CARGO_BIN_EXE_desqu-browser-host"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the host starts");
    let mut to_host = host.stdin.take().unwrap();
    let mut from_host = host.stdout.take().unwrap();

    // Desqu side: the host introduces itself first.
    let mut desqu = listener.accept().expect("the host connects");
    let hello = json_of(&read_frame(&mut desqu).unwrap().unwrap());
    assert_eq!(hello["type"], "hostHello");
    assert_eq!(hello["hostVersion"], env!("CARGO_PKG_VERSION"));

    // Browser side: the extension's hello goes to Desqu unchanged.
    let request = json!({"type": "hello", "id": "1", "protocol": 1, "extensionVersion": "0.1.0",
        "browser": {"name": "Chrome", "version": "130"},
        "rules": {"version": 1, "updatedAt": 0, "defaultMode": "native", "defaultZoneId": null, "rules": []}});
    write_frame(&mut to_host, &serde_json::to_vec(&request).unwrap()).unwrap();
    let relayed = json_of(&read_frame(&mut desqu).unwrap().unwrap());
    assert_eq!(relayed, request);

    // Desqu answers; the host passes the answer to the browser.
    let answer = json!({"type": "helloAck", "replyTo": "1", "protocol": 1, "product": "desqu",
        "appVersion": "0.2.1", "zones": [], "rules": {"version": 1, "updatedAt": 0, "defaultMode": "native", "defaultZoneId": null, "rules": []}});
    write_frame(&mut desqu, &serde_json::to_vec(&answer).unwrap()).unwrap();

    // The browser first hears that the bridge is up, then the answer.
    let status = json_of(&read_frame(&mut from_host).unwrap().unwrap());
    assert_eq!(status, json!({"type": "bridgeStatus", "connected": true}));
    let reply = json_of(&read_frame(&mut from_host).unwrap().unwrap());
    assert_eq!(reply, answer);

    // Closing stdin means the browser is done: the host exits.
    drop(to_host);
    let status = host.wait().expect("the host exits");
    assert!(status.success(), "the host exits cleanly when the browser closes it");
}
