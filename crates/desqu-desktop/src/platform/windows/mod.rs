//! Windows backend. One worker thread (a COM single-threaded apartment) owns every shell object
//! and the journal; handles talk to it over a channel, so shell access is serialized and no COM
//! pointer ever crosses threads.

mod lock;
mod shell;
mod watch;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};

use crate::layer::{DesktopError, DesktopLayer, DesktopResult, EventSink, HideReport, Subscription};
use crate::model::{classify, is_parked, plan_recovery, DesktopItem, Journal, ParkedItem, PARK_POINT};

/// How long a handle waits for the worker before giving up (Explorer can be slow or hung).
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

type Reply<T> = Sender<DesktopResult<T>>;

enum Command {
    Items(Reply<Vec<DesktopItem>>),
    Icon { id: String, size: u32, reply: Reply<Vec<u8>> },
    CanHide(Reply<bool>),
    Hide { owner: String, ids: Vec<String>, reply: Reply<HideReport> },
    Show { owner: String, ids: Vec<String>, reply: Reply<()> },
    ShowAll { owner: String, reply: Reply<()> },
    Hidden { owner: String, reply: Reply<Vec<String>> },
    Shutdown,
}

struct State {
    journal: Journal,
    path: PathBuf,
}

impl State {
    fn save(&self) -> DesktopResult<()> {
        self.journal.save(&self.path).map_err(DesktopError::from)
    }
}

/// The process-wide layer. Dropping the last handle drops it, which restores every parked item.
struct Host {
    tx: Mutex<Sender<Command>>,
    thread: Mutex<Option<JoinHandle<()>>>,
    _lock: lock::NamedLock,
}

static HOST: Mutex<Weak<Host>> = Mutex::new(Weak::new());

pub(crate) fn open(owner: &str, journal_path: &Path) -> DesktopResult<Arc<dyn DesktopLayer>> {
    let mut slot = HOST.lock().unwrap_or_else(|e| e.into_inner());
    let host = match slot.upgrade() {
        Some(host) => host,
        None => {
            let host = Host::start(journal_path)?;
            *slot = Arc::downgrade(&host);
            host
        }
    };
    Ok(Arc::new(Handle { owner: owner.to_string(), host }))
}

pub(crate) fn recover_from(journal_path: &Path) -> DesktopResult<usize> {
    let path = journal_path.to_path_buf();
    // Shell calls need a COM apartment of their own, so use a short-lived thread.
    std::thread::Builder::new()
        .name("desqu-desktop-recover".into())
        .spawn(move || with_com(|| recover_file(&path)))?
        .join()
        .map_err(|_| DesktopError::Native { op: "recover", message: "recovery thread panicked".into() })?
}

fn with_com<T>(f: impl FnOnce() -> T) -> T {
    // SAFETY: balanced with CoUninitialize below; this thread owns its apartment.
    let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    let result = f();
    if initialized {
        unsafe { CoUninitialize() };
    }
    result
}

impl Host {
    fn start(journal_path: &Path) -> DesktopResult<Arc<Self>> {
        let lock = lock::acquire(lock::LOCK_NAME)
            .map_err(|message| DesktopError::Native { op: "lock", message })?
            .ok_or(DesktopError::OwnedElsewhere)?;
        let (tx, rx) = mpsc::channel();
        let path = journal_path.to_path_buf();
        let thread = std::thread::Builder::new()
            .name("desqu-desktop".into())
            .spawn(move || with_com(|| worker(rx, path)))?;
        Ok(Arc::new(Self {
            tx: Mutex::new(tx),
            thread: Mutex::new(Some(thread)),
            _lock: lock,
        }))
    }

    fn call<T>(&self, make: impl FnOnce(Reply<T>) -> Command) -> DesktopResult<T> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .send(make(reply_tx))
            .map_err(|_| DesktopError::Native { op: "desktop layer", message: "the worker has stopped".into() })?;
        reply_rx
            .recv_timeout(REPLY_TIMEOUT)
            .map_err(|_| DesktopError::Native { op: "desktop layer", message: "Explorer did not respond in time".into() })?
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.tx.lock().unwrap_or_else(|e| e.into_inner()).send(Command::Shutdown);
        if let Some(thread) = self.thread.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = thread.join();
        }
    }
}

// ---------------------------------------------------------------------------------- the worker

fn worker(rx: Receiver<Command>, path: PathBuf) {
    let mut state = State { journal: Journal::load(&path), path };
    // A journal at startup is a previous run's leftovers (crash or kill): put everything back.
    if let Err(e) = restore_all(&mut state) {
        tracing::warn!("could not restore desktop items left by a previous run: {e}");
    }
    while let Ok(command) = rx.recv() {
        match command {
            Command::Items(reply) => {
                let _ = reply.send(items(&state));
            }
            Command::Icon { id, size, reply } => {
                // SAFETY: on the COM thread.
                let _ = reply.send(unsafe { shell::icon_png(&id, size) });
            }
            Command::CanHide(reply) => {
                let _ = reply.send(can_hide());
            }
            Command::Hide { owner, ids, reply } => {
                let _ = reply.send(hide(&mut state, &owner, &ids));
            }
            Command::Show { owner, ids, reply } => {
                let _ = reply.send(show(&mut state, &owner, &ids));
            }
            Command::ShowAll { owner, reply } => {
                let _ = reply.send(show_all(&mut state, &owner));
            }
            Command::Hidden { owner, reply } => {
                let _ = reply.send(Ok(state.journal.hidden_by(&owner)));
            }
            Command::Shutdown => break,
        }
    }
    // The last handle is gone (or the process is ending): never leave items parked.
    if let Err(e) = restore_all(&mut state) {
        tracing::warn!("could not restore desktop items on shutdown: {e}");
    }
}

fn items(state: &State) -> DesktopResult<Vec<DesktopItem>> {
    // SAFETY: on the COM thread.
    let raw = unsafe {
        let view = shell::desktop_view()?;
        shell::list(&view)?
    };
    Ok(raw
        .into_iter()
        .map(|r| {
            let kind = classify(&r.id, Path::new(&r.id).is_dir());
            DesktopItem {
                parked: state.journal.contains(&r.id) || is_parked(r.pos),
                id: r.id,
                name: r.name,
                kind,
                position: r.pos,
            }
        })
        .collect())
}

fn can_hide() -> DesktopResult<bool> {
    // SAFETY: on the COM thread.
    unsafe { Ok(!shell::auto_arrange(&shell::desktop_view()?)?) }
}

fn hide(state: &mut State, owner: &str, ids: &[String]) -> DesktopResult<HideReport> {
    // SAFETY: on the COM thread.
    unsafe {
        let view = shell::desktop_view()?;
        if shell::auto_arrange(&view)? {
            return Err(DesktopError::AutoArrange);
        }
        let lv = shell::listview()?;
        let raw = shell::list(&view)?;

        let mut report = HideReport::default();
        let mut to_park = Vec::new();
        for id in ids {
            match raw.iter().find(|r| &r.id == id) {
                None => report.missing.push(id.clone()),
                Some(item) => {
                    if state.journal.park(owner, id, item.pos) {
                        to_park.push(item.index);
                    }
                    report.hidden.push(id.clone());
                }
            }
        }
        // Write-ahead: the original positions are on disk before anything moves.
        state.save()?;
        for index in to_park {
            shell::set_position(lv, index, PARK_POINT)?;
        }
        Ok(report)
    }
}

/// Moves items back, then (only after that succeeded) commits the journal change. A crash in the
/// middle therefore leaves the journal claiming *more* hidden items, never fewer.
fn restore_positions(restore: &[ParkedItem]) -> DesktopResult<()> {
    if restore.is_empty() {
        return Ok(());
    }
    // SAFETY: on the COM thread.
    unsafe {
        let view = shell::desktop_view()?;
        let lv = shell::listview()?;
        let raw = shell::list(&view)?;
        for item in restore {
            // An item deleted meanwhile has nothing to restore.
            if let Some(found) = raw.iter().find(|r| r.id == item.id) {
                shell::set_position(lv, found.index, item.original)?;
            }
        }
    }
    Ok(())
}

fn show(state: &mut State, owner: &str, ids: &[String]) -> DesktopResult<()> {
    let mut next = state.journal.clone();
    let mut restore = Vec::new();
    for id in ids {
        if let Some(original) = next.unpark(owner, id) {
            restore.push(ParkedItem { id: id.clone(), original, owners: Vec::new() });
        }
    }
    restore_positions(&restore)?;
    state.journal = next;
    state.save()
}

fn show_all(state: &mut State, owner: &str) -> DesktopResult<()> {
    let mut next = state.journal.clone();
    let restore = next.release_owner(owner);
    restore_positions(&restore)?;
    state.journal = next;
    state.save()
}

fn restore_all(state: &mut State) -> DesktopResult<()> {
    if state.journal.items.is_empty() {
        return Ok(());
    }
    // SAFETY: on the COM thread.
    let present: HashSet<String> = unsafe {
        let view = shell::desktop_view()?;
        shell::list(&view)?.into_iter().map(|r| r.id).collect()
    };
    let plan = plan_recovery(&state.journal, &present);
    restore_positions(&plan.restore)?;
    state.journal = Journal::default();
    state.save()
}

fn recover_file(path: &Path) -> DesktopResult<usize> {
    let journal = Journal::load(path);
    if journal.items.is_empty() {
        let _ = std::fs::remove_file(path);
        return Ok(0);
    }
    // SAFETY: on a COM thread.
    let present: HashSet<String> = unsafe {
        let view = shell::desktop_view()?;
        shell::list(&view)?.into_iter().map(|r| r.id).collect()
    };
    let plan = plan_recovery(&journal, &present);
    restore_positions(&plan.restore)?;
    let _ = std::fs::remove_file(path);
    Ok(plan.restore.len())
}

// ---------------------------------------------------------------------------------- the handle

struct Handle {
    owner: String,
    host: Arc<Host>,
}

impl DesktopLayer for Handle {
    fn items(&self) -> DesktopResult<Vec<DesktopItem>> {
        self.host.call(Command::Items)
    }

    fn icon_png(&self, id: &str, size: u32) -> DesktopResult<Vec<u8>> {
        self.host.call(|reply| Command::Icon { id: id.to_string(), size, reply })
    }

    fn can_hide(&self) -> DesktopResult<bool> {
        self.host.call(Command::CanHide)
    }

    fn hide(&self, ids: &[String]) -> DesktopResult<HideReport> {
        self.host.call(|reply| Command::Hide { owner: self.owner.clone(), ids: ids.to_vec(), reply })
    }

    fn show(&self, ids: &[String]) -> DesktopResult<()> {
        self.host.call(|reply| Command::Show { owner: self.owner.clone(), ids: ids.to_vec(), reply })
    }

    fn show_all(&self) -> DesktopResult<()> {
        self.host.call(|reply| Command::ShowAll { owner: self.owner.clone(), reply })
    }

    fn hidden(&self) -> DesktopResult<Vec<String>> {
        self.host.call(|reply| Command::Hidden { owner: self.owner.clone(), reply })
    }

    fn subscribe(&self, sink: EventSink) -> DesktopResult<Subscription> {
        watch::watch(shell::desktop_folders(), sink)
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // A module that forgets to call show_all still never leaves its items parked.
        let _ = self.host.call(|reply| Command::ShowAll { owner: self.owner.clone(), reply });
    }
}
