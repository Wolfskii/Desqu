//! Which desktop icons to hide or show for the current virtual desktop. Pure.

use std::collections::{HashMap, HashSet};

use super::model::WorkspacesConfig;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub hide: Vec<String>,
    pub show: Vec<String>,
}

/// `items`: ids on the desktop now. `hidden_by_us`: ids this module currently hides.
/// `current`: the current virtual desktop. `live`: every virtual desktop that exists.
///
/// An item owned by another live desktop is hidden. Everything else is shown: items nobody owns,
/// items of this desktop, and items whose desktop was removed (hiding them forever would lose
/// them). If the current desktop is unknown nothing is hidden.
pub fn plan(
    items: &[String],
    hidden_by_us: &[String],
    cfg: &WorkspacesConfig,
    current: Option<&str>,
    live: &[String],
) -> Plan {
    let live: HashSet<&str> = live.iter().map(String::as_str).collect();
    let owner: HashMap<&str, &str> = cfg
        .workspaces
        .iter()
        .filter(|w| live.contains(w.desktop_id.as_str()))
        .flat_map(|w| {
            w.items
                .iter()
                .map(move |i| (i.as_str(), w.desktop_id.as_str()))
        })
        .collect();

    let should_hide = |id: &str| current.is_some_and(|c| owner.get(id).is_some_and(|o| *o != c));
    let already: HashSet<&str> = hidden_by_us.iter().map(String::as_str).collect();

    let mut show: Vec<String> = hidden_by_us
        .iter()
        .filter(|id| !should_hide(id))
        .cloned()
        .collect();
    show.sort();
    let mut hide: Vec<String> = items
        .iter()
        .filter(|id| should_hide(id) && !already.contains(id.as_str()))
        .cloned()
        .collect();
    hide.sort();
    hide.dedup();
    Plan { hide, show }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "{753C9B0D-EDAA-45D2-AC5A-5740141905EA}";
    const B: &str = "{3FEBEE14-E0E2-4DCE-A63D-09646A16F375}";

    fn v(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    fn cfg() -> WorkspacesConfig {
        let mut c = WorkspacesConfig::default();
        c.assign("work.docx", Some((A, "Work")));
        c.assign("game.lnk", Some((B, "Play")));
        c
    }

    #[test]
    fn items_of_other_desktops_are_hidden_and_the_rest_stays_visible() {
        let items = v(&["work.docx", "game.lnk", "free.txt"]);
        let on_a = plan(&items, &[], &cfg(), Some(A), &v(&[A, B]));
        assert_eq!(
            on_a,
            Plan {
                hide: v(&["game.lnk"]),
                show: vec![]
            }
        );
        let on_b = plan(&items, &[], &cfg(), Some(B), &v(&[A, B]));
        assert_eq!(
            on_b,
            Plan {
                hide: v(&["work.docx"]),
                show: vec![]
            }
        );
    }

    #[test]
    fn switching_swaps_what_is_hidden() {
        let items = v(&["work.docx", "game.lnk", "free.txt"]);
        let p = plan(&items, &v(&["game.lnk"]), &cfg(), Some(B), &v(&[A, B]));
        assert_eq!(
            p,
            Plan {
                hide: v(&["work.docx"]),
                show: v(&["game.lnk"])
            }
        );
    }

    #[test]
    fn nothing_is_hidden_when_the_current_desktop_is_unknown() {
        let items = v(&["work.docx", "game.lnk"]);
        let p = plan(&items, &v(&["game.lnk"]), &cfg(), None, &v(&[A, B]));
        assert_eq!(
            p,
            Plan {
                hide: vec![],
                show: v(&["game.lnk"])
            }
        );
    }

    #[test]
    fn items_of_a_removed_desktop_come_back() {
        let items = v(&["work.docx", "game.lnk"]);
        // Desktop B no longer exists: its items must not stay hidden forever.
        let p = plan(&items, &v(&["game.lnk"]), &cfg(), Some(A), &v(&[A]));
        assert_eq!(
            p,
            Plan {
                hide: vec![],
                show: v(&["game.lnk"])
            }
        );
    }

    #[test]
    fn already_hidden_items_are_left_alone() {
        let items = v(&["work.docx", "game.lnk"]);
        let p = plan(&items, &v(&["game.lnk"]), &cfg(), Some(A), &v(&[A, B]));
        assert_eq!(p, Plan::default());
    }
}
