//! Runtime of Workspaces: the shared desktop layer and one worker thread that makes the desktop icons
//! match the workspace of the current virtual desktop. Event driven: the worker sleeps until the
//! virtual desktop changes, the desktop folders change, or a request arrives, and looks once more a
//! few seconds later because Explorer's view lags the file system.
//!
//! The data decisions live in `workspaces-core` (which icons to hide); this connects them to the
//! desktop, the disk and Windows' virtual desktops. The list of desktops is injectable so the engine
//! can be tested on a machine with one virtual desktop.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use desqu_desktop::{DesktopError, DesktopLayer, Subscription};
use workspaces_core::model::{desktop_label, WorkspacesConfig};
use workspaces_core::{paste, visibility};
use workspaces_platform::{CutPasteWatcher, Desktops, SwitchWatcher};

/// The desktop view shows a new item several seconds after the file appears.
const LAG_RECHECK: Duration = Duration::from_secs(5);
const RETRY_DELAY: Duration = Duration::from_secs(3);
const MAX_RETRIES: u8 = 4;

type DesktopsFn = Box<dyn Fn() -> Desktops + Send + Sync>;

enum Job {
    Reconcile,
    Switched,
    DesktopChanged,
    Pasted(Vec<String>),
    Stop,
}

/// One row of the window: a virtual desktop (or a workspace whose desktop is gone).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEntry {
    pub id: String,
    pub name: String,
    pub current: bool,
    pub items: Vec<String>,
    pub removed: bool,
}

pub struct Engine {
    layer: Arc<dyn DesktopLayer>,
    config: Mutex<WorkspacesConfig>,
    path: PathBuf,
    desktops: DesktopsFn,
    known: Mutex<Option<HashSet<String>>>,
    problem: Mutex<Option<String>>,
    /// Counts passes over the desktop; the window redraws when it changes.
    generation: AtomicU64,
    jobs: Mutex<Option<Sender<Job>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    _subscriptions: Mutex<Vec<Subscription>>,
    _paste: Mutex<Option<CutPasteWatcher>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Engine {
    /// Takes the desktop layer (only one process may own it) and starts watching. `desktops` tells
    /// which virtual desktops exist (the real one: `workspaces_platform::desktops`).
    pub fn start(path: &Path, desktops: DesktopsFn) -> Result<Arc<Self>, String> {
        let journal = path.with_file_name("desktop-journal.json");
        // Items a previous run left hidden: put them back first.
        if let Err(e) = desqu_desktop::recover_from(&journal) {
            eprintln!("could not recover desktop items: {e}");
        }
        let layer = desqu_desktop::open("workspaces", &journal).map_err(|e| match e {
            DesktopError::OwnedElsewhere => "another Desqu app is arranging the desktop".to_string(),
            other => other.to_string(),
        })?;
        let config = match WorkspacesConfig::load(path) {
            Ok(c) => c,
            Err(e) => {
                // Never overwrite a config that could not be read: keep it aside.
                let backup = path.with_extension("json.broken");
                let _ = std::fs::rename(path, &backup);
                eprintln!("workspaces config unreadable ({e}); moved to {} and using defaults", backup.display());
                WorkspacesConfig::default()
            }
        };
        let (jobs, rx) = mpsc::channel();
        let engine = Arc::new(Self {
            layer: layer.clone(),
            config: Mutex::new(config),
            path: path.to_path_buf(),
            desktops,
            known: Mutex::new(None),
            problem: Mutex::new(None),
            generation: AtomicU64::new(0),
            jobs: Mutex::new(Some(jobs.clone())),
            worker: Mutex::new(None),
            _subscriptions: Mutex::new(Vec::new()),
            _paste: Mutex::new(None),
        });

        let mut subs = Vec::new();
        let sink = Mutex::new(jobs.clone());
        subs.push(layer.subscribe(Box::new(move |_| drop(lock(&sink).send(Job::DesktopChanged)))).map_err(|e| e.to_string())?);
        let sink = Mutex::new(jobs.clone());
        subs.push(layer.subscribe_claims(Box::new(move || drop(lock(&sink).send(Job::Reconcile)))).map_err(|e| e.to_string())?);
        *lock(&engine._subscriptions) = subs;
        // Not fatal: without the hook everything works except moving icons with Ctrl+X / Ctrl+V.
        let sink = Mutex::new(jobs.clone());
        *lock(&engine._paste) = workspaces_platform::watch_cut_paste(Box::new(move |paths| drop(lock(&sink).send(Job::Pasted(paths)))))
            .map_err(|e| eprintln!("could not watch Ctrl+V on the desktop: {e}"))
            .ok();

        let worker = {
            let engine = engine.clone();
            std::thread::Builder::new().name("workspaces-engine".into()).spawn(move || engine.worker(rx, jobs)).map_err(|e| e.to_string())?
        };
        *lock(&engine.worker) = Some(worker);
        engine.request();
        Ok(engine)
    }

    /// Ends the worker and puts every hidden item back.
    pub fn stop(&self) {
        if let Some(jobs) = lock(&self.jobs).take() {
            let _ = jobs.send(Job::Stop);
        }
        if let Some(worker) = lock(&self.worker).take() {
            let _ = worker.join();
        }
        if let Err(e) = self.layer.show_all() {
            eprintln!("could not restore hidden desktop items: {e}");
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    pub fn problem(&self) -> Option<String> {
        lock(&self.problem).clone()
    }

    pub fn config(&self) -> WorkspacesConfig {
        lock(&self.config).clone()
    }

    /// Looks at the desktop again soon.
    pub fn request(&self) {
        if let Some(jobs) = lock(&self.jobs).as_ref() {
            let _ = jobs.send(Job::Reconcile);
        }
    }

    /// Changes the config, saves it and has the desktop follow.
    fn change(&self, edit: impl FnOnce(&mut WorkspacesConfig) -> bool) -> bool {
        let changed = {
            let mut cfg = lock(&self.config);
            if !edit(&mut cfg) {
                return false;
            }
            if let Err(e) = cfg.save(&self.path) {
                eprintln!("could not save the config: {e}");
            }
            true
        };
        self.request();
        changed
    }

    pub fn set_auto_assign(&self, on: bool) -> bool {
        self.change(|c| {
            let changed = c.auto_assign_new != on;
            c.auto_assign_new = on;
            changed
        })
    }

    /// First run: every desktop item nobody owns yet goes to one virtual desktop (or, with `None`, stays on
    /// every desktop); either way the question counts as answered.
    pub fn answer_first_run(&self, desktop_id: Option<&str>) -> Result<(), String> {
        let target = match desktop_id {
            None => None,
            Some(id) => Some(self.desktop_named(id)?),
        };
        let ids: Vec<String> = match &target {
            Some(_) => self.layer.items().map_err(|e| e.to_string())?.into_iter().map(|i| i.id).collect(),
            None => Vec::new(),
        };
        self.change(|c| {
            if let Some((id, name)) = &target {
                for item in &ids {
                    if c.owner_of(item).is_none() {
                        c.assign(item, Some((id, name)));
                    }
                }
            }
            c.initialized = true;
            true
        });
        Ok(())
    }

    /// Forgets a desktop's workspace: its items show on every desktop again.
    pub fn release_desktop(&self, desktop_id: &str) -> bool {
        self.change(|c| c.remove_workspace(desktop_id))
    }

    /// Ties one desktop item to a virtual desktop, or with `None` shows it on every desktop.
    pub fn assign_item(&self, item_id: &str, desktop_id: Option<&str>) -> Result<bool, String> {
        let target = match desktop_id {
            None => None,
            Some(id) => Some(self.desktop_named(id)?),
        };
        Ok(self.change(|c| c.assign(item_id, target.as_ref().map(|(id, name)| (id.as_str(), name.as_str())))))
    }

    /// The desktop items no virtual desktop owns (they show everywhere), by name.
    pub fn shared_items(&self) -> Vec<String> {
        let cfg = self.config();
        let mut ids: Vec<String> = self.layer.items().map(|items| items.into_iter().map(|i| i.id).collect()).unwrap_or_default();
        ids.retain(|id| cfg.owner_of(id).is_none());
        ids.sort_by_key(|id| id.rsplit(['\\', '/']).next().unwrap_or(id).to_lowercase());
        ids
    }

    fn desktop_named(&self, id: &str) -> Result<(String, String), String> {
        let desktops = (self.desktops)();
        let index = desktops.list.iter().position(|d| d.id == id).ok_or("that virtual desktop does not exist")?;
        Ok((id.to_string(), desktop_label(index, &desktops.list[index].name)))
    }

    /// Every virtual desktop (in Windows' order) plus workspaces whose desktop no longer exists.
    pub fn entries(&self) -> (Vec<DesktopEntry>, bool) {
        let desktops = (self.desktops)();
        let cfg = self.config();
        let mut out: Vec<DesktopEntry> = desktops
            .list
            .iter()
            .enumerate()
            .map(|(i, d)| DesktopEntry {
                id: d.id.clone(),
                name: desktop_label(i, &d.name),
                current: desktops.current.as_deref() == Some(d.id.as_str()),
                items: cfg.workspace(&d.id).map(|w| w.items.clone()).unwrap_or_default(),
                removed: false,
            })
            .collect();
        for ws in cfg.workspaces.iter().filter(|w| !desktops.list.iter().any(|d| d.id == w.desktop_id)) {
            out.push(DesktopEntry {
                id: ws.desktop_id.clone(),
                name: if ws.name.is_empty() { "Removed desktop".into() } else { ws.name.clone() },
                current: false,
                items: ws.items.clone(),
                removed: true,
            });
        }
        (out, !desktops.list.is_empty())
    }

    // ---------------------------------------------------------------------------- the worker

    fn worker(&self, rx: Receiver<Job>, jobs: Sender<Job>) {
        let mut watcher: Option<SwitchWatcher> = None;
        let mut later: Option<Instant> = None;
        let mut retries = 0u8;
        loop {
            // The registry key only exists once Windows has more than one desktop: keep trying.
            if watcher.is_none() {
                let sink = Mutex::new(jobs.clone());
                watcher = workspaces_platform::watch_switches(Arc::new(move || drop(lock(&sink).send(Job::Switched)))).ok();
            }
            let received = match later {
                Some(t) => rx.recv_timeout(t.saturating_duration_since(Instant::now())),
                None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
            };
            let now = Instant::now();
            match received {
                Ok(Job::Stop) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(Job::Reconcile) | Ok(Job::Switched) => {}
                Ok(Job::Pasted(paths)) => self.adopt_pasted(&paths),
                Ok(Job::DesktopChanged) => {
                    retries = 0;
                    later = Some(now + LAG_RECHECK);
                }
                Err(RecvTimeoutError::Timeout) => later = None,
            }
            let lagging = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.reconcile())).unwrap_or(true);
            if lagging && retries < MAX_RETRIES && later.is_none() {
                retries += 1;
                later = Some(now + RETRY_DELAY);
            }
        }
    }

    /// Makes the hidden icons match the current virtual desktop. Returns whether the desktop view could
    /// not be read (worth another look shortly).
    pub fn reconcile(&self) -> bool {
        let items = match self.layer.items() {
            Ok(items) => items,
            Err(e) => {
                *lock(&self.problem) = Some(e.to_string());
                self.generation.fetch_add(1, Ordering::Relaxed);
                return true;
            }
        };
        let ids: Vec<String> = items.iter().map(|i| i.id.clone()).collect();
        let desktops = (self.desktops)();
        let live: Vec<String> = desktops.list.iter().map(|d| d.id.clone()).collect();
        let current = desktops.current.clone();

        // A file that appeared since the last look belongs to the desktop you are on.
        let fresh: Vec<String> = match lock(&self.known).as_ref() {
            Some(seen) => ids.iter().filter(|id| !seen.contains(*id)).cloned().collect(),
            None => Vec::new(),
        };
        lock(&self.known).get_or_insert_with(HashSet::new).extend(ids.iter().cloned());
        if !fresh.is_empty() && self.config().auto_assign_new {
            if let Some(cur) = current.as_deref() {
                let index = desktops.list.iter().position(|d| d.id == cur).unwrap_or(0);
                let name = desktop_label(index, desktops.list.get(index).map_or("", |d| d.name.as_str()));
                // Only files nobody owns yet: a restored or re-listed item keeps its owner. Ghost entries of
                // deleted files are not given a desktop.
                self.change(|cfg| {
                    let mut changed = false;
                    for id in fresh.iter().filter(|id| id.starts_with("::") || Path::new(id).exists()) {
                        if cfg.owner_of(id).is_none() {
                            changed |= cfg.assign(id, Some((cur, name.as_str())));
                        }
                    }
                    changed
                });
            }
        }

        let hidden = self.layer.hidden().unwrap_or_default();
        let cfg = self.config();
        let plan = visibility::plan(&ids, &hidden, &cfg, current.as_deref(), &live);

        let mut problem = None;
        if !plan.show.is_empty() {
            if let Err(e) = self.layer.show(&plan.show) {
                problem = Some(e.to_string());
            }
        }
        let mut lagging = false;
        // Claimed as hidden but found on screen (a park that failed or was undone): hide it again.
        let mut to_hide = plan.hide.clone();
        to_hide.extend(items.iter().filter(|i| !i.parked && hidden.contains(&i.id) && !plan.show.contains(&i.id)).map(|i| i.id.clone()));
        if !to_hide.is_empty() {
            match self.layer.hide(&to_hide) {
                Ok(report) => {
                    lagging = !report.missing.is_empty() || !report.failed.is_empty();
                    if !report.failed.is_empty() {
                        // Retrying changes nothing: system items have no file to mark hidden, and Windows
                        // refuses the Public Desktop without administrator rights.
                        problem = Some(format!(
                            "{} item(s) cannot be hidden on other desktops (system items such as the Recycle Bin, or files Windows will not let Workspaces mark hidden)",
                            report.failed.len()
                        ));
                        lagging = !report.missing.is_empty();
                    }
                }
                Err(e) => problem = Some(e.to_string()),
            }
        }
        *lock(&self.problem) = problem;
        self.generation.fetch_add(1, Ordering::Relaxed);
        lagging
    }

    /// Files were cut on the desktop and Ctrl+V was pressed on a desktop: they now belong to the virtual
    /// desktop you are on (and are hidden on the others). Explorer itself does nothing for a cut-and-paste
    /// into the folder the files are already in.
    fn adopt_pasted(&self, paths: &[String]) {
        let desktops = (self.desktops)();
        let Some(current) = desktops.current.clone() else { return };
        let Ok(items) = self.layer.items() else { return };
        let ids: Vec<String> = items.into_iter().map(|i| i.id).collect();
        let picked = paste::desktop_items_in(paths, &ids);
        if picked.is_empty() {
            return;
        }
        let index = desktops.list.iter().position(|d| d.id == current).unwrap_or(0);
        let name = desktop_label(index, desktops.list.get(index).map_or("", |d| d.name.as_str()));
        self.change(|cfg| picked.iter().fold(false, |c, id| cfg.assign(id, Some((&current, &name))) | c));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use workspaces_platform::DesktopInfo;

    const A: &str = "{AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA}";
    const B: &str = "{BBBBBBBB-BBBB-BBBB-BBBB-BBBBBBBBBBBB}";

    fn hidden(p: &Path) -> bool {
        use std::os::windows::fs::MetadataExt;
        std::fs::metadata(p).map(|m| m.file_attributes() & 0x2 != 0).unwrap_or(false)
    }

    fn two_desktops(current: &'static std::sync::atomic::AtomicBool) -> DesktopsFn {
        // `current` true = desktop A is the one you are on, false = B.
        Box::new(move || Desktops {
            list: vec![DesktopInfo { id: A.into(), name: String::new() }, DesktopInfo { id: B.into(), name: "Play".into() }],
            current: Some(if current.load(Ordering::Relaxed) { A } else { B }.to_string()),
        })
    }

    /// Needs the real desktop and no other Desqu app running; creates throw-away files on the Desktop
    /// (removed again). `cargo test --release switching -- --ignored`.
    #[test]
    #[ignore = "uses the real desktop"]
    fn switching_desktops_hides_what_belongs_elsewhere_and_shows_it_back() {
        static ON_A: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
        let desktop = PathBuf::from(std::env::var_os("USERPROFILE").unwrap()).join("Desktop");
        let mine = desktop.join(format!("zz-workspaces-a-{}.txt", std::process::id()));
        let theirs = desktop.join(format!("zz-workspaces-b-{}.txt", std::process::id()));
        std::fs::write(&mine, "a").unwrap();
        std::fs::write(&theirs, "b").unwrap();
        let dir = std::env::temp_dir().join(format!("workspaces-engine-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg_path = dir.join("config.json");
        let mut cfg = WorkspacesConfig::default();
        cfg.assign(&mine.to_string_lossy(), Some((A, "Desktop 1")));
        cfg.assign(&theirs.to_string_lossy(), Some((B, "Play")));
        cfg.initialized = true;
        cfg.save(&cfg_path).unwrap();

        let result = std::panic::catch_unwind(|| {
            let engine = Engine::start(&cfg_path, two_desktops(&ON_A)).expect("engine");
            // The desktop notices new files after a moment.
            for _ in 0..120 {
                engine.reconcile();
                if hidden(&theirs) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(500));
            }
            assert!(hidden(&theirs), "an item of the other desktop is hidden");
            assert!(!hidden(&mine), "an item of this desktop stays");

            // Switch to the other desktop: the pictures swap.
            ON_A.store(false, Ordering::Relaxed);
            for _ in 0..40 {
                engine.reconcile();
                if hidden(&mine) && !hidden(&theirs) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            assert!(hidden(&mine) && !hidden(&theirs), "after the switch the other desktop's item shows and this one's hides");

            // Releasing a desktop's workspace shows its items everywhere.
            assert!(engine.release_desktop(A));
            for _ in 0..40 {
                engine.reconcile();
                if !hidden(&mine) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            assert!(!hidden(&mine));
            let (entries, any) = engine.entries();
            // One item can be tied to a desktop by hand, and shows everywhere again when released.
            let key = mine.to_string_lossy().to_string();
            assert!(engine.assign_item(&key, Some(B)).unwrap());
            assert_eq!(engine.config().owner_of(&key), Some(B));
            assert!(engine.assign_item("x", Some("{CCCCCCCC-CCCC-CCCC-CCCC-CCCCCCCCCCCC}")).is_err());
            assert!(engine.assign_item(&key, None).unwrap());
            assert!(engine.shared_items().contains(&key));
            assert!(any && entries.len() == 2 && entries[1].current && entries[1].items.len() == 1, "{entries:?}");
            engine.stop();
            assert!(!hidden(&theirs), "stopping puts everything back");
        });
        for p in [&mine, &theirs] {
            let _ = std::process::Command::new("attrib").arg("-h").arg(p).status();
            let _ = std::fs::remove_file(p);
        }
        let _ = std::fs::remove_dir_all(&dir);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }
}
