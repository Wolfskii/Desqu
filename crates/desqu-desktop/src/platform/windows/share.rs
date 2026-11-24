//! Sharing the layer between processes. Vials and Workspaces are separate programs, but the desktop
//! has one layer: the first program to open it hosts it (it holds the lock) and serves it on a pipe;
//! the next one gets a [`Shared`] handle that sends the same commands over that pipe.
//!
//! The host's handles and the remote ones are equal: an item stays hidden until every owner that
//! hid it shows it. When a remote program ends, the host shows what it had hidden. When the host
//! ends (or crashes), a remote program takes over: it becomes the host, first puts back what the
//! old host's journal still lists, then hides its own items again.

use std::collections::HashSet;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use desqu_ipc::pipe::{connect, Pipe, PipeListener};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Handle, Host, Notifier};
use crate::layer::{ClaimSink, DesktopError, DesktopLayer, DesktopResult, EventSink, HideReport, Subscription};
use crate::model::{DesktopItem, Point};

/// How long a program whose host went away keeps trying to reach (or become) the next one.
const TAKEOVER_WINDOW: Duration = Duration::from_secs(5);
/// Messages carry item lists; a desktop with a few thousand items still fits.
const MAX_MESSAGE: usize = 16 * 1024 * 1024;

/// One pipe per Windows session, like the lock (`Local\`).
fn pipe_name() -> String {
    let mut session = 0u32;
    // SAFETY: plain query about this process.
    let _ = unsafe {
        windows::Win32::System::RemoteDesktop::ProcessIdToSessionId(std::process::id(), &mut session)
    };
    format!(r"\\.\pipe\Desqu.DesktopLayer.{session}")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub(super) enum Request {
    /// First message of a command connection: who is asking. Answered with the host's journal.
    Hello { owner: String },
    /// First message of an event connection: the host then sends a message per claim change.
    Subscribe,
    Items,
    CanHide,
    Hide { ids: Vec<String> },
    Show { ids: Vec<String> },
    ShowAt { id: String, screen: Point },
    ShowBlock { ids: Vec<String>, screen: Point },
    ShowAll,
    Hidden,
    SetSurface { on: bool },
    SurfaceActive,
}

type Reply = Result<Value, String>;

fn write_msg(pipe: &mut Pipe, value: &impl Serialize) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    if bytes.len() > MAX_MESSAGE {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "message too large"));
    }
    pipe.write_all(&(bytes.len() as u32).to_le_bytes())?;
    pipe.write_all(&bytes)?;
    pipe.flush()
}

/// `Ok(None)` when the other end closed between messages.
fn read_msg<T: for<'de> Deserialize<'de>>(pipe: &mut Pipe) -> io::Result<Option<T>> {
    let mut header = [0u8; 4];
    let mut filled = 0;
    while filled < 4 {
        match pipe.read(&mut header[filled..])? {
            0 if filled == 0 => return Ok(None),
            0 => return Err(io::ErrorKind::UnexpectedEof.into()),
            n => filled += n,
        }
    }
    let len = u32::from_le_bytes(header) as usize;
    if len > MAX_MESSAGE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "message too large"));
    }
    let mut body = vec![0u8; len];
    pipe.read_exact(&mut body)?;
    serde_json::from_slice(&body).map(Some).map_err(io::Error::other)
}

fn to_value(result: DesktopResult<impl Serialize>) -> Reply {
    result.map_err(|e| e.to_string()).and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string()))
}

/// Runs one command against a layer handle (the owner is the handle's).
fn dispatch(layer: &dyn DesktopLayer, request: &Request) -> Reply {
    match request {
        Request::Hello { .. } | Request::Subscribe => Err("not a command".into()),
        Request::Items => to_value(layer.items()),
        Request::CanHide => to_value(layer.can_hide()),
        Request::Hide { ids } => to_value(layer.hide(ids)),
        Request::Show { ids } => to_value(layer.show(ids)),
        Request::ShowAt { id, screen } => to_value(layer.show_at(id, *screen)),
        Request::ShowBlock { ids, screen } => to_value(layer.show_block(ids, *screen)),
        Request::ShowAll => to_value(layer.show_all()),
        Request::Hidden => to_value(layer.hidden()),
        Request::SetSurface { on } => to_value(layer.set_surface(*on)),
        Request::SurfaceActive => to_value(Ok::<_, DesktopError>(layer.surface_active())),
    }
}

// ---------------------------------------------------------------------------------- the host side

/// The pipe the host serves. Dropped with the host: open connections are closed, so the remote
/// programs notice and take over.
pub(super) struct Server {
    name: String,
    stop: Arc<AtomicBool>,
    connections: Arc<Mutex<Vec<(u64, Pipe)>>>,
    accept: Option<JoinHandle<()>>,
}

impl Server {
    pub(super) fn start(host: Weak<Host>) -> io::Result<Self> {
        let name = pipe_name();
        let mut listener = PipeListener::bind(&name)?;
        let stop = Arc::new(AtomicBool::new(false));
        let connections: Arc<Mutex<Vec<(u64, Pipe)>>> = Arc::default();
        let accept = {
            let (stop, connections) = (stop.clone(), connections.clone());
            std::thread::Builder::new().name("desqu-desktop-share".into()).spawn(move || {
                let next = AtomicU64::new(0);
                while let Ok(pipe) = listener.accept() {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let id = next.fetch_add(1, Ordering::Relaxed);
                    connections.lock().unwrap_or_else(|e| e.into_inner()).push((id, pipe.clone()));
                    let (host, connections) = (host.clone(), connections.clone());
                    let _ = std::thread::Builder::new().name("desqu-desktop-peer".into()).spawn(move || {
                        serve(pipe, &host);
                        connections.lock().unwrap_or_else(|e| e.into_inner()).retain(|(i, _)| *i != id);
                    });
                }
            })?
        };
        Ok(Self { name, stop, connections, accept: Some(accept) })
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        for (_, pipe) in self.connections.lock().unwrap_or_else(|e| e.into_inner()).drain(..) {
            pipe.close();
        }
        // A throwaway connection wakes the accept loop so it sees `stop`.
        drop(connect(&self.name));
        if let Some(accept) = self.accept.take() {
            if accept.thread().id() != std::thread::current().id() {
                let _ = accept.join();
            }
        }
    }
}

fn serve(mut pipe: Pipe, host: &Weak<Host>) {
    match read_msg::<Request>(&mut pipe) {
        Ok(Some(Request::Hello { owner })) => commands(pipe, host, owner),
        Ok(Some(Request::Subscribe)) => events(pipe, host),
        _ => {}
    }
}

/// A remote owner's commands. Its handle lives as long as the connection: when the program ends,
/// dropping the handle shows everything it had hidden.
fn commands(mut pipe: Pipe, host: &Weak<Host>, owner: String) {
    let Some(host) = host.upgrade() else { return };
    let journal = host.journal_path.to_string_lossy().into_owned();
    let handle = Handle { owner, host };
    if write_msg(&mut pipe, &Ok::<_, String>(Value::String(journal))).is_err() {
        return;
    }
    while let Ok(Some(request)) = read_msg::<Request>(&mut pipe) {
        if write_msg(&mut pipe, &dispatch(&handle, &request)).is_err() {
            break;
        }
    }
}

/// Sends a message for every claim change until either side closes.
fn events(pipe: Pipe, host: &Weak<Host>) {
    let Some(notifier) = host.upgrade().map(|h| h.notifier.clone()) else { return };
    enum Msg {
        Claims,
        Closed,
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let _subscription = {
        let tx = Mutex::new(tx.clone());
        notifier.add(Box::new(move || drop(tx.lock().unwrap_or_else(|e| e.into_inner()).send(Msg::Claims))))
    };
    // Nothing is expected from the remote end; the read only ends when it closes.
    let mut reader = pipe.clone();
    let _ = std::thread::Builder::new().name("desqu-desktop-peer-watch".into()).spawn(move || {
        while let Ok(Some(_)) = read_msg::<Value>(&mut reader) {}
        let _ = tx.send(Msg::Closed);
    });
    let mut writer = pipe;
    while let Ok(Msg::Claims) = rx.recv() {
        if write_msg(&mut writer, &Value::Null).is_err() {
            break;
        }
    }
}

// ---------------------------------------------------------------------------------- the remote side

enum Link {
    Remote { pipe: Pipe, events: Pipe, host_journal: PathBuf },
    /// After a takeover this program hosts the layer itself.
    Local { layer: Arc<dyn DesktopLayer>, _claims: Subscription },
}

impl Link {
    fn close(&self) {
        if let Link::Remote { pipe, events, .. } = self {
            pipe.close();
            events.close();
        }
    }
}

/// What this owner hides, so it can hide it again under a new host.
#[derive(Default)]
struct Mine {
    hidden: HashSet<String>,
    surface: bool,
}

/// A handle on a layer another program hosts.
pub(super) struct Shared {
    owner: String,
    journal_path: PathBuf,
    link: Mutex<(u64, Link)>,
    mine: Mutex<Mine>,
    sinks: Arc<Notifier>,
    me: Weak<Shared>,
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

fn connect_remote(owner: &str) -> io::Result<(Pipe, Pipe, PathBuf)> {
    let name = pipe_name();
    let mut pipe = connect(&name)?;
    write_msg(&mut pipe, &Request::Hello { owner: owner.to_string() })?;
    let journal = match read_msg::<Reply>(&mut pipe)? {
        Some(Ok(Value::String(journal))) => PathBuf::from(journal),
        _ => return Err(io::Error::other("the desktop layer's host did not answer")),
    };
    let mut events = connect(&name)?;
    write_msg(&mut events, &Request::Subscribe)?;
    Ok((pipe, events, journal))
}

impl Shared {
    /// Connects to the program that hosts the layer. `OwnedElsewhere` when nothing answers (a host
    /// that does not share, such as an older build).
    pub(super) fn connect(owner: &str, journal_path: &Path) -> DesktopResult<Arc<dyn DesktopLayer>> {
        let started = Instant::now();
        let (pipe, events, host_journal) = loop {
            match connect_remote(owner) {
                Ok(link) => break link,
                // The host may have taken the lock a moment ago and not be serving yet.
                Err(_) if started.elapsed() < Duration::from_secs(1) => std::thread::sleep(Duration::from_millis(100)),
                Err(e) => {
                    tracing::info!("the desktop layer is held by a program that does not share it: {e}");
                    return Err(DesktopError::OwnedElsewhere);
                }
            }
        };
        // What a crashed run of this program left hidden is in its own journal, which no host
        // uses now: put it back before hiding anything.
        if !same_path(&host_journal, journal_path) {
            if let Err(e) = super::recover_from(journal_path) {
                tracing::warn!("could not restore items a previous run left hidden: {e}");
            }
        }
        let shared = Arc::new_cyclic(|me| Shared {
            owner: owner.to_string(),
            journal_path: journal_path.to_path_buf(),
            link: Mutex::new((0, Link::Remote { pipe, events: events.clone(), host_journal })),
            mine: Mutex::default(),
            sinks: Arc::default(),
            me: me.clone(),
        });
        shared.watch(0, events);
        Ok(shared)
    }

    /// Relays the host's claim changes, and takes over when the host goes away.
    fn watch(&self, generation: u64, mut events: Pipe) {
        let me = self.me.clone();
        let _ = std::thread::Builder::new().name("desqu-desktop-remote".into()).spawn(move || {
            while let Ok(Some(_)) = read_msg::<Value>(&mut events) {
                match me.upgrade() {
                    Some(shared) => shared.sinks.notify(),
                    None => return,
                }
            }
            if let Some(shared) = me.upgrade() {
                if let Err(e) = shared.take_over(generation) {
                    tracing::warn!("{} lost the desktop layer: {e}", shared.owner);
                }
            }
        });
    }

    /// The host went away: become the host, or connect to whoever did. Then hide this owner's
    /// items again.
    fn take_over(&self, seen: u64) -> DesktopResult<()> {
        {
            let mut link = self.link.lock().unwrap_or_else(|e| e.into_inner());
            if link.0 != seen {
                return Ok(()); // someone already did
            }
            link.1.close();
            let old_journal = match &link.1 {
                Link::Remote { host_journal, .. } => Some(host_journal.clone()),
                Link::Local { .. } => None,
            };
            let started = Instant::now();
            let next = loop {
                match super::open_local(&self.owner, &self.journal_path, old_journal.as_deref()) {
                    Ok(layer) => {
                        let sinks = self.sinks.clone();
                        let claims = layer.subscribe_claims(Box::new(move || sinks.notify()))?;
                        break Link::Local { layer, _claims: claims };
                    }
                    Err(DesktopError::OwnedElsewhere) => {}
                    Err(e) if started.elapsed() >= TAKEOVER_WINDOW => return Err(e),
                    Err(e) => tracing::debug!("taking over the desktop layer: {e}"),
                }
                if let Ok((pipe, events, host_journal)) = connect_remote(&self.owner) {
                    self.watch(seen + 1, events.clone());
                    break Link::Remote { pipe, events, host_journal };
                }
                if started.elapsed() >= TAKEOVER_WINDOW {
                    return Err(DesktopError::OwnedElsewhere);
                }
                std::thread::sleep(Duration::from_millis(100));
            };
            *link = (seen + 1, next);
        }
        let (surface, hidden) = {
            let mine = self.mine.lock().unwrap_or_else(|e| e.into_inner());
            (mine.surface, mine.hidden.iter().cloned().collect::<Vec<_>>())
        };
        if surface {
            self.request(&Request::SetSurface { on: true })?;
        }
        if !hidden.is_empty() {
            self.request(&Request::Hide { ids: hidden })?;
        }
        self.sinks.notify();
        Ok(())
    }

    fn send(&self, request: &Request) -> (u64, io::Result<Reply>) {
        let link = self.link.lock().unwrap_or_else(|e| e.into_inner());
        let outcome = match &link.1 {
            Link::Local { layer, .. } => Ok(dispatch(layer.as_ref(), request)),
            Link::Remote { pipe, .. } => {
                let mut pipe = pipe.clone();
                write_msg(&mut pipe, request).and_then(|()| {
                    read_msg::<Reply>(&mut pipe)?.ok_or_else(|| io::ErrorKind::BrokenPipe.into())
                })
            }
        };
        (link.0, outcome)
    }

    fn request(&self, request: &Request) -> DesktopResult<Value> {
        let reply = match self.send(request) {
            (_, Ok(reply)) => reply,
            (generation, Err(_)) => {
                self.take_over(generation)?;
                self.send(request).1.map_err(DesktopError::from)?
            }
        };
        reply.map_err(|message| DesktopError::Native { op: "desktop layer", message })
    }

    fn call<T: for<'de> Deserialize<'de>>(&self, request: Request) -> DesktopResult<T> {
        let value = self.request(&request)?;
        serde_json::from_value(value).map_err(|e| DesktopError::Native { op: "desktop layer", message: e.to_string() })
    }

    fn mine(&self) -> std::sync::MutexGuard<'_, Mine> {
        self.mine.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl DesktopLayer for Shared {
    fn items(&self) -> DesktopResult<Vec<DesktopItem>> {
        self.call(Request::Items)
    }

    fn icon_png(&self, id: &str, size: u32) -> DesktopResult<Vec<u8>> {
        // Reading an icon needs no lock: do it here instead of sending the picture over the pipe.
        let id = id.to_string();
        std::thread::Builder::new()
            .name("desqu-desktop-icon".into())
            .spawn(move || super::icon_png_standalone(&id, size))?
            .join()
            .map_err(|_| DesktopError::Native { op: "icon", message: "thread panicked".into() })?
    }

    fn can_hide(&self) -> DesktopResult<bool> {
        self.call(Request::CanHide)
    }

    fn hide(&self, ids: &[String]) -> DesktopResult<HideReport> {
        let report: HideReport = self.call(Request::Hide { ids: ids.to_vec() })?;
        self.mine().hidden.extend(report.hidden.iter().cloned());
        Ok(report)
    }

    fn show(&self, ids: &[String]) -> DesktopResult<()> {
        self.call::<()>(Request::Show { ids: ids.to_vec() })?;
        let mut mine = self.mine();
        for id in ids {
            mine.hidden.remove(id);
        }
        Ok(())
    }

    fn show_at(&self, id: &str, screen: Point) -> DesktopResult<()> {
        self.call::<()>(Request::ShowAt { id: id.to_string(), screen })?;
        self.mine().hidden.remove(id);
        Ok(())
    }

    fn show_block(&self, ids: &[String], screen: Point) -> DesktopResult<()> {
        self.call::<()>(Request::ShowBlock { ids: ids.to_vec(), screen })?;
        let mut mine = self.mine();
        for id in ids {
            mine.hidden.remove(id);
        }
        Ok(())
    }

    fn show_all(&self) -> DesktopResult<()> {
        self.call::<()>(Request::ShowAll)?;
        let mut mine = self.mine();
        mine.hidden.clear();
        mine.surface = false;
        Ok(())
    }

    fn hidden(&self) -> DesktopResult<Vec<String>> {
        self.call(Request::Hidden)
    }

    fn subscribe(&self, sink: EventSink) -> DesktopResult<Subscription> {
        super::watch::watch(super::shell::desktop_folders(), sink)
    }

    fn set_surface(&self, on: bool) -> DesktopResult<()> {
        self.call::<()>(Request::SetSurface { on })?;
        self.mine().surface = on;
        Ok(())
    }

    fn surface_active(&self) -> bool {
        self.call(Request::SurfaceActive).unwrap_or(false)
    }

    fn subscribe_claims(&self, sink: ClaimSink) -> DesktopResult<Subscription> {
        Ok(self.sinks.add(sink))
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        let link = self.link.get_mut().unwrap_or_else(|e| e.into_inner());
        if let Link::Remote { pipe, .. } = &link.1 {
            // Like a local handle: never leave this owner's items hidden. (The host would also do
            // it when the connection closes.)
            let mut pipe = pipe.clone();
            if write_msg(&mut pipe, &Request::ShowAll).is_ok() {
                let _ = read_msg::<Reply>(&mut pipe);
            }
        }
        link.1.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_round_trip_as_json() {
        let all = [
            Request::Hello { owner: "workspaces".into() },
            Request::Subscribe,
            Request::Hide { ids: vec![r"C:\Users\a\Desktop\x.txt".into()] },
            Request::ShowAt { id: "y".into(), screen: Point { x: -5, y: 7 } },
            Request::ShowBlock { ids: vec![], screen: Point { x: 1, y: 2 } },
            Request::SetSurface { on: true },
            Request::ShowAll,
        ];
        for request in all {
            let text = serde_json::to_string(&request).unwrap();
            assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request, "{text}");
        }
        let report = HideReport { hidden: vec!["a".into()], missing: vec![], failed: vec!["b".into()] };
        let back: HideReport = serde_json::from_value(to_value(Ok::<_, DesktopError>(report.clone())).unwrap()).unwrap();
        assert_eq!(back, report);
        let unit: () = serde_json::from_value(to_value(Ok::<_, DesktopError>(())).unwrap()).unwrap();
        assert_eq!(unit, ());
    }

    /// Two "programs" in one process: one hosts, the other connects over the pipe; when the host
    /// ends, the other takes over. Read-only: it lists the desktop and hides nothing.
    #[test]
    #[ignore = "requires an interactive Windows desktop, and no other Desqu program running"]
    fn a_second_program_uses_the_host_and_takes_over_when_it_ends() {
        let dir = std::env::temp_dir().join(format!("desqu-share-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let host = match super::super::open_local("a", &dir.join("a.json"), None) {
            Err(DesktopError::OwnedElsewhere) => {
                eprintln!("skipped: another program holds the desktop layer");
                return;
            }
            other => other.unwrap(),
        };
        let remote = Shared::connect("b", &dir.join("b.json")).unwrap();
        let mut local_ids: Vec<String> = host.items().unwrap().into_iter().map(|i| i.id).collect();
        let mut remote_ids: Vec<String> = remote.items().unwrap().into_iter().map(|i| i.id).collect();
        local_ids.sort();
        remote_ids.sort();
        assert_eq!(remote_ids, local_ids, "the remote handle sees the host's desktop");
        assert_eq!(remote.can_hide().unwrap(), host.can_hide().unwrap());

        drop(host);
        // The remote program notices the host is gone and hosts the layer itself.
        let started = Instant::now();
        while super::super::HOST.lock().unwrap().upgrade().is_none() {
            assert!(started.elapsed() < Duration::from_secs(6), "nobody took over");
            std::thread::sleep(Duration::from_millis(50));
        }
        let mut after: Vec<String> = remote.items().unwrap().into_iter().map(|i| i.id).collect();
        after.sort();
        assert_eq!(after, local_ids);
        drop(remote);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_pipe_is_per_session() {
        assert!(pipe_name().starts_with(r"\\.\pipe\Desqu.DesktopLayer."));
    }

    #[test]
    fn journal_paths_compare_without_case() {
        assert!(same_path(Path::new(r"C:\Users\A\x.json"), Path::new(r"c:\users\a\X.json")));
        assert!(!same_path(Path::new(r"C:\a.json"), Path::new(r"C:\b.json")));
    }
}
