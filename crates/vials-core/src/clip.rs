//! Clipboard rules for vials: what pasting files into a vial means. Pure, so it is unit tested.
//!
//! A vial only refers to files that live in a Desktop folder, so pasting into one is "bring the files
//! to the Desktop, then put them in the vial". Files cut on the Desktop already are there: pasting
//! them is just moving them between vials, no file is touched.

use std::collections::HashSet;

/// A path as a comparison key: case and slash direction do not matter on Windows.
pub fn key(path: &str) -> String {
    path.trim().replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

/// The folder a path sits in, as a comparison key (`""` for a root).
pub fn parent_key(path: &str) -> String {
    let k = key(path);
    k.rsplit_once('\\').map_or(String::new(), |(parent, _)| parent.to_string())
}

/// Which pasted files need a file operation and which are on the Desktop already.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PastePlan {
    /// Cut on the Desktop and pasted: they only change vial.
    pub adopt: Vec<String>,
    /// Copied from the Desktop itself: Windows names the copies "... - Copy".
    pub copy_here: Vec<String>,
    /// Anything else: moved (cut) or copied into the Desktop folder.
    pub transfer: Vec<String>,
}

/// `desktop_folders` are the folders the desktop shows (the user's Desktop and the Public Desktop).
pub fn plan_paste(paths: &[String], cut: bool, desktop_folders: &[String]) -> PastePlan {
    let folders: HashSet<String> = desktop_folders.iter().map(|f| key(f)).collect();
    let mut plan = PastePlan::default();
    for path in paths {
        let on_desktop = folders.contains(&parent_key(path));
        match (on_desktop, cut) {
            (true, true) => plan.adopt.push(path.clone()),
            (true, false) => plan.copy_here.push(path.clone()),
            (false, _) => plan.transfer.push(path.clone()),
        }
    }
    plan
}

/// The ids (as the desktop lists them) that any of `paths` stands for. Anything that is not a known id
/// is ignored, so a clipboard can never tie an arbitrary path to a vial.
pub fn matching_ids(paths: &[String], ids: &[String]) -> Vec<String> {
    let wanted: HashSet<String> = paths.iter().map(|p| key(p)).collect();
    ids.iter().filter(|id| wanted.contains(&key(id))).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    const DESK: &str = r"C:\Users\me\Desktop";
    const PUBLIC: &str = r"C:\Users\Public\Desktop";

    #[test]
    fn keys_ignore_case_slashes_and_a_trailing_backslash() {
        assert_eq!(key("C:/Users/Me/Desktop/Games\\"), r"c:\users\me\desktop\games");
        assert_eq!(parent_key(r"C:\Users\me\Desktop\a.txt"), r"c:\users\me\desktop");
        assert_eq!(parent_key("a.txt"), "");
    }

    #[test]
    fn files_cut_on_the_desktop_only_change_vial() {
        let paths = v(&[r"c:\users\ME\desktop\a.txt", r"C:\Users\Public\Desktop\b.lnk", r"D:\docs\c.txt"]);
        let plan = plan_paste(&paths, true, &v(&[DESK, PUBLIC]));
        assert_eq!(plan.adopt, v(&[r"c:\users\ME\desktop\a.txt", r"C:\Users\Public\Desktop\b.lnk"]));
        assert_eq!(plan.transfer, v(&[r"D:\docs\c.txt"]));
        assert!(plan.copy_here.is_empty());
    }

    #[test]
    fn files_copied_from_the_desktop_become_new_copies() {
        let paths = v(&[r"C:\Users\me\Desktop\a.txt", r"D:\docs\c.txt"]);
        let plan = plan_paste(&paths, false, &v(&[DESK]));
        assert!(plan.adopt.is_empty());
        assert_eq!(plan.copy_here, v(&[r"C:\Users\me\Desktop\a.txt"]));
        assert_eq!(plan.transfer, v(&[r"D:\docs\c.txt"]));
    }

    #[test]
    fn a_file_in_a_subfolder_of_the_desktop_is_not_on_the_desktop() {
        let plan = plan_paste(&v(&[r"C:\Users\me\Desktop\Games\x.txt"]), true, &v(&[DESK]));
        assert!(plan.adopt.is_empty());
        assert_eq!(plan.transfer.len(), 1);
    }

    #[test]
    fn only_known_ids_are_picked_whatever_the_case_or_slashes() {
        let ids = v(&[r"C:\Users\me\Desktop\Notes.txt", r"C:\Users\me\Desktop\Games"]);
        let pasted = v(&["c:/users/me/desktop/notes.TXT", r"D:\elsewhere\file.txt"]);
        assert_eq!(matching_ids(&pasted, &ids), v(&[r"C:\Users\me\Desktop\Notes.txt"]));
        assert!(matching_ids(&[], &ids).is_empty());
    }
}
