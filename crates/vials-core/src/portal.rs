//! Folder portals: a vial that shows a folder. Listing a folder and moving around inside it. The listing is
//! plain `std::fs`, so it works for any folder Windows can open (local, network share, a synced cloud folder).

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use super::sort::natural_cmp;

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
    folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| folder.to_string_lossy().into_owned())
}

/// A path compared the way Windows compares: lower case, one kind of slash, no trailing slash.
fn normal(p: &Path) -> PathBuf {
    PathBuf::from(p.to_string_lossy().to_lowercase().replace('/', "\\").trim_end_matches('\\').to_string())
}

#[cfg(test)]
mod tests {
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
