//! Windows backend. One worker thread (a COM single-threaded apartment) owns every shell object
//! and the journal; handles talk to it over a channel, so shell access is serialized and no COM
//! pointer ever crosses threads.

mod lock;
mod shell;
mod watch;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};

use crate::layer::{ClaimSink, DesktopError, DesktopLayer, DesktopResult, EventSink, HideReport, Subscription};
use crate::model::{classify, is_parked, plan_recovery, DesktopItem, Journal, ParkedItem, Point, FALLBACK_ORIGIN};

/// How long a handle waits for the worker before giving up (Explorer can be slow or hung).
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

type Reply<T> = Sender<DesktopResult<T>>;

enum Command {
    Items(Reply<Vec<DesktopItem>>),
    Icon { id: String, size: u32, reply: Reply<Vec<u8>> },
    CanHide(Reply<bool>),
    Hide { owner: String, ids: Vec<String>, reply: Reply<HideReport> },
    Show { owner: String, ids: Vec<String>, reply: Reply<()> },
    ShowAt { owner: String, id: String, screen: Point, reply: Reply<()> },
    ShowBlock { owner: String, ids: Vec<String>, screen: Point, reply: Reply<()> },
    ShowAll { owner: String, reply: Reply<()> },
    Hidden { owner: String, reply: Reply<Vec<String>> },
    SetSurface { owner: String, on: bool, reply: Reply<()> },
    SurfaceActive(Reply<bool>),
    Shutdown,
}

struct State {
    journal: Journal,
    path: PathBuf,
    /// The owner that switched the desktop's own icons off and draws them itself. While it is
    /// set nothing is parked: hiding only records claims, and `journal.items` stays empty.
    surface: Option<String>,
    /// Surface mode: the ids each owner wants hidden.
    claims: HashMap<String, HashSet<String>>,
    notifier: Arc<Notifier>,
}

/// Listeners for "an owner hid or showed something". Called on the worker thread.
#[derive(Default)]
struct Notifier {
    next: AtomicU64,
    sinks: Mutex<Vec<(u64, ClaimSink)>>,
}

impl Notifier {
    fn add(self: &Arc<Self>, sink: ClaimSink) -> Subscription {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.sinks.lock().unwrap_or_else(|e| e.into_inner()).push((id, sink));
        let this = Arc::clone(self);
        Subscription::new(move || this.sinks.lock().unwrap_or_else(|e| e.into_inner()).retain(|(i, _)| *i != id))
    }

    fn notify(&self) {
        for (_, sink) in self.sinks.lock().unwrap_or_else(|e| e.into_inner()).iter() {
            sink();
        }
    }
}

impl State {
    fn save(&self) -> DesktopResult<()> {
        self.journal.save(&self.path).map_err(DesktopError::from)
    }
}

/// The process-wide layer. Dropping the last handle drops it, which restores every parked item.
struct Host {
    journal_path: PathBuf,
    notifier: Arc<Notifier>,
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
    // The journal of this process's own live layer is not "left over": it describes items that
    // are hidden on purpose right now. Touching it would unhide them behind its owners' backs.
    if let Some(host) = HOST.lock().unwrap_or_else(|e| e.into_inner()).upgrade() {
        if host.journal_path == journal_path {
            return Ok(0);
        }
    }
    let path = journal_path.to_path_buf();
    // Shell calls need a COM apartment of their own, so use a short-lived thread.
    std::thread::Builder::new()
        .name("desqu-desktop-recover".into())
        .spawn(move || with_com(|| recover_file(&path)))?
        .join()
        .map_err(|_| DesktopError::Native { op: "recover", message: "recovery thread panicked".into() })?
}

pub(crate) fn item_position(item_id: &str) -> DesktopResult<Option<Point>> {
    let id = item_id.to_string();
    std::thread::Builder::new()
        .name("desqu-desktop-peek".into())
        .spawn(move || {
            with_com(|| {
                // SAFETY: on a COM thread of its own.
                let raw = unsafe {
                    let view = shell::desktop_view()?;
                    shell::list(&view)?
                };
                Ok(raw.into_iter().find(|r| r.id == id).map(|r| r.pos))
            })
        })?
        .join()
        .map_err(|_| DesktopError::Native { op: "peek", message: "thread panicked".into() })?
}

/// Whether the desktop currently shows its icons (right-click the desktop, View, "Show desktop
/// icons"). Read-only, takes no lock.
pub(crate) fn desktop_folders() -> Vec<PathBuf> {
    shell::desktop_folders()
}

pub(crate) fn icons_visible() -> DesktopResult<bool> {
    // SAFETY: plain window lookup and visibility query.
    unsafe { shell::icons_visible() }
}

/// Diagnostics: asks Explorer to put one item where it already is, `rounds` times, and returns how
/// long that took. Used by the `layer_timing` example to learn what one position write costs.
pub(crate) fn nudge_timing(item_id: &str, rounds: usize) -> DesktopResult<std::time::Duration> {
    let id = item_id.to_string();
    std::thread::Builder::new()
        .name("desqu-desktop-timing".into())
        .spawn(move || {
            with_com(|| {
                // SAFETY: on a COM thread of its own.
                unsafe {
                    let view = shell::desktop_view()?;
                    let lv = shell::listview()?;
                    let raw = shell::list(&view)?;
                    let item = raw.iter().find(|r| r.id == id).ok_or(DesktopError::Native {
                        op: "timing",
                        message: "the item is not in the desktop view yet".into(),
                    })?;
                    let started = std::time::Instant::now();
                    for _ in 0..rounds {
                        shell::set_position(lv, item.index, item.pos)?;
                    }
                    Ok(started.elapsed())
                }
            })
        })?
        .join()
        .map_err(|_| DesktopError::Native { op: "timing", message: "thread panicked".into() })?
}

/// Diagnostics: switches the desktop's icons off for a moment and reports what the shell still
/// answers while they are off: (items listed before, items listed while off, selected while off,
/// icons visible again afterwards). Always switches them back on.
pub(crate) fn surface_probe() -> DesktopResult<(usize, usize, usize, bool)> {
    std::thread::Builder::new()
        .name("desqu-desktop-probe".into())
        .spawn(move || {
            with_com(|| {
                struct Restore(bool);
                impl Drop for Restore {
                    fn drop(&mut self) {
                        if self.0 {
                            // SAFETY: on a COM thread.
                            let _ = unsafe { shell::set_icons_hidden(false) };
                        }
                    }
                }
                // SAFETY: on a COM thread of its own.
                unsafe {
                    let before = shell::list(&shell::desktop_view()?)?.len();
                    let mut guard = Restore(false);
                    guard.0 = shell::set_icons_hidden(true)?;
                    std::thread::sleep(Duration::from_millis(400));
                    let view = shell::desktop_view()?;
                    let during = shell::list(&view)?.len();
                    let selected = shell::selection_count(&view)?;
                    drop(guard);
                    Ok((before, during, selected, shell::icons_visible()?))
                }
            })
        })?
        .join()
        .map_err(|_| DesktopError::Native { op: "probe", message: "thread panicked".into() })?
}

/// The id of the desktop folder under `screen` (screen pixels), or `None` when the icon under it is not
/// a folder or there is no icon there. Read-only.
pub(crate) fn folder_at(screen: Point) -> DesktopResult<Option<String>> {
    std::thread::Builder::new()
        .name("desqu-desktop-folder-at".into())
        .spawn(move || {
            with_com(|| {
                // SAFETY: on a COM thread of its own.
                unsafe {
                    let view = shell::desktop_view()?;
                    let lv = shell::listview()?;
                    let p = shell::screen_to_client(lv, screen);
                    let Some(index) = shell::hit_item(lv, p) else { return Ok(None) };
                    let raw = shell::list(&view)?;
                    Ok(raw
                        .into_iter()
                        .find(|r| r.index == index)
                        .filter(|r| Path::new(&r.id).is_dir())
                        .map(|r| r.id))
                }
            })
        })?
        .join()
        .map_err(|_| DesktopError::Native { op: "folder at", message: "thread panicked".into() })?
}

/// How many desktop items are selected right now. Read-only, takes no lock.
pub(crate) fn selection_count() -> DesktopResult<usize> {
    std::thread::Builder::new()
        .name("desqu-desktop-selection".into())
        .spawn(move || {
            with_com(|| {
                // SAFETY: on a COM thread of its own.
                unsafe { shell::selection_count(&shell::desktop_view()?) }
            })
        })?
        .join()
        .map_err(|_| DesktopError::Native { op: "selection", message: "thread panicked".into() })?
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
        let notifier = Arc::new(Notifier::default());
        let thread = {
            let notifier = Arc::clone(&notifier);
            std::thread::Builder::new()
                .name("desqu-desktop".into())
                .spawn(move || with_com(|| worker(rx, path, notifier)))?
        };
        Ok(Arc::new(Self {
            journal_path: journal_path.to_path_buf(),
            notifier,
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

fn worker(rx: Receiver<Command>, path: PathBuf, notifier: Arc<Notifier>) {
    let mut state =
        State { journal: Journal::load(&path), path, surface: None, claims: HashMap::new(), notifier };
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
            Command::ShowBlock { owner, ids, screen, reply } => {
                let _ = reply.send(show_block(&mut state, &owner, &ids, screen));
            }
            Command::ShowAt { owner, id, screen, reply } => {
                let _ = reply.send(show_at(&mut state, &owner, &id, screen));
            }
            Command::ShowAll { owner, reply } => {
                let _ = reply.send(show_all(&mut state, &owner));
            }
            Command::Hidden { owner, reply } => {
                let hidden = match state.surface {
                    Some(_) => state.claims.get(&owner).map(|c| c.iter().cloned().collect()).unwrap_or_default(),
                    None => state.journal.hidden_by(&owner),
                };
                let _ = reply.send(Ok(hidden));
            }
            Command::SetSurface { owner, on, reply } => {
                let _ = reply.send(set_surface(&mut state, &owner, on));
            }
            Command::SurfaceActive(reply) => {
                let _ = reply.send(Ok(state.surface.is_some()));
            }
            Command::Shutdown => break,
        }
    }
    // The last handle is gone (or the process is ending): never leave items parked or the desktop's
    // own icons switched off.
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
    let mut items: Vec<DesktopItem> = raw
        .iter()
        .map(|r| {
            let kind = classify(&r.id, Path::new(&r.id).is_dir());
            DesktopItem {
                parked: if state.surface.is_some() {
                    state.claims.values().any(|ids| ids.contains(&r.id))
                } else {
                    state.journal.contains(&r.id) || is_parked(r.pos)
                },
                id: r.id.clone(),
                name: r.name.clone(),
                kind,
                position: r.pos,
            }
        })
        .collect();
    // Explorer no longer lists what we marked hidden, but it is still on the desktop as far as the
    // owners are concerned: list it from the journal, flagged as hidden.
    if state.surface.is_none() {
        for hidden in state.journal.items.iter().filter(|i| i.by_attribute) {
            if raw.iter().any(|r| r.id == hidden.id) || !Path::new(&hidden.id).exists() {
                continue;
            }
            // SAFETY: on the COM thread.
            let name = unsafe { shell::display_name_of(&hidden.id) };
            items.push(DesktopItem {
                kind: classify(&hidden.id, Path::new(&hidden.id).is_dir()),
                name: if name.is_empty() { hidden.id.rsplit('\\').next().unwrap_or(&hidden.id).to_string() } else { name },
                id: hidden.id.clone(),
                position: hidden.original,
                parked: true,
            });
        }
    }
    Ok(items)
}

fn can_hide() -> DesktopResult<bool> {
    Ok(!shell::show_hidden_files())
}

fn hide(state: &mut State, owner: &str, ids: &[String]) -> DesktopResult<HideReport> {
    if state.surface.is_some() {
        return claim(state, owner, ids);
    }
    hide_by_attribute(state, owner, ids)
}

/// Surface mode: remembers that `owner` wants these items not drawn. Nothing moves.
fn claim(state: &mut State, owner: &str, ids: &[String]) -> DesktopResult<HideReport> {
    // SAFETY: on the COM thread.
    let raw = unsafe { shell::list(&shell::desktop_view()?)? };
    let mut report = HideReport::default();
    let claimed = state.claims.entry(owner.to_string()).or_default();
    let mut changed = false;
    for id in ids {
        if raw.iter().any(|r| &r.id == id) {
            changed |= claimed.insert(id.clone());
            report.hidden.push(id.clone());
        } else {
            report.missing.push(id.clone());
        }
    }
    if changed {
        state.notifier.notify();
    }
    Ok(report)
}

/// Hides files with the hidden attribute (journaled first). System items (the Recycle Bin) have no
/// file to mark and are reported as failed; they stay where they are.
fn hide_by_attribute(state: &mut State, owner: &str, ids: &[String]) -> DesktopResult<HideReport> {
    // SAFETY: on the COM thread.
    let raw = unsafe { shell::list(&shell::desktop_view()?)? };
    let mut report = HideReport::default();
    let mut to_mark = Vec::new();
    for id in ids {
        if id.starts_with("::") {
            report.failed.push(id.clone());
            continue;
        }
        let Some(was_hidden) = shell::is_hidden_attribute(id) else {
            report.missing.push(id.clone());
            continue;
        };
        let at = raw.iter().find(|r| &r.id == id).map(|r| r.pos);
        // The file exists, which is all that is needed. The view may not have caught up with a new
        // file yet; marking it now means it never shows on the desktop at all.
        if state.journal.claim_attr(owner, id, at.unwrap_or(FALLBACK_ORIGIN), was_hidden) {
            to_mark.push(id.clone());
        }
        report.hidden.push(id.clone());
    }
    // Write-ahead: the journal says what to undo before any file is touched.
    state.save()?;
    for id in to_mark {
        if let Err(e) = shell::set_hidden_attribute(&id, true) {
            tracing::warn!("could not hide {id}: {e}");
            state.journal.unpark(owner, &id);
            report.hidden.retain(|h| h != &id);
            report.failed.push(id);
        }
    }
    state.save()?;
    Ok(report)
}

/// Undoes what was done to the items: the hidden attribute is cleared (unless the file had it
/// before us) and items an older build parked off-screen are moved back. The journal is changed only
/// after this succeeded, so a crash in the middle leaves it claiming *more* hidden items, never fewer.
fn restore_items(restore: &[ParkedItem]) -> DesktopResult<()> {
    for item in restore.iter().filter(|i| i.by_attribute && !i.attr_was_set) {
        match shell::set_hidden_attribute(&item.id, false) {
            Ok(()) | Err(DesktopError::NotFound(_)) => {}
            Err(e) => return Err(e),
        }
    }
    let parked: Vec<&ParkedItem> = restore.iter().filter(|i| !i.by_attribute).collect();
    if parked.is_empty() {
        return Ok(());
    }
    // SAFETY: on the COM thread.
    unsafe {
        let view = shell::desktop_view()?;
        let lv = shell::listview()?;
        let raw = shell::list(&view)?;
        for item in parked {
            // An item deleted meanwhile has nothing to restore.
            if let Some(found) = raw.iter().find(|r| r.id == item.id) {
                shell::set_position(lv, found.index, item.original)?;
            }
        }
    }
    Ok(())
}

fn show(state: &mut State, owner: &str, ids: &[String]) -> DesktopResult<()> {
    if state.surface.is_some() {
        let mut changed = false;
        if let Some(claimed) = state.claims.get_mut(owner) {
            for id in ids {
                changed |= claimed.remove(id);
            }
        }
        if changed {
            state.notifier.notify();
        }
        return Ok(());
    }
    let mut next = state.journal.clone();
    let restore: Vec<ParkedItem> = ids.iter().filter_map(|id| next.unpark_item(owner, id)).collect();
    restore_items(&restore)?;
    state.journal = next;
    state.save()
}

/// Half an icon: the cursor ends up over the middle of the icon, not at its corner.
const ICON_HALF: i32 = 32;

fn show_at(state: &mut State, owner: &str, id: &str, screen: Point) -> DesktopResult<()> {
    if state.surface.is_some() {
        // No native position to give it: the owner draws the item wherever it belongs.
        return show(state, owner, std::slice::from_ref(&id.to_string()));
    }
    let mut next = state.journal.clone();
    // Only when this was the last owner does the item come back at all.
    let restored = next.unpark_item(owner, id);
    if let Some(item) = &restored {
        restore_items(std::slice::from_ref(item))?;
    }
    state.journal = next;
    state.save()?;
    if restored.is_some_and(|i| i.by_attribute) {
        // The desktop shows the item again within a moment; put it where the user dropped it.
        // SAFETY: on the COM thread.
        unsafe { place_when_back(id, screen) };
    }
    Ok(())
}

/// Waits (briefly) for the desktop to show `id` again and moves it under `screen`. Best effort: if
/// the item does not show up the desktop keeps it where it was.
unsafe fn place_when_back(id: &str, screen: Point) {
    let started = std::time::Instant::now();
    while started.elapsed() < Duration::from_secs(3) {
        if let (Ok(view), Ok(lv)) = (shell::desktop_view(), shell::listview()) {
            if let Ok(raw) = shell::list(&view) {
                if let Some(item) = raw.iter().find(|r| r.id == id) {
                    let p = shell::screen_to_client(lv, screen);
                    let _ = shell::set_position(lv, item.index, Point { x: p.x - ICON_HALF, y: p.y - ICON_HALF });
                    return;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Gap between the icons of a block shown at once (screen pixels), and icons per row.
const BLOCK_STEP: (i32, i32) = (80, 90);
const BLOCK_PER_ROW: usize = 6;

/// Shows several items for `owner` at once and lays them out as one block from `screen`. The files
/// are restored in one pass, and the desktop view is waited for once, so the block appears together.
fn show_block(state: &mut State, owner: &str, ids: &[String], screen: Point) -> DesktopResult<()> {
    if state.surface.is_some() {
        return show(state, owner, ids);
    }
    let mut next = state.journal.clone();
    let restore: Vec<ParkedItem> = ids.iter().filter_map(|id| next.unpark_item(owner, id)).collect();
    restore_items(&restore)?;
    state.journal = next;
    state.save()?;

    let placed: Vec<String> = restore.iter().filter(|i| i.by_attribute).map(|i| i.id.clone()).collect();
    if placed.is_empty() {
        return Ok(());
    }
    // SAFETY: on the COM thread.
    unsafe {
        let lv = shell::listview()?;
        let origin = shell::screen_to_client(lv, screen);
        let started = std::time::Instant::now();
        loop {
            let raw = shell::list(&shell::desktop_view()?)?;
            let all_back = placed.iter().all(|id| raw.iter().any(|r| &r.id == id));
            if all_back || started.elapsed() >= Duration::from_secs(3) {
                for (n, id) in placed.iter().enumerate() {
                    let Some(item) = raw.iter().find(|r| &r.id == id) else { continue };
                    let spot = Point {
                        x: origin.x - ICON_HALF + (n % BLOCK_PER_ROW) as i32 * BLOCK_STEP.0,
                        y: origin.y - ICON_HALF + (n / BLOCK_PER_ROW) as i32 * BLOCK_STEP.1,
                    };
                    if let Err(e) = shell::set_position(lv, item.index, spot) {
                        tracing::warn!("could not place {id} in the block: {e}");
                    }
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
    }
    Ok(())
}

fn show_all(state: &mut State, owner: &str) -> DesktopResult<()> {
    if state.surface.is_some() {
        let had = state.claims.remove(owner).is_some_and(|c| !c.is_empty());
        if state.surface.as_deref() == Some(owner) {
            return set_surface(state, owner, false);
        }
        if had {
            state.notifier.notify();
        }
        return Ok(());
    }
    let mut next = state.journal.clone();
    let restore = next.release_owner(owner);
    restore_items(&restore)?;
    state.journal = next;
    state.save()
}

fn restore_all(state: &mut State) -> DesktopResult<()> {
    state.claims.clear();
    state.surface = None;
    let icons = restore_icons(&mut state.journal);
    if state.journal.items.is_empty() {
        if icons {
            state.save()?;
        }
        return Ok(());
    }
    restore_journal(&state.journal)?;
    state.journal.items.clear();
    state.save()
}

/// Puts back everything a journal says was hidden. Files marked hidden are cleared whether or not the
/// desktop lists them (it does not); items an older build parked are only moved if still there.
fn restore_journal(journal: &Journal) -> DesktopResult<usize> {
    let (by_attr, parked): (Vec<ParkedItem>, Vec<ParkedItem>) =
        journal.items.iter().cloned().partition(|i| i.by_attribute);
    restore_items(&by_attr)?;
    if parked.is_empty() {
        return Ok(by_attr.len());
    }
    // SAFETY: on the COM thread.
    let present: HashSet<String> = unsafe {
        let view = shell::desktop_view()?;
        shell::list(&view)?.into_iter().map(|r| r.id).collect()
    };
    let plan = plan_recovery(&Journal { items: parked, icons_hidden: false }, &present);
    restore_items(&plan.restore)?;
    Ok(by_attr.len() + plan.restore.len())
}

/// Switches the desktop's own icons back on if a run of ours switched them off. Returns whether
/// the journal changed.
fn restore_icons(journal: &mut Journal) -> bool {
    if !journal.icons_hidden {
        return false;
    }
    // SAFETY: on a COM thread.
    match unsafe { shell::set_icons_hidden(false) } {
        Ok(_) => {
            journal.icons_hidden = false;
            true
        }
        Err(e) => {
            tracing::warn!("could not switch the desktop icons back on: {e}");
            false
        }
    }
}

/// Surface mode on or off for `owner`; see [`DesktopLayer::set_surface`].
fn set_surface(state: &mut State, owner: &str, on: bool) -> DesktopResult<()> {
    if on {
        if state.surface.is_none() {
            // What was parked comes back, and whoever hid it keeps the claim.
            let parked = std::mem::take(&mut state.journal.items);
            restore_items(&parked)?;
            for item in &parked {
                for o in &item.owners {
                    state.claims.entry(o.clone()).or_default().insert(item.id.clone());
                }
            }
        }
        // SAFETY: on the COM thread. Also re-asserts the state if someone switched the icons on.
        let changed = unsafe { shell::set_icons_hidden(true)? };
        state.journal.icons_hidden |= changed;
        state.surface = Some(owner.to_string());
        state.save()?;
        state.notifier.notify();
        return Ok(());
    }
    if state.surface.as_deref() != Some(owner) {
        return Ok(());
    }
    if state.journal.icons_hidden {
        // SAFETY: on the COM thread.
        unsafe { shell::set_icons_hidden(false)? };
        state.journal.icons_hidden = false;
    }
    state.surface = None;
    state.save()?;
    // Whatever the owners hid goes back to being hidden on the desktop itself.
    for (claimant, ids) in std::mem::take(&mut state.claims) {
        let ids: Vec<String> = ids.into_iter().collect();
        if let Err(e) = hide_by_attribute(state, &claimant, &ids) {
            tracing::warn!("could not hide {} items for {claimant} after leaving surface mode: {e}", ids.len());
        }
    }
    state.notifier.notify();
    Ok(())
}

fn recover_file(path: &Path) -> DesktopResult<usize> {
    let mut journal = Journal::load(path);
    let restored = restore_journal(&journal)?;
    journal.items.clear();
    restore_icons(&mut journal);
    if journal.icons_hidden {
        // Could not switch them on: keep the file so the next start tries again.
        let _ = journal.save(path);
    } else {
        let _ = std::fs::remove_file(path);
    }
    Ok(restored)
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

    fn show_at(&self, id: &str, screen: Point) -> DesktopResult<()> {
        self.host.call(|reply| Command::ShowAt { owner: self.owner.clone(), id: id.to_string(), screen, reply })
    }

    fn show_block(&self, ids: &[String], screen: Point) -> DesktopResult<()> {
        self.host.call(|reply| Command::ShowBlock { owner: self.owner.clone(), ids: ids.to_vec(), screen, reply })
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

    fn set_surface(&self, on: bool) -> DesktopResult<()> {
        self.host.call(|reply| Command::SetSurface { owner: self.owner.clone(), on, reply })
    }

    fn surface_active(&self) -> bool {
        self.host.call(Command::SurfaceActive).unwrap_or(false)
    }

    fn subscribe_claims(&self, sink: ClaimSink) -> DesktopResult<Subscription> {
        Ok(self.host.notifier.add(sink))
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // A module that forgets to call show_all still never leaves its items parked.
        let _ = self.host.call(|reply| Command::ShowAll { owner: self.owner.clone(), reply });
    }
}
