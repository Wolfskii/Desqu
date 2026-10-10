//! Which desktop icons to hide or show for the current virtual desktop. Pure.

use std::collections::HashSet;

use super::model::Workspace;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub hide: Vec<String>,
    pub show: Vec<String>,
}

/// `items`: ids on the desktop now. `hidden_by_us`: ids this module currently hides.
/// `workspace`: the workspace of the current virtual desktop, if it has one.
///
/// Without a workspace, or with icon management off, everything this module hid comes back.
/// Otherwise exactly the listed items are hidden: items hidden before but no longer listed come
/// back, and listed items missing from the desktop are simply skipped (they may reappear).
pub fn plan(items: &[String], hidden_by_us: &[String], workspace: Option<&Workspace>) -> Plan {
    let wanted: HashSet<&str> = match workspace {
        Some(ws) if ws.manage_icons => ws.hidden_items.iter().map(String::as_str).collect(),
        _ => HashSet::new(),
    };
    let on_desktop: HashSet<&str> = items.iter().map(String::as_str).collect();
    let already: HashSet<&str> = hidden_by_us.iter().map(String::as_str).collect();

    let mut show: Vec<String> = hidden_by_us
        .iter()
        .filter(|id| !wanted.contains(id.as_str()))
        .cloned()
        .collect();
    show.sort();
    let mut hide: Vec<String> = items
        .iter()
        .filter(|id| wanted.contains(id.as_str()) && !already.contains(id.as_str()))
        .cloned()
        .collect();
    hide.sort();
    hide.dedup();
    // Only ids that exist on the desktop can be hidden; keep `on_desktop` honest for callers.
    hide.retain(|id| on_desktop.contains(id.as_str()));
    Plan { hide, show }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(manage: bool, hidden: &[&str]) -> Workspace {
        let mut w = Workspace::new("{753C9B0D-EDAA-45D2-AC5A-5740141905EA}", "w");
        w.manage_icons = manage;
        w.hidden_items = hidden.iter().map(|s| s.to_string()).collect();
        w
    }

    fn v(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn hides_the_listed_items_that_exist() {
        let p = plan(
            &v(&["a", "b", "c"]),
            &[],
            Some(&ws(true, &["a", "c", "gone"])),
        );
        assert_eq!(
            p,
            Plan {
                hide: v(&["a", "c"]),
                show: vec![]
            }
        );
    }

    #[test]
    fn already_hidden_items_are_left_alone_and_unlisted_ones_come_back() {
        let p = plan(
            &v(&["b", "c"]),
            &v(&["a", "b"]),
            Some(&ws(true, &["b", "c"])),
        );
        assert_eq!(
            p,
            Plan {
                hide: v(&["c"]),
                show: v(&["a"])
            }
        );
    }

    #[test]
    fn no_workspace_or_management_off_shows_everything_back() {
        for w in [None, Some(ws(false, &["a"]))] {
            let p = plan(&v(&["a", "b"]), &v(&["a"]), w.as_ref());
            assert_eq!(
                p,
                Plan {
                    hide: vec![],
                    show: v(&["a"])
                }
            );
        }
    }

    #[test]
    fn new_files_stay_visible() {
        let p = plan(&v(&["a", "new.txt"]), &[], Some(&ws(true, &["a"])));
        assert_eq!(p.hide, v(&["a"]));
    }
}
