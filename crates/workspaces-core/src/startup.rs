//! Opening a desktop's apps and folders the first time you come to it. Pure.

use std::collections::HashSet;

use super::model::{vial_of, Workspace};

/// At most this many things are opened at once.
pub const LIMIT: usize = 8;

/// What to open for a visit to `ws`: its items that are real files, folders or shortcuts (not a vial and not a
/// system item), at most [`LIMIT`], in the order the desktop lists them.
pub fn items_to_open(ws: &Workspace) -> Vec<String> {
    ws.items.iter().filter(|i| !i.starts_with("::") && vial_of(i).is_none()).take(LIMIT).cloned().collect()
}

/// Notes a visit to a desktop. Returns whether this is the first one in this session (`opened` is what was
/// visited so far) and the desktop asks for its items to be opened.
pub fn first_visit(opened: &mut HashSet<String>, ws: Option<&Workspace>) -> bool {
    match ws {
        Some(ws) if ws.open_on_visit => opened.insert(ws.desktop_id.clone()),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(on: bool, items: &[&str]) -> Workspace {
        Workspace { desktop_id: "{A}".into(), name: "Work".into(), items: items.iter().map(|s| s.to_string()).collect(), open_on_visit: on, wallpaper: None }
    }

    #[test]
    fn only_real_files_and_folders_are_opened_and_not_too_many() {
        let many: Vec<String> = (0..12).map(|n| format!("C:/d/{n}.lnk")).collect();
        let refs: Vec<&str> = many.iter().map(String::as_str).collect();
        assert_eq!(items_to_open(&ws(true, &refs)).len(), LIMIT);
        let mixed = ws(true, &["C:/d/app.lnk", "vial:vial-1", "::{645FF040-5081-101B-9F08-00AA002F954E}", "C:/d/folder"]);
        assert_eq!(items_to_open(&mixed), vec!["C:/d/app.lnk", "C:/d/folder"]);
    }

    #[test]
    fn the_first_visit_in_a_session_opens_and_later_ones_do_not() {
        let mut seen = HashSet::new();
        let on = ws(true, &["C:/d/a.lnk"]);
        assert!(first_visit(&mut seen, Some(&on)));
        assert!(!first_visit(&mut seen, Some(&on)), "only once");
        assert!(!first_visit(&mut HashSet::new(), Some(&ws(false, &[]))), "the desktop did not ask");
        assert!(!first_visit(&mut HashSet::new(), None), "no workspace for the desktop");
    }
}
