//! Sort rules: which new desktop items go into which vial. Pure, so it is easy to test.

use std::collections::HashSet;
use std::path::Path;

use desqu_desktop::DesktopItem;

use super::model::{ItemMatcher, SortRule, VialsConfig};

/// `*` (any run of characters) and `?` (one character), case-insensitive.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (None::<usize>, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

impl ItemMatcher {
    pub fn matches(&self, item: &DesktopItem) -> bool {
        match self {
            ItemMatcher::Extension(ext) => {
                let wanted = ext.trim().trim_start_matches('.');
                !wanted.is_empty()
                    && Path::new(&item.id)
                        .extension()
                        .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case(wanted))
            }
            ItemMatcher::Kind(kind) => item.kind == *kind,
            ItemMatcher::NameGlob(pattern) => !pattern.is_empty() && glob_match(pattern, &item.name),
            ItemMatcher::Any(ms) => ms.iter().any(|m| m.matches(item)),
            ItemMatcher::All(ms) => !ms.is_empty() && ms.iter().all(|m| m.matches(item)),
        }
    }
}

/// The first enabled rule that matches `item` and points at a vial that exists.
pub fn first_match<'a>(cfg: &'a VialsConfig, item: &DesktopItem) -> Option<&'a SortRule> {
    cfg.rules
        .iter()
        .find(|r| r.enabled && cfg.vial(&r.target_vial).is_some_and(|v| !v.inbox) && r.matcher.matches(item))
}

/// `(item id, vial id)` for every desktop item that is in no vial yet and matches a rule.
/// Items already in a vial, and items the user took out on purpose (`declined`), are never
/// touched.
pub fn assign_new_items(cfg: &VialsConfig, items: &[DesktopItem]) -> Vec<(String, String)> {
    if !cfg.auto_sort {
        return Vec::new();
    }
    let declined: HashSet<&str> = cfg.declined.iter().map(String::as_str).collect();
    items
        .iter()
        .filter(|item| cfg.vial_of(&item.id).is_none() && !declined.contains(item.id.as_str()))
        .filter_map(|item| first_match(cfg, item).map(|rule| (item.id.clone(), rule.target_vial.clone())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{NormalizedRect, Vial};
    use desqu_desktop::{ItemKind, Point};

    fn item(id: &str, name: &str, kind: ItemKind) -> DesktopItem {
        DesktopItem {
            id: id.into(),
            name: name.into(),
            kind,
            position: Point { x: 0, y: 0 },
            parked: false,
        }
    }

    fn vial(id: &str, items: &[&str]) -> Vial {
        Vial {
            id: id.into(),
            name: id.into(),
            monitor: None,
            rect: NormalizedRect { x: 0.0, y: 0.0, width: 0.2, height: 0.3 },
            items: items.iter().map(|s| s.to_string()).collect(),
            collapsed: false,
            inbox: false,
        }
    }

    fn rule(id: &str, matcher: ItemMatcher, target: &str) -> SortRule {
        SortRule { id: id.into(), name: id.into(), enabled: true, matcher, target_vial: target.into() }
    }

    #[test]
    fn glob_basics() {
        assert!(glob_match("*.pdf", "report.PDF"));
        assert!(glob_match("report*", "Report final"));
        assert!(glob_match("r?port", "report"));
        assert!(glob_match("*", ""));
        assert!(glob_match("a*b*c", "aXXbYYc"));
        assert!(!glob_match("*.pdf", "report.docx"));
        assert!(!glob_match("r?port", "rport"));
        assert!(!glob_match("abc", "abcd"));
    }

    #[test]
    fn extension_matching_ignores_case_and_a_leading_dot() {
        let pdf = item(r"C:\D\Report.PDF", "Report.PDF", ItemKind::File);
        assert!(ItemMatcher::Extension("pdf".into()).matches(&pdf));
        assert!(ItemMatcher::Extension(".pdf".into()).matches(&pdf));
        assert!(!ItemMatcher::Extension("doc".into()).matches(&pdf));
        assert!(!ItemMatcher::Extension("".into()).matches(&pdf));
    }

    #[test]
    fn kind_name_any_and_all() {
        let app = item(r"C:\D\Tool.lnk", "Tool", ItemKind::Shortcut);
        assert!(ItemMatcher::Kind(ItemKind::Shortcut).matches(&app));
        assert!(!ItemMatcher::Kind(ItemKind::Folder).matches(&app));
        assert!(ItemMatcher::NameGlob("to*".into()).matches(&app));
        assert!(ItemMatcher::Any(vec![ItemMatcher::Kind(ItemKind::Folder), ItemMatcher::NameGlob("Tool".into())]).matches(&app));
        assert!(!ItemMatcher::All(vec![ItemMatcher::Kind(ItemKind::Shortcut), ItemMatcher::NameGlob("x*".into())]).matches(&app));
        assert!(!ItemMatcher::All(vec![]).matches(&app), "an empty All must not match everything");
    }

    #[test]
    fn the_first_enabled_rule_with_a_real_target_wins() {
        let mut cfg = VialsConfig { vials: vec![vial("docs", &[]), vial("apps", &[])], ..VialsConfig::default() };
        let mut disabled = rule("off", ItemMatcher::Extension("pdf".into()), "apps");
        disabled.enabled = false;
        cfg.rules = vec![
            disabled,
            rule("ghost", ItemMatcher::Extension("pdf".into()), "gone"),
            rule("pdfs", ItemMatcher::Extension("pdf".into()), "docs"),
            rule("later", ItemMatcher::Extension("pdf".into()), "apps"),
        ];
        let pdf = item(r"C:\D\a.pdf", "a.pdf", ItemKind::File);
        assert_eq!(first_match(&cfg, &pdf).unwrap().id, "pdfs");
    }

    #[test]
    fn only_unplaced_items_are_assigned_and_declined_ones_stay_out() {
        let mut cfg = VialsConfig { vials: vec![vial("docs", &["placed"])], ..VialsConfig::default() };
        cfg.rules = vec![rule("pdfs", ItemMatcher::Extension("pdf".into()), "docs")];
        let items = vec![
            item("placed", "placed.pdf", ItemKind::File),
            item("new.pdf", "new.pdf", ItemKind::File),
            item("declined.pdf", "declined.pdf", ItemKind::File),
            item("other.txt", "other.txt", ItemKind::File),
        ];
        cfg.declined = vec!["declined.pdf".to_string()];
        let assigned = assign_new_items(&cfg, &items);
        assert_eq!(assigned, vec![("new.pdf".to_string(), "docs".to_string())]);

        cfg.auto_sort = false;
        assert!(assign_new_items(&cfg, &items).is_empty());
    }
}
