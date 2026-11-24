//! Folder portals: a vial that shows a folder. Listing a folder and moving around inside it. The listing is
//! plain `std::fs`, so it works for any folder Windows can open (local, network share, a synced cloud folder).

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use super::sort::natural_cmp;

/// The source of a portal that shows the files opened lately (Windows' own Recent list) instead of a folder.
pub const RECENT: &str = "shell:recent";

/// How many recent files the portal shows.
pub const RECENT_LIMIT: usize = 50;

/// The folder Windows keeps the shortcuts to recent files in.
pub fn recent_folder() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default().join("Microsoft").join("Windows").join("Recent")
}

/// What a portal shows for `dir`: the folder's entries, or for [`RECENT`] the recent files.
pub fn list(dir: &Path) -> Result<Vec<String>, String> {
    if dir == Path::new(RECENT) {
        return list_recent(&recent_folder(), RECENT_LIMIT);
    }
    list_dir(dir)
}

/// The shortcuts in `folder` (a Recent folder), newest first, at most `limit`. Folders of the Recent list
/// (Windows keeps `AutomaticDestinations` there) are not shown.
pub fn list_recent(folder: &Path, limit: usize) -> Result<Vec<String>, String> {
    let mut entries: Vec<(std::time::SystemTime, String)> = Vec::new();
    for entry in std::fs::read_dir(folder).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("lnk")) {
            continue;
        }
        let modified = entry.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
        entries.push((modified, path.to_string_lossy().into_owned()));
    }
    entries.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(entries.into_iter().take(limit).map(|e| e.1).collect())
}

/// The entries of `dir` that a person would see in Explorer: folders first, then files, each by name
/// the way Explorer orders them ("file2" before "file10"). Hidden and system entries and `desktop.ini`
/// are left out. Entries that cannot be read are skipped; an unreadable folder is an error.
pub fn list_dir(dir: &Path) -> Result<Vec<String>, String> {
    let mut entries: Vec<(bool, String, String)> = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.eq_ignore_ascii_case("desktop.ini") || name.eq_ignore_ascii_case("thumbs.db") {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if is_hidden(&meta) {
            continue;
        }
        entries.push((meta.is_dir(), name, entry.path().to_string_lossy().into_owned()));
    }
    entries.sort_by(|a, b| match (a.0, b.0) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => natural_cmp(&a.1, &b.1),
    });
    Ok(entries.into_iter().map(|e| e.2).collect())
}

#[cfg(windows)]
fn is_hidden(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    meta.file_attributes() & (HIDDEN | SYSTEM) != 0
}

#[cfg(not(windows))]
fn is_hidden(_meta: &std::fs::Metadata) -> bool {
    false
}

/// Whether `dir` is `root` or somewhere below it.
pub fn is_inside(root: &Path, dir: &Path) -> bool {
    let (root, dir) = (normal(root), normal(dir));
    dir == root || dir.starts_with(&root)
}

/// The folder one level up from `dir`, but never above `root` (`None` at the root).
pub fn up(root: &Path, dir: &Path) -> Option<PathBuf> {
    if normal(dir) == normal(root) || !is_inside(root, dir) {
        return None;
    }
    dir.parent().map(Path::to_path_buf)
}

/// A name for a new portal: the folder's own name, or the whole path for a drive root.
pub fn title_of(folder: &Path) -> String {
    if folder == Path::new(RECENT) {
        return "Recent files".to_string();
    }
    folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| folder.to_string_lossy().into_owned())
}

/// A path compared the way Windows compares: lower case, one kind of slash, no trailing slash.
fn normal(p: &Path) -> PathBuf {
    PathBuf::from(p.to_string_lossy().to_lowercase().replace('/', "\\").trim_end_matches('\\').to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_recent_list_is_shortcuts_newest_first_and_cut_at_the_limit() {
        let dir = temp("recent");
        let now = std::time::SystemTime::now();
        for (name, age_secs) in [("old.lnk", 300u64), ("new.lnk", 10), ("mid.lnk", 100), ("not a link.txt", 1)] {
            let file = std::fs::File::create(dir.join(name)).unwrap();
            file.set_modified(now - std::time::Duration::from_secs(age_secs)).unwrap();
        }
        std::fs::create_dir(dir.join("AutomaticDestinations")).unwrap();
        let names = |v: Vec<String>| v.iter().map(|p| title_of(Path::new(p))).collect::<Vec<_>>();
        assert_eq!(names(list_recent(&dir, 50).unwrap()), vec!["new.lnk", "mid.lnk", "old.lnk"]);
        assert_eq!(names(list_recent(&dir, 2).unwrap()), vec!["new.lnk", "mid.lnk"]);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(title_of(Path::new(RECENT)), "Recent files");
    }

    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("desqu-portal-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_folder_lists_folders_first_then_files_in_natural_order_without_the_clutter() {
        let dir = temp("list");
        std::fs::create_dir(dir.join("zeta")).unwrap();
        std::fs::create_dir(dir.join("Alpha")).unwrap();
        for f in ["file10.txt", "file2.txt", "desktop.ini", "b.txt"] {
            std::fs::write(dir.join(f), "x").unwrap();
        }
        let names: Vec<String> = list_dir(&dir).unwrap().iter().map(|p| title_of(Path::new(p))).collect();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(names, vec!["Alpha", "zeta", "b.txt", "file2.txt", "file10.txt"]);
    }

    #[test]
    fn an_unreadable_folder_is_an_error() {
        assert!(list_dir(Path::new(r"C:\no\such\folder\here")).is_err());
    }

    #[test]
    fn a_portal_never_goes_above_its_folder() {
        let root = Path::new(r"C:\Data\Projects");
        assert!(is_inside(root, Path::new("c:/data/projects/a/b")));
        assert!(!is_inside(root, Path::new(r"C:\Data\Other")));
        assert!(!is_inside(root, Path::new(r"C:\Data\ProjectsX")), "a name that only starts the same is another folder");
        assert_eq!(up(root, Path::new(r"C:\Data\Projects\a\b")), Some(PathBuf::from(r"C:\Data\Projects\a")));
        assert_eq!(up(root, Path::new(r"C:\Data\Projects\a")), Some(PathBuf::from(r"C:\Data\Projects")));
        assert_eq!(up(root, root), None, "at the top");
        assert_eq!(up(root, Path::new(r"C:\Elsewhere")), None);
    }

    #[test]
    fn a_portal_is_named_after_its_folder() {
        assert_eq!(title_of(Path::new(r"C:\Data\Projects")), "Projects");
        assert_eq!(title_of(Path::new("D:\\")), "D:\\");
    }
}
