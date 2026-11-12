//! Native messaging host for the Desqu Fullscreen extension.
//!
//! The browser starts this program and talks to it over stdin/stdout (frames from `desqu_ipc`). The
//! host relays every frame to Desqu or Screenbound over a named pipe. Stdout carries only frames;
//! diagnostics go to stderr.
//!
//! If no Desqu is running, the host keeps trying every few seconds and tells the extension its
//! state with `bridgeStatus`. Requests made while Desqu is away get `desquUnavailable`.

// No console window when the browser starts us.
#![windows_subsystem = "windows"]

#[cfg(not(windows))]
compile_error!("desqu-browser-host is Windows only");

use std::collections::HashMap;
use std::io;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use desqu_ipc::frame::{read_frame, write_frame};
use desqu_ipc::pipe::{connect, Pipe, BRIDGE_PIPE_NAME};
use serde_json::{json, Value};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

const HOST_VERSION: &str = env!("CARGO_PKG_VERSION");
/// How long to wait before trying the pipe again while Desqu is not running.
const RETRY_EVERY: Duration = Duration::from_secs(2);
/// Parents we never mistake for the browser: the shells Windows puts in between.
const SKIPPED_PARENTS: &[&str] = &["cmd.exe", "conhost.exe"];
/// How far up the process tree to look for the browser.
const MAX_DEPTH: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
struct BrowserProcess {
    pid: u32,
    name: String,
    path: String,
}

/// Returns the first ancestor that is not a skipped shell.
fn pick_browser(ancestors: &[(u32, String)]) -> Option<(u32, String)> {
    ancestors
        .iter()
        .find(|(_, name)| !SKIPPED_PARENTS.iter().any(|s| name.eq_ignore_ascii_case(s)))
        .cloned()
}

/// `hostHello`: the first frame on every pipe connection.
fn host_hello_frame(browser: &BrowserProcess) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "type": "hostHello",
        "browserPid": browser.pid,
        "browserProcessName": browser.name,
        "browserExePath": browser.path,
        "hostVersion": HOST_VERSION,
    }))
    .expect("json value serializes")
}

/// `bridgeStatus`: sent to the extension whenever the pipe to Desqu goes up or down.
fn status_frame(connected: bool) -> Vec<u8> {
    serde_json::to_vec(&json!({ "type": "bridgeStatus", "connected": connected }))
        .expect("json value serializes")
}

/// The request `id` of a frame from the extension, if it is a request (requests carry an id).
fn request_id(frame: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(frame).ok()?;
    value.get("id")?.as_str().map(str::to_string)
}

/// `error` for a request that cannot reach Desqu.
fn unavailable_frame(request: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "type": "error",
        "replyTo": request,
        "code": "desquUnavailable",
        "message": "Desqu or Screenbound is not running",
    }))
    .expect("json value serializes")
}

fn utf16_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// Parent pid and exe name of every process, from one snapshot.
fn process_table() -> HashMap<u32, (u32, String)> {
    let mut table = HashMap::new();
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return table;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                table.insert(
                    entry.th32ProcessID,
                    (entry.th32ParentProcessID, utf16_to_string(&entry.szExeFile)),
                );
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }
    table
}

fn exe_path(pid: u32) -> String {
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return String::new();
        };
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(handle);
        match ok {
            Ok(()) => String::from_utf16_lossy(&buf[..len as usize]),
            Err(_) => String::new(),
        }
    }
}

/// Walks up from this process to the browser that started it.
fn find_browser() -> Option<BrowserProcess> {
    let table = process_table();
    let mut ancestors = Vec::new();
    let mut pid = std::process::id();
    for _ in 0..MAX_DEPTH {
        let Some((parent, _)) = table.get(&pid) else { break };
        let Some((_, name)) = table.get(parent) else { break };
        ancestors.push((*parent, name.clone()));
        pid = *parent;
    }
    let (pid, name) = pick_browser(&ancestors)?;
    Some(BrowserProcess {
        path: exe_path(pid),
        pid,
        name,
    })
}

enum Inbound {
    /// A frame from the browser.
    Frame(Vec<u8>),
    /// The browser closed our stdin: it is done with us.
    StdinClosed,
    /// The pipe to Desqu ended. Carries the connection generation, so a late report about an old
    /// connection is ignored.
    PipeLost(u64),
}

type Stdout = Arc<Mutex<io::Stdout>>;

fn send_to_browser(stdout: &Stdout, frame: &[u8]) {
    let mut out = stdout.lock().unwrap_or_else(|e| e.into_inner());
    if let Err(e) = write_frame(&mut *out, frame) {
        eprintln!("desqu-browser-host: could not write to the browser: {e}");
    }
}

fn main() {
    let browser = find_browser().unwrap_or(BrowserProcess {
        pid: 0,
        name: String::new(),
        path: String::new(),
    });
    let stdout: Stdout = Arc::new(Mutex::new(io::stdout()));
    let (tx, rx) = mpsc::channel::<Inbound>();

    {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut stdin = io::stdin().lock();
            loop {
                match read_frame(&mut stdin) {
                    Ok(Some(frame)) => {
                        if tx.send(Inbound::Frame(frame)).is_err() {
                            break;
                        }
                    }
                    _ => {
                        let _ = tx.send(Inbound::StdinClosed);
                        break;
                    }
                }
            }
        });
    }

    let mut pipe: Option<Pipe> = None;
    let mut generation = 0u64;
    let mut last_status: Option<bool> = None;

    loop {
        if pipe.is_none() {
            match connect(BRIDGE_PIPE_NAME) {
                Ok(mut p) => {
                    generation += 1;
                    if write_frame(&mut p, &host_hello_frame(&browser)).is_err() {
                        // Could not even introduce ourselves: retry on the next round.
                    } else {
                        spawn_pipe_reader(p.clone(), generation, tx.clone(), stdout.clone());
                        pipe = Some(p);
                        set_status(&stdout, &mut last_status, true);
                    }
                }
                Err(_) => set_status(&stdout, &mut last_status, false),
            }
        }

        let next = if pipe.is_some() {
            rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
        } else {
            rx.recv_timeout(RETRY_EVERY)
        };
        match next {
            Ok(Inbound::Frame(frame)) => match pipe.as_mut() {
                Some(p) => {
                    if write_frame(p, &frame).is_err() {
                        pipe = None;
                        set_status(&stdout, &mut last_status, false);
                        reply_unavailable(&stdout, &frame);
                    }
                }
                None => reply_unavailable(&stdout, &frame),
            },
            Ok(Inbound::PipeLost(gen)) if gen == generation => {
                pipe = None;
                set_status(&stdout, &mut last_status, false);
            }
            Ok(Inbound::PipeLost(_)) => {}
            Ok(Inbound::StdinClosed) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

/// Only reports a change, so the extension is not told "not running" every few seconds.
fn set_status(stdout: &Stdout, last: &mut Option<bool>, connected: bool) {
    if *last != Some(connected) {
        *last = Some(connected);
        send_to_browser(stdout, &status_frame(connected));
    }
}

fn reply_unavailable(stdout: &Stdout, frame: &[u8]) {
    // Notifications (no id) are dropped silently: there is nobody to answer.
    if let Some(id) = request_id(frame) {
        send_to_browser(stdout, &unavailable_frame(&id));
    }
}

/// Copies frames from the pipe to the browser until the pipe ends.
fn spawn_pipe_reader(mut pipe: Pipe, generation: u64, tx: mpsc::Sender<Inbound>, stdout: Stdout) {
    std::thread::spawn(move || {
        loop {
            match read_frame(&mut pipe) {
                Ok(Some(frame)) => send_to_browser(&stdout, &frame),
                _ => {
                    let _ = tx.send(Inbound::PipeLost(generation));
                    break;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_browser_is_the_first_ancestor_that_is_not_a_shell() {
        let chain = vec![
            (10, "cmd.exe".to_string()),
            (11, "conhost.exe".to_string()),
            (12, "chrome.exe".to_string()),
            (13, "explorer.exe".to_string()),
        ];
        assert_eq!(pick_browser(&chain), Some((12, "chrome.exe".to_string())));
    }

    #[test]
    fn no_browser_when_only_shells_are_above_us() {
        let chain = vec![(10, "cmd.exe".to_string())];
        assert_eq!(pick_browser(&chain), None);
    }

    #[test]
    fn shell_names_match_regardless_of_case() {
        let chain = vec![(1, "CMD.EXE".to_string()), (2, "msedge.exe".to_string())];
        assert_eq!(pick_browser(&chain), Some((2, "msedge.exe".to_string())));
    }

    #[test]
    fn host_hello_has_the_fields_the_engine_expects() {
        let browser = BrowserProcess {
            pid: 42,
            name: "brave.exe".into(),
            path: r"C:\Brave\brave.exe".into(),
        };
        let json: Value = serde_json::from_slice(&host_hello_frame(&browser)).unwrap();
        assert_eq!(json["type"], "hostHello");
        assert_eq!(json["browserPid"], 42);
        assert_eq!(json["browserProcessName"], "brave.exe");
        assert_eq!(json["browserExePath"], r"C:\Brave\brave.exe");
        assert_eq!(json["hostVersion"], HOST_VERSION);
    }

    #[test]
    fn status_frame_reports_the_connection() {
        let on: Value = serde_json::from_slice(&status_frame(true)).unwrap();
        let off: Value = serde_json::from_slice(&status_frame(false)).unwrap();
        assert_eq!(on, json!({"type": "bridgeStatus", "connected": true}));
        assert_eq!(off, json!({"type": "bridgeStatus", "connected": false}));
    }

    #[test]
    fn only_requests_with_an_id_get_an_unavailable_reply() {
        assert_eq!(
            request_id(br#"{"type":"hello","id":"7","protocol":1}"#),
            Some("7".into())
        );
        assert_eq!(request_id(br#"{"type":"fullscreenNative","tabId":3}"#), None);
        assert_eq!(request_id(b"not json"), None);

        let reply: Value = serde_json::from_slice(&unavailable_frame("7")).unwrap();
        assert_eq!(reply["type"], "error");
        assert_eq!(reply["replyTo"], "7");
        assert_eq!(reply["code"], "desquUnavailable");
    }

    #[test]
    fn utf16_names_stop_at_the_terminator() {
        let mut buf = [0u16; 8];
        for (i, c) in "chrome".encode_utf16().enumerate() {
            buf[i] = c;
        }
        assert_eq!(utf16_to_string(&buf), "chrome");
    }
}
