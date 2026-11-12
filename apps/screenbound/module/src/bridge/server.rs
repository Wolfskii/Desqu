//! Accepts native-host connections on the bridge pipe and hands their messages to the engine.
//!
//! One accept thread, then one reader thread and one writer thread per connection. The reader
//! forwards requests to a `BridgeSink`. The sink keeps the `Sender` it is given and uses it to push
//! replies and unsolicited messages back to that browser.

use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use desqu_ipc::frame::{read_frame, write_frame};
use desqu_ipc::pipe::{connect, Pipe, PipeListener, BRIDGE_PIPE_NAME};

use super::protocol::{ClientMessage, ServerMessage};

/// One native-host connection (one browser process).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConnId(pub u64);

/// Receives everything the server sees. Called from connection threads, so it must be cheap and
/// thread-safe (the engine implementation posts the call onto the engine thread).
pub trait BridgeSink: Send + Sync + 'static {
    /// The host introduced itself with `hostHello`. Keep `outbox` to send messages to this host.
    fn connected(&self, id: ConnId, outbox: Sender<ServerMessage>);
    fn message(&self, id: ConnId, message: ClientMessage);
    /// The connection is gone. Drop the `outbox` you kept for it.
    fn disconnected(&self, id: ConnId);
}

#[derive(Default)]
struct Shared {
    clients: Mutex<HashMap<ConnId, Pipe>>,
    stopping: AtomicBool,
}

/// Runs for as long as it is alive. Dropping it closes every connection and stops accepting.
pub struct BridgeServer {
    name: String,
    shared: Arc<Shared>,
    accept: Option<JoinHandle<()>>,
}

impl BridgeServer {
    /// Serves the real bridge pipe. Fails if another process already serves it.
    pub fn start(sink: Arc<dyn BridgeSink>) -> io::Result<Self> {
        Self::start_at(BRIDGE_PIPE_NAME, sink)
    }

    pub fn start_at(name: &str, sink: Arc<dyn BridgeSink>) -> io::Result<Self> {
        let mut listener = PipeListener::bind(name)?;
        let shared = Arc::new(Shared::default());
        let accept_shared = shared.clone();
        let accept = std::thread::Builder::new()
            .name("screenbound-bridge".into())
            .spawn(move || {
                let mut next_id = 0u64;
                loop {
                    let pipe = match listener.accept() {
                        Ok(pipe) => pipe,
                        Err(e) => {
                            tracing::warn!("bridge accept failed: {e}");
                            break;
                        }
                    };
                    // The wake-up connection from `drop` lands here too.
                    if accept_shared.stopping.load(Ordering::SeqCst) {
                        break;
                    }
                    next_id += 1;
                    let id = ConnId(next_id);
                    accept_shared
                        .clients
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(id, pipe.clone());
                    let shared = accept_shared.clone();
                    let sink = sink.clone();
                    let spawned = std::thread::Builder::new()
                        .name(format!("screenbound-bridge-{next_id}"))
                        .spawn(move || serve(id, pipe, shared, sink));
                    if let Err(e) = spawned {
                        tracing::warn!("bridge connection thread failed: {e}");
                    }
                }
            })?;
        Ok(Self {
            name: name.to_string(),
            shared,
            accept: Some(accept),
        })
    }
}

impl Drop for BridgeServer {
    fn drop(&mut self) {
        self.shared.stopping.store(true, Ordering::SeqCst);
        for pipe in self
            .shared
            .clients
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
        {
            pipe.close();
        }
        // Wake the accept thread so it sees `stopping` and exits.
        let _ = connect(&self.name);
        if let Some(handle) = self.accept.take() {
            let _ = handle.join();
        }
    }
}

/// Serves one host connection until it disconnects.
fn serve(id: ConnId, pipe: Pipe, shared: Arc<Shared>, sink: Arc<dyn BridgeSink>) {
    let mut reader = pipe.clone();
    // The first message must be `hostHello`. Anything else means this is not our host.
    let hello = match read_message(&mut reader) {
        Some(message @ ClientMessage::HostHello { .. }) => message,
        _ => {
            tracing::warn!(conn = id.0, "refusing a bridge connection without hostHello");
            pipe.close();
            return;
        }
    };
    tracing::info!(conn = id.0, "browser host connected");

    let (tx, rx) = mpsc::channel::<ServerMessage>();
    let mut writer = pipe.clone();
    // Detached on purpose: it ends when the sink drops its `Sender`, or when a write fails.
    let _ = std::thread::Builder::new()
        .name(format!("screenbound-bridge-out-{}", id.0))
        .spawn(move || {
            for message in rx {
                let Ok(bytes) = serde_json::to_vec(&message) else {
                    continue;
                };
                if write_frame(&mut writer, &bytes).is_err() {
                    break;
                }
            }
        });

    sink.connected(id, tx);
    sink.message(id, hello);
    while let Some(message) = read_message(&mut reader) {
        sink.message(id, message);
    }
    sink.disconnected(id);

    shared
        .clients
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&id);
    pipe.close();
}

/// Reads frames until one parses. `None` at end of stream or on a read error. A frame that does not
/// parse is logged and skipped, so one bad message does not cut off the browser.
fn read_message(reader: &mut Pipe) -> Option<ClientMessage> {
    loop {
        let bytes = match read_frame(reader) {
            Ok(Some(bytes)) => bytes,
            _ => return None,
        };
        match serde_json::from_slice::<ClientMessage>(&bytes) {
            Ok(message) => return Some(message),
            Err(e) => tracing::warn!("bridge: dropped a message that did not parse: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::protocol::ServerMessage;
    use desqu_ipc::frame::{read_frame, write_frame};
    use desqu_ipc::pipe::{connect, PipeListener};
    use std::sync::atomic::AtomicU32;
    use std::time::{Duration, Instant};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn unique_name() -> String {
        format!(
            r"\\.\pipe\desqu-bridge-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        )
    }

    #[allow(dead_code)] // the tests match on the variant; the ids are for debugging
    enum Event {
        Connected(ConnId, Sender<ServerMessage>),
        Message(ConnId, ClientMessage),
        Disconnected(ConnId),
    }

    #[derive(Default)]
    struct Recorder {
        events: Mutex<Vec<Event>>,
    }

    impl Recorder {
        fn count(&self, pred: impl Fn(&Event) -> bool) -> usize {
            self.events.lock().unwrap().iter().filter(|e| pred(e)).count()
        }
        fn outbox(&self) -> Option<Sender<ServerMessage>> {
            self.events.lock().unwrap().iter().find_map(|e| match e {
                Event::Connected(_, tx) => Some(tx.clone()),
                _ => None,
            })
        }
    }

    impl BridgeSink for Recorder {
        fn connected(&self, id: ConnId, outbox: Sender<ServerMessage>) {
            self.events.lock().unwrap().push(Event::Connected(id, outbox));
        }
        fn message(&self, id: ConnId, message: ClientMessage) {
            self.events.lock().unwrap().push(Event::Message(id, message));
        }
        fn disconnected(&self, id: ConnId) {
            self.events.lock().unwrap().push(Event::Disconnected(id));
        }
    }

    fn wait_for(what: &str, cond: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !cond() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    const HOST_HELLO: &[u8] = br#"{"type":"hostHello","browserPid":1,"browserProcessName":"chrome.exe","browserExePath":"C:\\chrome.exe","hostVersion":"0.1.0"}"#;

    #[test]
    fn host_hello_reaches_the_sink_and_replies_reach_the_host() {
        let name = unique_name();
        let recorder = Arc::new(Recorder::default());
        let server = BridgeServer::start_at(&name, recorder.clone()).unwrap();

        let mut host = connect(&name).unwrap();
        write_frame(&mut host, HOST_HELLO).unwrap();
        wait_for("the connection", || recorder.outbox().is_some());
        recorder.outbox().unwrap().send(ServerMessage::BridgeStatus { connected: true }).unwrap();

        let bytes = read_frame(&mut host).unwrap().unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["type"], "bridgeStatus");
        assert_eq!(json["connected"], true);
        assert_eq!(
            recorder.count(|e| matches!(e, Event::Message(_, ClientMessage::HostHello { browser_pid: 1, .. }))),
            1
        );
        drop(server);
    }

    #[test]
    fn a_connection_that_does_not_start_with_host_hello_is_refused() {
        let name = unique_name();
        let recorder = Arc::new(Recorder::default());
        let server = BridgeServer::start_at(&name, recorder.clone()).unwrap();

        let mut stranger = connect(&name).unwrap();
        write_frame(&mut stranger, br#"{"type":"exemptNext","id":"1","tabTitle":"x"}"#).unwrap();
        // The server closes the pipe, so the read ends without a frame.
        assert!(!matches!(read_frame(&mut stranger), Ok(Some(_))));
        assert_eq!(recorder.count(|e| matches!(e, Event::Connected(..))), 0);
        drop(server);
    }

    #[test]
    fn a_host_that_leaves_is_reported_as_disconnected() {
        let name = unique_name();
        let recorder = Arc::new(Recorder::default());
        let server = BridgeServer::start_at(&name, recorder.clone()).unwrap();

        let mut host = connect(&name).unwrap();
        write_frame(&mut host, HOST_HELLO).unwrap();
        wait_for("the connection", || recorder.outbox().is_some());
        drop(host);
        wait_for("the disconnect", || recorder.count(|e| matches!(e, Event::Disconnected(_))) == 1);
        drop(server);
    }

    #[test]
    fn dropping_the_server_frees_the_pipe_name() {
        let name = unique_name();
        let server = BridgeServer::start_at(&name, Arc::new(Recorder::default())).unwrap();
        drop(server);
        assert!(PipeListener::bind(&name).is_ok(), "the name must be free after the server stops");
    }
}
