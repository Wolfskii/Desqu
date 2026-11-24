//! Sort rules: which new desktop items go into which vial. Pure, so it is easy to test.

use std::collections::HashSet;
use std::path::Path;

use desqu_desktop::{DesktopItem, ItemKind};

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

/// What a rule may ask about the file behind an item: seconds since the Unix epoch, and bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Meta {
    pub modified: Option<u64>,
    pub created: Option<u64>,
    pub size: Option<u64>,
}

/// The facts of the file at `id`; nothing for a system item or a path that is gone. A folder has no size.
pub fn file_meta(id: &str) -> Meta {
    let Ok(m) = std::fs::metadata(id) else { return Meta::default() };
    let secs = |t: std::io::Result<std::time::SystemTime>| t.ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs());
    Meta { modified: secs(m.modified()), created: secs(m.created()), size: m.is_file().then(|| m.len()) }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

const DAY: u64 = 86_400;
const MB: u64 = 1_048_576;

impl ItemMatcher {
    pub fn matches(&self, item: &DesktopItem) -> bool {
        self.matches_with(item, now_secs(), &file_meta)
    }

    /// Like [`ItemMatcher::matches`] with the clock and the file facts given (so a test can say what they are).
    pub fn matches_with(&self, item: &DesktopItem, now: u64, meta: &dyn Fn(&str) -> Meta) -> bool {
        let age_days = |t: Option<u64>| t.map(|t| now.saturating_sub(t) / DAY);
        match self {
            ItemMatcher::ModifiedWithinDays(days) => age_days(meta(&item.id).modified).is_some_and(|a| a < u64::from(*days).max(1)),
            ItemMatcher::ModifiedOlderThanDays(days) => age_days(meta(&item.id).modified).is_some_and(|a| a >= u64::from(*days)),
            ItemMatcher::CreatedWithinDays(days) => age_days(meta(&item.id).created).is_some_and(|a| a < u64::from(*days).max(1)),
            ItemMatcher::SizeOverMb(mb) => meta(&item.id).size.is_some_and(|s| s > mb * MB),
            ItemMatcher::SizeUnderMb(mb) => meta(&item.id).size.is_some_and(|s| s < mb * MB),
            ItemMatcher::Any(ms) => ms.iter().any(|m| m.matches_with(item, now, meta)),
            ItemMatcher::All(ms) => !ms.is_empty() && ms.iter().all(|m| m.matches_with(item, now, meta)),
            _ => self.matches_name(item),
        }
    }

    fn matches_name(&self, item: &DesktopItem) -> bool {
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
            ItemMatcher::NameContains(text) => {
                !text.is_empty() && item.name.to_lowercase().contains(&text.trim().to_lowercase())
            }
            ItemMatcher::NameStartsWith(text) => {
                !text.is_empty() && item.name.to_lowercase().starts_with(&text.trim().to_lowercase())
            }
            ItemMatcher::NameEndsWith(text) => {
                !text.is_empty() && item.name.to_lowercase().ends_with(&text.trim().to_lowercase())
            }
            // The ones that need the file's facts are handled by `matches_with`.
            _ => false,
        }
    }
}

/// The kinds of condition a rule can be made of in the rules window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionKind {
    Contains,
    StartsWith,
    EndsWith,
    Pattern,
    Extension,
    Kind,
    ChangedWithin,
    NotChangedFor,
    CreatedWithin,
    BiggerThan,
    SmallerThan,
}

impl ConditionKind {
    pub const ALL: [ConditionKind; 11] = [
        ConditionKind::Contains,
        ConditionKind::StartsWith,
        ConditionKind::EndsWith,
        ConditionKind::Pattern,
        ConditionKind::Extension,
        ConditionKind::Kind,
        ConditionKind::ChangedWithin,
        ConditionKind::NotChangedFor,
        ConditionKind::CreatedWithin,
        ConditionKind::BiggerThan,
        ConditionKind::SmallerThan,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ConditionKind::Contains => "name contains",
            ConditionKind::StartsWith => "name starts with",
            ConditionKind::EndsWith => "name ends with",
            ConditionKind::Pattern => "name matches pattern",
            ConditionKind::Extension => "has the extension",
            ConditionKind::Kind => "is a",
            ConditionKind::ChangedWithin => "changed in the last (days)",
            ConditionKind::NotChangedFor => "not changed for (days)",
            ConditionKind::CreatedWithin => "created in the last (days)",
            ConditionKind::BiggerThan => "bigger than (MB)",
            ConditionKind::SmallerThan => "smaller than (MB)",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            ConditionKind::Contains => "Android",
            ConditionKind::StartsWith => "Report",
            ConditionKind::EndsWith => "Studio",
            ConditionKind::Pattern => "*.txt or Report*",
            ConditionKind::Extension => "pdf, docx",
            ConditionKind::Kind => "",
            ConditionKind::ChangedWithin | ConditionKind::NotChangedFor | ConditionKind::CreatedWithin => "7",
            ConditionKind::BiggerThan | ConditionKind::SmallerThan => "100",
        }
    }
}

pub fn kind_label(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Shortcut => "shortcut",
        ItemKind::Folder => "folder",
        ItemKind::App => "app",
        ItemKind::File => "file",
        ItemKind::Special => "system item",
    }
}

/// A condition as a matcher, or `None` while it is not filled in. Several extensions (`pdf, docx`)
/// make one matcher that accepts any of them.
pub fn build_matcher(kind: ConditionKind, text: &str, item_kind: ItemKind) -> Option<ItemMatcher> {
    let text = text.trim();
    let nonempty = |m: fn(String) -> ItemMatcher| (!text.is_empty()).then(|| m(text.to_string()));
    match kind {
        ConditionKind::Contains => nonempty(ItemMatcher::NameContains),
        ConditionKind::StartsWith => nonempty(ItemMatcher::NameStartsWith),
        ConditionKind::EndsWith => nonempty(ItemMatcher::NameEndsWith),
        ConditionKind::Pattern => nonempty(ItemMatcher::NameGlob),
        ConditionKind::Kind => Some(ItemMatcher::Kind(item_kind)),
        ConditionKind::ChangedWithin => text.parse().ok().map(ItemMatcher::ModifiedWithinDays),
        ConditionKind::NotChangedFor => text.parse().ok().map(ItemMatcher::ModifiedOlderThanDays),
        ConditionKind::CreatedWithin => text.parse().ok().map(ItemMatcher::CreatedWithinDays),
        ConditionKind::BiggerThan => text.parse().ok().map(ItemMatcher::SizeOverMb),
        ConditionKind::SmallerThan => text.parse().ok().map(ItemMatcher::SizeUnderMb),
        ConditionKind::Extension => {
            let mut list: Vec<String> =
                text.split(|c: char| c.is_whitespace() || c == ',' || c == ';').map(|x| x.trim_start_matches('.').to_lowercase()).filter(|x| !x.is_empty()).collect();
            list.dedup();
            match list.len() {
                0 => None,
                1 => Some(ItemMatcher::Extension(list.remove(0))),
                _ => Some(ItemMatcher::Any(list.into_iter().map(ItemMatcher::Extension).collect())),
            }
        }
    }
}

/// Conditions that must all hold: none gives `None`, one stays itself, several become `All`.
pub fn all_of(mut list: Vec<ItemMatcher>) -> Option<ItemMatcher> {
    match list.len() {
        0 => None,
        1 => list.pop(),
        _ => Some(ItemMatcher::All(list)),
    }
}

/// A matcher in words, for the list of rules.
pub fn describe(matcher: &ItemMatcher) -> String {
    match matcher {
        ItemMatcher::Extension(e) => format!("has the extension {e}"),
        ItemMatcher::Kind(k) => format!("is a {}", kind_label(*k)),
        ItemMatcher::NameGlob(p) => format!("name matches {p}"),
        ItemMatcher::NameContains(t) => format!("name contains {t}"),
        ItemMatcher::NameStartsWith(t) => format!("name starts with {t}"),
        ItemMatcher::NameEndsWith(t) => format!("name ends with {t}"),
        ItemMatcher::ModifiedWithinDays(d) => format!("changed in the last {d} days"),
        ItemMatcher::ModifiedOlderThanDays(d) => format!("not changed for {d} days"),
        ItemMatcher::CreatedWithinDays(d) => format!("created in the last {d} days"),
        ItemMatcher::SizeOverMb(mb) => format!("is bigger than {mb} MB"),
        ItemMatcher::SizeUnderMb(mb) => format!("is smaller than {mb} MB"),
        ItemMatcher::Any(list) => list.iter().map(describe).collect::<Vec<_>>().join(" or "),
        ItemMatcher::All(list) => list.iter().map(describe).collect::<Vec<_>>().join(" and "),
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
    #[test]
    fn age_and_size_conditions_use_the_files_facts_and_the_clock() {
        const NOW: u64 = 100 * 86_400;
        let meta = |id: &str| match id {
            "new" => Meta { modified: Some(NOW - 3_600), created: Some(NOW - 86_400 * 2), size: Some(5 * 1_048_576) },
            "old" => Meta { modified: Some(NOW - 86_400 * 40), created: Some(NOW - 86_400 * 90), size: Some(300 * 1_048_576) },
            "folder" => Meta { modified: Some(NOW), created: Some(NOW), size: None },
            _ => Meta::default(),
        };
        let at = |m: ItemMatcher, id: &str| m.matches_with(&item(id, id, ItemKind::File), NOW, &meta);
        assert!(at(ItemMatcher::ModifiedWithinDays(7), "new") && !at(ItemMatcher::ModifiedWithinDays(7), "old"));
        assert!(at(ItemMatcher::ModifiedOlderThanDays(30), "old") && !at(ItemMatcher::ModifiedOlderThanDays(30), "new"));
        assert!(at(ItemMatcher::CreatedWithinDays(3), "new") && !at(ItemMatcher::CreatedWithinDays(3), "old"));
        assert!(at(ItemMatcher::SizeOverMb(100), "old") && !at(ItemMatcher::SizeOverMb(100), "new"));
        assert!(at(ItemMatcher::SizeUnderMb(10), "new") && !at(ItemMatcher::SizeUnderMb(10), "old"));
        assert!(!at(ItemMatcher::SizeOverMb(0), "folder"), "a folder has no size");
        assert!(!at(ItemMatcher::ModifiedWithinDays(7), "gone"), "nothing is known about a missing file");
        let both = ItemMatcher::All(vec![ItemMatcher::Extension("zip".into()), ItemMatcher::ModifiedOlderThanDays(30)]);
        assert!(!both.matches_with(&item("old", "old", ItemKind::File), NOW, &meta), "the extension does not match");
    }

    #[test]
    fn the_new_conditions_are_built_from_numbers_and_described_in_words() {
        assert_eq!(build_matcher(ConditionKind::NotChangedFor, " 30 ", ItemKind::File), Some(ItemMatcher::ModifiedOlderThanDays(30)));
        assert_eq!(build_matcher(ConditionKind::BiggerThan, "x", ItemKind::File), None);
        assert_eq!(build_matcher(ConditionKind::SmallerThan, "", ItemKind::File), None);
        assert_eq!(describe(&ItemMatcher::SizeOverMb(100)), "is bigger than 100 MB");
        assert_eq!(describe(&ItemMatcher::ModifiedWithinDays(7)), "changed in the last 7 days");
    }

    use super::*;

    #[test]
    fn conditions_become_matchers_only_when_filled_in() {
        assert_eq!(build_matcher(ConditionKind::Contains, "  ", ItemKind::File), None);
        assert_eq!(build_matcher(ConditionKind::Contains, " Android ", ItemKind::File), Some(ItemMatcher::NameContains("Android".into())));
        assert_eq!(build_matcher(ConditionKind::Kind, "", ItemKind::Folder), Some(ItemMatcher::Kind(ItemKind::Folder)));
        assert_eq!(build_matcher(ConditionKind::Extension, ".PDF", ItemKind::File), Some(ItemMatcher::Extension("pdf".into())));
        assert_eq!(
            build_matcher(ConditionKind::Extension, "pdf, .docx;pdf", ItemKind::File),
            Some(ItemMatcher::Any(vec![ItemMatcher::Extension("pdf".into()), ItemMatcher::Extension("docx".into()), ItemMatcher::Extension("pdf".into())])),
            "adjacent duplicates only are dropped"
        );
        assert_eq!(build_matcher(ConditionKind::Extension, " , ", ItemKind::File), None);
    }

    #[test]
    fn matchers_are_described_in_words() {
        assert_eq!(describe(&ItemMatcher::Extension("pdf".into())), "has the extension pdf");
        assert_eq!(describe(&ItemMatcher::Kind(ItemKind::Folder)), "is a folder");
        let any = ItemMatcher::Any(vec![ItemMatcher::Extension("a".into()), ItemMatcher::NameContains("b".into())]);
        assert_eq!(describe(&any), "has the extension a or name contains b");
    }

    use crate::model::{NormalizedRect, Vial};
    use desqu_desktop::Point;

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
            sort_key: None,
            sort_desc: false,
            folders_first: false,
            locked: false,
            tint: None,
            color: None,
            icon_size: None,
            title_center: false,
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
    fn name_text_matchers_ignore_case_and_say_what_they_mean() {
        let app = item(r"C:\D\Android Studio.lnk", "Android Studio", ItemKind::Shortcut);
        assert!(ItemMatcher::NameContains("android".into()).matches(&app));
        assert!(ItemMatcher::NameStartsWith("ANDROID St".into()).matches(&app));
        assert!(!ItemMatcher::NameStartsWith("Studio".into()).matches(&app));
        assert!(ItemMatcher::NameEndsWith("studio".into()).matches(&app));
        assert!(!ItemMatcher::NameContains("".into()).matches(&app), "an empty text matches nothing");
        assert!(ItemMatcher::NameContains("roid".into()).matches(&app), "without any wildcard");
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

#[cfg(test)]
mod all_of_tests {
    use super::*;

    #[test]
    fn combines_conditions() {
        assert_eq!(all_of(vec![]), None);
        assert_eq!(all_of(vec![ItemMatcher::Extension("pdf".into())]), Some(ItemMatcher::Extension("pdf".into())));
        let both = all_of(vec![ItemMatcher::Extension("pdf".into()), ItemMatcher::NameContains("tax".into())]).unwrap();
        assert_eq!(describe(&both), "has the extension pdf and name contains tax");
    }
}
