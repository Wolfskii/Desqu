//! Tidy a folder (Downloads, say): the files that have been there a while go into folders by kind (Images,
//! Documents, Videos...), and every move is written down so the last tidy can be taken back. Files that are
//! still being written, folders, and files that are too new are never touched. What goes where is pure and
//! tested; the moves are plain renames inside the same folder tree.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use vials_core::search::Kind;

/// A file is left alone until it has not changed for this long (seconds).
pub const SETTLED_SECS: u64 = 15 * 60;

/// Names that mean a download that is not finished.
const UNFINISHED: &[&str] = &["crdownload", "part", "tmp", "download", "partial", "opdownload"];

/// The subfolder a kind of file goes into.
pub fn folder_of(kind: Kind) -> &'static str {
    match kind {
        Kind::Images => "Images",
        Kind::Documents => "Documents",
        Kind::Videos => "Videos",
        Kind::Audio => "Audio",
        Kind::Archives => "Archives",
        Kind::Programs => "Programs",
        Kind::Shortcuts => "Shortcuts",
        Kind::Folders => "Folders",
    }
}

/// One thing in the folder: its name, whether it is a folder, and how many seconds ago it last changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub age_secs: u64,
}

/// `(file name, subfolder)` for every file that should move. Files of a kind that has no folder, unfinished
/// downloads, folders and files that changed lately stay where they are.
pub fn plan(entries: &[Entry], settled_secs: u64) -> Vec<(String, &'static str)> {
    entries
        .iter()
        .filter(|e| !e.is_dir && e.age_secs >= settled_secs)
        .filter_map(|e| {
            let ext = Path::new(&e.name).extension()?.to_string_lossy().to_lowercase();
            if UNFINISHED.contains(&ext.as_str()) {
                return None;
            }
            // Shortcuts are the user's own placing on a desktop-like folder: they stay.
            let kind = Kind::of_extension(&ext).filter(|k| *k != Kind::Shortcuts && *k != Kind::Folders)?;
            Some((e.name.clone(), folder_of(kind)))
        })
        .collect()
}

/// The folder that is tidied unless another one is chosen: the user's Downloads.
pub fn default_folder() -> PathBuf {
    std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default().join("Downloads")
}

/// A move that was done: where the file was and where it went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Moved {
    pub from: PathBuf,
    pub to: PathBuf,
}

/// The file name `stem.ext` in `dir` that is not taken: `a.pdf`, `a (2).pdf`...
fn free_name(dir: &Path, name: &str) -> PathBuf {
    let path = Path::new(name);
    let (stem, ext) = (path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(), path.extension().map(|e| e.to_string_lossy().into_owned()));
    let candidate = |n: u32| match (&ext, n) {
        (Some(e), 1) => format!("{stem}.{e}"),
        (Some(e), n) => format!("{stem} ({n}).{e}"),
        (None, 1) => stem.clone(),
        (None, n) => format!("{stem} ({n})"),
    };
    (1..).map(|n| dir.join(candidate(n))).find(|p| !p.exists()).unwrap_or_else(|| dir.join(name))
}

/// What is in the folder now, as the planner wants it.
pub fn read_folder(folder: &Path) -> Vec<Entry> {
    let now = std::time::SystemTime::now();
    std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let age_secs = meta.modified().ok().and_then(|m| now.duration_since(m).ok()).map_or(0, |d| d.as_secs());
            Some(Entry { name: e.file_name().to_string_lossy().into_owned(), is_dir: meta.is_dir(), age_secs })
        })
        .collect()
}

/// Moves the files the plan names into their folders (made when needed) and returns what was moved.
pub fn tidy(folder: &Path, settled_secs: u64) -> Vec<Moved> {
    let mut done = Vec::new();
    for (name, sub) in plan(&read_folder(folder), settled_secs) {
        let target_dir = folder.join(sub);
        if std::fs::create_dir_all(&target_dir).is_err() {
            continue;
        }
        let (from, to) = (folder.join(&name), free_name(&target_dir, &name));
        if std::fs::rename(&from, &to).is_ok() {
            done.push(Moved { from, to });
        }
    }
    done
}

/// Puts back what a tidy moved, as far as the files are still where it left them. Returns how many came back.
pub fn undo(moves: &[Moved]) -> usize {
    moves
        .iter()
        .rev()
        .filter(|m| m.to.is_file() && !m.from.exists() && m.from.parent().is_some_and(|p| p.is_dir()) && std::fs::rename(&m.to, &m.from).is_ok())
        .count()
}

fn journal_path() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default().join("io.github.wolfskii.desqu").join("tidy-journal.json")
}

pub fn save_journal(moves: &[Moved]) {
    if let Some(dir) = journal_path().parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string(moves) {
        let _ = std::fs::write(journal_path(), text);
    }
}

pub fn load_journal() -> Vec<Moved> {
    std::fs::read_to_string(journal_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, age: u64) -> Entry {
        Entry { name: name.into(), is_dir: false, age_secs: age }
    }

    #[test]
    fn old_finished_files_go_by_kind_and_everything_else_stays() {
        let old = SETTLED_SECS + 1;
        let entries = [
            entry("photo.JPG", old),
            entry("paper.pdf", old),
            entry("movie.mkv", old),
            entry("setup.exe", old),
            entry("just-now.png", 5),
            entry("big.zip.crdownload", old),
            entry("notes", old),
            entry("link.lnk", old),
            entry("data.xyz", old),
            Entry { name: "Some folder".into(), is_dir: true, age_secs: old },
        ];
        assert_eq!(plan(&entries, SETTLED_SECS), vec![("photo.JPG".to_string(), "Images"), ("paper.pdf".into(), "Documents"), ("movie.mkv".into(), "Videos"), ("setup.exe".into(), "Programs")]);
    }

    #[test]
    fn a_tidy_moves_into_folders_keeps_names_free_and_can_be_undone() {
        let dir = std::env::temp_dir().join(format!("desqu-tidy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Images")).unwrap();
        std::fs::write(dir.join("a.png"), "new").unwrap();
        std::fs::write(dir.join("Images").join("a.png"), "already there").unwrap();
        std::fs::write(dir.join("b.pdf"), "doc").unwrap();
        // With no waiting time everything counts as settled.
        let moves = tidy(&dir, 0);
        assert_eq!(moves.len(), 2);
        assert_eq!(std::fs::read_to_string(dir.join("Images").join("a.png")).unwrap(), "already there", "an existing file is never overwritten");
        assert_eq!(std::fs::read_to_string(dir.join("Images").join("a (2).png")).unwrap(), "new");
        assert!(dir.join("Documents").join("b.pdf").is_file() && !dir.join("b.pdf").exists());
        // Undo puts them back.
        assert_eq!(undo(&moves), 2);
        assert!(dir.join("a.png").is_file() && dir.join("b.pdf").is_file());
        assert!(dir.join("Images").join("a.png").is_file(), "the file that was there before stays");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_journal_round_trips_as_text() {
        let moves = vec![Moved { from: PathBuf::from(r"C:\d\a.png"), to: PathBuf::from(r"C:\d\Images\a.png") }];
        let back: Vec<Moved> = serde_json::from_str(&serde_json::to_string(&moves).unwrap()).unwrap();
        assert_eq!(back, moves);
    }
}
