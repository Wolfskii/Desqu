//! What the "Groups, rules and browser" window decides, as pure functions on the config: groups of apps,
//! the rules that say what happens to a group's windows, and the browser's per-site fullscreen rules.

use screenbound_module::core::browser::{normalize_host, FullscreenMode, SitePattern, SiteRule};
use screenbound_module::core::config::{AppConfig, AppGroup};
use screenbound_module::core::rules::{Action, Matcher, RuleScope, WindowRule};
use screenbound_module::core::window::ChromeMode;

const COLORS: [&str; 6] = ["#5b9cff", "#47c28a", "#f2a65a", "#e06c8b", "#a78bfa", "#4fd1d9"];

fn fresh_id(prefix: &str, taken: impl Fn(&str) -> bool) -> String {
    (1..).map(|n| format!("{prefix}-{n}")).find(|id| !taken(id)).unwrap_or_default()
}

// ---------------------------------------------------------------------------------- groups

/// A new group with the next colour. Returns its id, or `None` for an empty name or one already used.
pub fn add_group(cfg: &mut AppConfig, name: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty() || cfg.groups.iter().any(|g| g.name.eq_ignore_ascii_case(name)) {
        return None;
    }
    let id = fresh_id("group", |id| cfg.groups.iter().any(|g| g.id == id));
    let color = COLORS[cfg.groups.len() % COLORS.len()].to_string();
    cfg.groups.push(AppGroup { id: id.clone(), name: name.to_string(), color, icon: "apps".into() });
    Some(id)
}

/// Removes a group: its apps are left without one and rules for it stop listing it.
pub fn delete_group(cfg: &mut AppConfig, id: &str) -> bool {
    let before = cfg.groups.len();
    cfg.groups.retain(|g| g.id != id);
    for app in &mut cfg.known_apps {
        if app.group_id.as_deref() == Some(id) {
            app.group_id = None;
        }
    }
    for rule in &mut cfg.rules {
        if let Some(RuleScope::Groups { group_ids }) = &mut rule.scope {
            group_ids.retain(|g| g != id);
        }
    }
    cfg.groups.len() != before
}

/// Moves an app on to the next group (none -> first -> ... -> last -> none). Returns whether the app exists.
pub fn cycle_app_group(cfg: &mut AppConfig, process_name: &str) -> bool {
    let ids: Vec<String> = cfg.groups.iter().map(|g| g.id.clone()).collect();
    let Some(app) = cfg.known_apps.iter_mut().find(|a| a.process_name.eq_ignore_ascii_case(process_name)) else { return false };
    app.group_id = match app.group_id.as_deref().and_then(|g| ids.iter().position(|i| i == g)) {
        None => ids.first().cloned(),
        Some(i) => ids.get(i + 1).cloned(),
    };
    true
}

// ---------------------------------------------------------------------------------- rules

/// Which apps a new rule covers, as the window offers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    All,
    Group(String),
}

pub fn scope_label(cfg: &AppConfig, scope: &RuleScope) -> String {
    match scope {
        RuleScope::All => "every app that is on".into(),
        RuleScope::Groups { group_ids } => {
            let names: Vec<&str> = group_ids.iter().filter_map(|id| cfg.groups.iter().find(|g| &g.id == id).map(|g| g.name.as_str())).collect();
            if names.is_empty() { "a group that is gone".into() } else { format!("group {}", names.join(", ")) }
        }
        RuleScope::Apps { process_names } => format!("{} app(s)", process_names.len()),
    }
}

/// A rule that keeps fullscreen windows of the chosen apps inside a zone. Returns its id.
pub fn add_rule(cfg: &mut AppConfig, name: &str, scope: &Scope, zone_id: &str, chrome: ChromeMode) -> Option<String> {
    let name = name.trim();
    if name.is_empty() || !cfg.zones.iter().any(|z| z.id == zone_id) {
        return None;
    }
    let scope = match scope {
        Scope::All => RuleScope::All,
        Scope::Group(id) if cfg.groups.iter().any(|g| &g.id == id) => RuleScope::Groups { group_ids: vec![id.clone()] },
        Scope::Group(_) => return None,
    };
    let id = fresh_id("rule", |id| cfg.rules.iter().any(|r| r.id == id));
    cfg.rules.push(WindowRule {
        id: id.clone(),
        name: name.to_string(),
        enabled: true,
        matcher: Matcher::Any(vec![]),
        scope: Some(scope),
        actions: vec![Action::FullscreenZone { zone_id: zone_id.to_string(), chrome, use_snapped_zone: true }],
    });
    Some(id)
}

pub fn delete_rule(cfg: &mut AppConfig, id: &str) -> bool {
    let before = cfg.rules.len();
    cfg.rules.retain(|r| r.id != id);
    cfg.rules.len() != before
}

pub fn set_rule_enabled(cfg: &mut AppConfig, id: &str, enabled: bool) -> bool {
    match cfg.rules.iter_mut().find(|r| r.id == id) {
        Some(r) if r.enabled != enabled => {
            r.enabled = enabled;
            true
        }
        _ => false,
    }
}

/// What a rule does, in words.
pub fn rule_summary(cfg: &AppConfig, rule: &WindowRule) -> String {
    let who = rule.scope.as_ref().map_or_else(|| "windows matched by hand-made conditions".to_string(), |s| scope_label(cfg, s));
    let what: Vec<String> = rule
        .actions
        .iter()
        .map(|a| match a {
            Action::FullscreenZone { zone_id, chrome, .. } => {
                let zone = cfg.zones.iter().find(|z| &z.id == zone_id).map_or("a zone that is gone", |z| z.name.as_str());
                format!("fullscreen goes into {zone}{}", if *chrome == ChromeMode::Hide { " without title bar" } else { "" })
            }
            _ => "makes a windowed app borderless".to_string(),
        })
        .collect();
    format!("{who}: {}", what.join("; "))
}

// ---------------------------------------------------------------------------------- browser

pub fn mode_label(mode: FullscreenMode) -> &'static str {
    match mode {
        FullscreenMode::Native => "browser's own fullscreen",
        FullscreenMode::Fullscreen => "whole screen",
        FullscreenMode::Window => "fill the window",
        FullscreenMode::Zone => "keep inside the zone",
    }
}

pub const MODES: [FullscreenMode; 4] = [FullscreenMode::Native, FullscreenMode::Fullscreen, FullscreenMode::Window, FullscreenMode::Zone];

pub fn pattern_text(p: &SitePattern) -> String {
    match p {
        SitePattern::Host(h) => h.clone(),
        SitePattern::Domain(d) => format!("*.{d}"),
        SitePattern::UrlPrefix(u) => u.clone(),
    }
}

/// The site rules changed: stamp the time so the browser extension takes this copy over its own.
fn touch(cfg: &mut AppConfig, now_ms: u64) {
    cfg.browser.site_rules.updated_at = now_ms;
}

pub fn set_integration(cfg: &mut AppConfig, on: bool) -> bool {
    let changed = cfg.browser.integration_enabled != on;
    cfg.browser.integration_enabled = on;
    changed
}

pub fn set_default_mode(cfg: &mut AppConfig, mode: FullscreenMode, now_ms: u64) -> bool {
    if cfg.browser.site_rules.default_mode == mode {
        return false;
    }
    cfg.browser.site_rules.default_mode = mode;
    touch(cfg, now_ms);
    true
}

/// A rule for one site (`youtube.com`, `*.vimeo.com`, or a full address). Returns whether it was added.
pub fn add_site_rule(cfg: &mut AppConfig, text: &str, mode: FullscreenMode, zone_id: Option<&str>, now_ms: u64) -> bool {
    let text = text.trim();
    // An address with a path after the host is a prefix; a bare host (with or without scheme) is a host.
    let has_path = text.split_once("://").is_some_and(|(_, rest)| rest.split_once('/').is_some_and(|(_, path)| !path.is_empty()));
    let pattern = if has_path {
        SitePattern::UrlPrefix(text.to_string())
    } else if text.starts_with("*.") {
        SitePattern::Domain(normalize_host(text))
    } else {
        SitePattern::Host(normalize_host(text))
    };
    if pattern_text(&pattern).trim_start_matches("*.").is_empty() {
        return false;
    }
    let rules = &mut cfg.browser.site_rules.rules;
    if rules.iter().any(|r| r.pattern == pattern) {
        return false;
    }
    let id = fresh_id("site", |id| rules.iter().any(|r| r.id == id));
    let zone_id = (mode == FullscreenMode::Zone).then(|| zone_id.map(str::to_string)).flatten();
    rules.push(SiteRule { id, enabled: true, pattern, mode, zone_id });
    touch(cfg, now_ms);
    true
}

pub fn delete_site_rule(cfg: &mut AppConfig, id: &str, now_ms: u64) -> bool {
    let before = cfg.browser.site_rules.rules.len();
    cfg.browser.site_rules.rules.retain(|r| r.id != id);
    let changed = cfg.browser.site_rules.rules.len() != before;
    if changed {
        touch(cfg, now_ms);
    }
    changed
}

pub fn set_site_rule_enabled(cfg: &mut AppConfig, id: &str, enabled: bool, now_ms: u64) -> bool {
    let changed = match cfg.browser.site_rules.rules.iter_mut().find(|r| r.id == id) {
        Some(r) if r.enabled != enabled => {
            r.enabled = enabled;
            true
        }
        _ => false,
    };
    if changed {
        touch(cfg, now_ms);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenbound_module::core::config::KnownApp;

    fn app(name: &str) -> KnownApp {
        KnownApp { process_name: name.into(), title: String::new(), enabled: true, group_id: None }
    }

    #[test]
    fn groups_are_added_cycled_and_deleted_with_their_traces() {
        let mut cfg = AppConfig::default();
        let a = add_group(&mut cfg, "Games").unwrap();
        let b = add_group(&mut cfg, "Video").unwrap();
        assert!(add_group(&mut cfg, " games ").is_none() && add_group(&mut cfg, "").is_none());
        cfg.known_apps.push(app("a.exe"));
        assert!(cycle_app_group(&mut cfg, "A.EXE"));
        assert_eq!(cfg.known_apps[0].group_id.as_deref(), Some(a.as_str()));
        cycle_app_group(&mut cfg, "a.exe");
        assert_eq!(cfg.known_apps[0].group_id.as_deref(), Some(b.as_str()));
        cycle_app_group(&mut cfg, "a.exe");
        assert_eq!(cfg.known_apps[0].group_id, None);
        assert!(!cycle_app_group(&mut cfg, "missing.exe"));

        cycle_app_group(&mut cfg, "a.exe");
        let zone = cfg.zones[0].id.clone();
        add_rule(&mut cfg, "Games rule", &Scope::Group(a.clone()), &zone, ChromeMode::Keep).unwrap();
        assert!(delete_group(&mut cfg, &a));
        assert_eq!(cfg.known_apps[0].group_id, None);
        assert_eq!(cfg.rules.last().unwrap().scope, Some(RuleScope::Groups { group_ids: vec![] }));
        assert!(!delete_group(&mut cfg, &a));
    }

    #[test]
    fn rules_are_added_switched_and_described() {
        let mut cfg = AppConfig::default();
        let zone = cfg.zones[0].id.clone();
        let g = add_group(&mut cfg, "Games").unwrap();
        assert!(add_rule(&mut cfg, "", &Scope::All, &zone, ChromeMode::Keep).is_none());
        assert!(add_rule(&mut cfg, "x", &Scope::All, "nope", ChromeMode::Keep).is_none());
        assert!(add_rule(&mut cfg, "x", &Scope::Group("nope".into()), &zone, ChromeMode::Keep).is_none());
        let id = add_rule(&mut cfg, "Games in the zone", &Scope::Group(g), &zone, ChromeMode::Hide).unwrap();
        let rule = cfg.rules.iter().find(|r| r.id == id).unwrap();
        assert_eq!(rule_summary(&cfg, rule), "group Games: fullscreen goes into 75% Left without title bar");
        assert!(set_rule_enabled(&mut cfg, &id, false) && !set_rule_enabled(&mut cfg, &id, false));
        assert!(delete_rule(&mut cfg, &id) && !delete_rule(&mut cfg, &id));
        assert!(cfg.clone().validated().is_ok());
    }

    #[test]
    fn site_rules_normalise_dedupe_and_stamp_the_time() {
        let mut cfg = AppConfig::default();
        assert!(add_site_rule(&mut cfg, "https://www.YouTube.com/", FullscreenMode::Zone, Some("zone-left-75"), 100));
        assert!(add_site_rule(&mut cfg, "*.vimeo.com", FullscreenMode::Window, Some("ignored"), 200));
        assert!(!add_site_rule(&mut cfg, "WWW.youtube.com", FullscreenMode::Window, None, 300), "same site twice");
        assert!(!add_site_rule(&mut cfg, "  ", FullscreenMode::Window, None, 300));
        let rules = &cfg.browser.site_rules.rules;
        assert_eq!(pattern_text(&rules[0].pattern), "www.youtube.com");
        assert_eq!(rules[0].zone_id.as_deref(), Some("zone-left-75"));
        assert_eq!((pattern_text(&rules[1].pattern), rules[1].zone_id.clone()), ("*.vimeo.com".to_string(), None));
        assert_eq!(cfg.browser.site_rules.updated_at, 200);
        let id = rules[0].id.clone();
        assert!(set_site_rule_enabled(&mut cfg, &id, false, 400) && cfg.browser.site_rules.updated_at == 400);
        assert!(delete_site_rule(&mut cfg, &id, 500) && cfg.browser.site_rules.rules.len() == 1);
        assert!(set_default_mode(&mut cfg, FullscreenMode::Zone, 600) && !set_default_mode(&mut cfg, FullscreenMode::Zone, 700));
        assert!(set_integration(&mut cfg, false) && !set_integration(&mut cfg, false));
        assert!(cfg.validated().is_ok());
    }
}
