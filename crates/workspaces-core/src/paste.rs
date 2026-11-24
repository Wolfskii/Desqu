//! Turning "files cut on the desktop, pasted on another desktop" into item ids. Pure.

fn key(path: &str) -> String {
    path.trim().replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

/// The ids (as the desktop lists them) of the pasted `paths` that are items on the desktop. Case and
/// slash direction do not matter; anything that is not a desktop item is ignored, so pasting files
/// from elsewhere can never tie an arbitrary path to a desktop.
pub fn desktop_items_in(paths: &[String], item_ids: &[String]) -> Vec<String> {
    let wanted: std::collections::HashSet<String> = paths.iter().map(|p| key(p)).collect();
    item_ids.iter().filter(|id| wanted.contains(&key(id))).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn only_desktop_items_are_picked_whatever_the_case_or_slashes() {
        let items = v(&[r"C:\Users\me\Desktop\Notes.txt", r"C:\Users\me\Desktop\Games", r"C:\Users\Public\Desktop\Tool.lnk"]);
        let pasted = v(&["c:/users/me/desktop/notes.TXT", r"C:\Users\me\Desktop\Games\", r"D:\elsewhere\file.txt"]);
        assert_eq!(desktop_items_in(&pasted, &items), v(&[r"C:\Users\me\Desktop\Notes.txt", r"C:\Users\me\Desktop\Games"]));
    }

    #[test]
    fn nothing_matches_nothing() {
        assert!(desktop_items_in(&[], &v(&["a"])).is_empty());
        assert!(desktop_items_in(&v(&["a"]), &[]).is_empty());
    }
}
