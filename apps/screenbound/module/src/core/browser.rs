//! Browser integration: per-site fullscreen rules and the geometry for placing a browser's page
//! viewport inside a zone. Pure logic. URL matching lives in the extension, never here (see
//! apps/browser-extension/src/shared/rules.ts, which must match these serde shapes).

use serde::{Deserialize, Serialize};

use super::geometry::Rect;

/// How a site's fullscreen request is handled. Mirrors `FullscreenMode` in the extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FullscreenMode {
    /// Browser fullscreen; Screenbound limits it as usual.
    #[default]
    Native,
    /// Browser fullscreen; Screenbound leaves it alone.
    Fullscreen,
    /// The video fills the tab; the browser window does not change.
    Window,
    /// The browser's page viewport is placed in a Screenbound zone.
    Zone,
}

/// What a site rule matches. Tagged like the TS side: `{"type":"host","value":"..."}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum SitePattern {
    /// Exact host, e.g. `www.youtube.com`.
    Host(String),
    /// Domain and all subdomains, e.g. `youtube.com`.
    Domain(String),
    /// URL prefix, e.g. `https://www.youtube.com/watch`.
    UrlPrefix(String),
}

impl SitePattern {
    fn normalized(&self) -> SitePattern {
        match self {
            SitePattern::UrlPrefix(v) => SitePattern::UrlPrefix(v.trim().to_string()),
            SitePattern::Host(v) => SitePattern::Host(normalize_host(v)),
            SitePattern::Domain(v) => SitePattern::Domain(normalize_host(v)),
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            SitePattern::Host(v) | SitePattern::Domain(v) | SitePattern::UrlPrefix(v) => v.is_empty(),
        }
    }
}

/// Same rules as `normalizeHost` in the extension: lowercase, no scheme, wildcard, path or trailing dot.
pub fn normalize_host(value: &str) -> String {
    let mut v = value.trim().to_ascii_lowercase();
    if let Some(pos) = v.find("://") {
        let scheme = &v[..pos];
        if !scheme.is_empty()
            && scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        {
            v = v[pos + 3..].to_string();
        }
    }
    if let Some(rest) = v.strip_prefix("*.") {
        v = rest.to_string();
    }
    let host = v.split('/').next().unwrap_or("");
    host.strip_suffix('.').unwrap_or(host).to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteRule {
    pub id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub pattern: SitePattern,
    pub mode: FullscreenMode,
    /// Zone mode only. `None` = use the document's default zone.
    #[serde(default)]
    pub zone_id: Option<String>,
}

fn default_true() -> bool {
    true
}

/// The whole rules document. Last edit wins when two copies are merged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteRules {
    #[serde(default = "rules_version")]
    pub version: u32,
    /// ms since the Unix epoch, set by whoever last edited the document.
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default)]
    pub default_mode: FullscreenMode,
    #[serde(default)]
    pub default_zone_id: Option<String>,
    #[serde(default)]
    pub rules: Vec<SiteRule>,
}

fn rules_version() -> u32 {
    1
}

impl Default for SiteRules {
    fn default() -> Self {
        Self {
            version: rules_version(),
            updated_at: 0,
            default_mode: FullscreenMode::Native,
            default_zone_id: None,
            rules: Vec::new(),
        }
    }
}

impl SiteRules {
    /// Whole-document last-write-wins, the same as `mergeRules` in the extension: the local copy
    /// wins only when strictly newer. On a tie the remote (Desqu's) copy is kept.
    pub fn newer(local: SiteRules, remote: SiteRules) -> SiteRules {
        if local.updated_at > remote.updated_at {
            local
        } else {
            remote
        }
    }

    /// Trims patterns, drops empty ones and duplicate ids, and forgets zones that no longer exist.
    /// Stale zone ids are cleared rather than rejected: the extension may still hold them.
    pub fn sanitize(&mut self, zone_ids: &[String]) {
        let known = |id: &str| zone_ids.iter().any(|z| z == id);
        let mut seen = std::collections::HashSet::new();
        self.rules.retain_mut(|rule| {
            rule.pattern = rule.pattern.normalized();
            if rule.pattern.is_empty() || !seen.insert(rule.id.clone()) {
                return false;
            }
            if rule.zone_id.as_deref().is_some_and(|z| !known(z)) {
                rule.zone_id = None;
            }
            true
        });
        if self.default_zone_id.as_deref().is_some_and(|z| !known(z)) {
            self.default_zone_id = None;
        }
        self.version = rules_version();
    }
}

/// `config.browser` in `config.json`. Missing on older configs, so everything defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSettings {
    /// Off means: no native host registration and no pipe server.
    #[serde(default = "default_true")]
    pub integration_enabled: bool,
    #[serde(default)]
    pub site_rules: SiteRules,
}

impl Default for BrowserSettings {
    fn default() -> Self {
        Self {
            integration_enabled: true,
            site_rules: SiteRules::default(),
        }
    }
}

/// Places a browser page viewport inside a zone.
///
/// `window` is the browser window, `viewport` the page area inside it (both physical pixels, screen
/// coordinates). Returns:
/// - the window rect to set so the viewport lands exactly on `zone`, and
/// - the region (window-relative) to clip the window to, which hides the tab strip and toolbar.
///
/// The viewport may sit anywhere inside the window; the insets on each side are kept.
pub fn zone_window_plan(zone: Rect, window: Rect, viewport: Rect) -> (Rect, Rect) {
    let inset_left = viewport.left - window.left;
    let inset_top = viewport.top - window.top;
    let inset_right = window.right - viewport.right;
    let inset_bottom = window.bottom - viewport.bottom;

    let window_rect = Rect::new(
        zone.left - inset_left,
        zone.top - inset_top,
        zone.right + inset_right,
        zone.bottom + inset_bottom,
    );
    let region = Rect::new(
        inset_left,
        inset_top,
        inset_left + zone.width(),
        inset_top + zone.height(),
    );
    (window_rect, region)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: &str, pattern: SitePattern, zone: Option<&str>) -> SiteRule {
        SiteRule {
            id: id.into(),
            enabled: true,
            pattern,
            mode: FullscreenMode::Window,
            zone_id: zone.map(str::to_string),
        }
    }

    #[test]
    fn json_shape_matches_the_extension() {
        let rules = SiteRules {
            version: 1,
            updated_at: 1_700_000_000_000,
            default_mode: FullscreenMode::Native,
            default_zone_id: None,
            rules: vec![rule(
                "r1",
                SitePattern::Domain("youtube.com".into()),
                Some("zone-left-75"),
            )],
        };
        let json = serde_json::to_value(&rules).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "version": 1,
                "updatedAt": 1_700_000_000_000u64,
                "defaultMode": "native",
                "defaultZoneId": null,
                "rules": [{
                    "id": "r1",
                    "enabled": true,
                    "pattern": {"type": "domain", "value": "youtube.com"},
                    "mode": "window",
                    "zoneId": "zone-left-75"
                }]
            })
        );
        let back: SiteRules = serde_json::from_value(json).unwrap();
        assert_eq!(back, rules);
    }

    #[test]
    fn url_prefix_and_zone_modes_use_camel_case_names() {
        let p: SitePattern = serde_json::from_str(r#"{"type":"urlPrefix","value":"https://a.example/x"}"#).unwrap();
        assert_eq!(p, SitePattern::UrlPrefix("https://a.example/x".into()));
        assert_eq!(serde_json::to_string(&FullscreenMode::Zone).unwrap(), r#""zone""#);
        assert_eq!(serde_json::to_string(&FullscreenMode::Fullscreen).unwrap(), r#""fullscreen""#);
    }

    #[test]
    fn missing_fields_take_defaults() {
        let s: BrowserSettings = serde_json::from_str("{}").unwrap();
        assert!(s.integration_enabled);
        assert_eq!(s.site_rules, SiteRules::default());
        let r: SiteRules = serde_json::from_str(r#"{"rules":[{"id":"a","pattern":{"type":"host","value":"x.y"},"mode":"zone"}]}"#).unwrap();
        assert_eq!(r.default_mode, FullscreenMode::Native);
        assert!(r.rules[0].enabled);
        assert_eq!(r.rules[0].zone_id, None);
    }

    #[test]
    fn normalize_host_matches_the_extension() {
        assert_eq!(normalize_host("  WWW.Example.COM. "), "www.example.com");
        assert_eq!(normalize_host("*.vimeo.com/"), "vimeo.com");
        assert_eq!(normalize_host("https://youtube.com/watch?v=1"), "youtube.com");
        assert_eq!(normalize_host("example.com"), "example.com");
    }

    #[test]
    fn newer_keeps_the_later_edit_and_the_remote_on_a_tie() {
        let mk = |t: u64, mode: FullscreenMode| SiteRules {
            updated_at: t,
            default_mode: mode,
            ..SiteRules::default()
        };
        assert_eq!(SiteRules::newer(mk(200, FullscreenMode::Zone), mk(100, FullscreenMode::Window)).default_mode, FullscreenMode::Zone);
        assert_eq!(SiteRules::newer(mk(100, FullscreenMode::Zone), mk(200, FullscreenMode::Window)).default_mode, FullscreenMode::Window);
        assert_eq!(SiteRules::newer(mk(5, FullscreenMode::Zone), mk(5, FullscreenMode::Window)).default_mode, FullscreenMode::Window);
    }

    #[test]
    fn sanitize_trims_dedupes_and_clears_stale_zones() {
        let mut s = SiteRules {
            default_zone_id: Some("gone".into()),
            rules: vec![
                rule("a", SitePattern::Host("  Example.COM. ".into()), Some("left")),
                rule("a", SitePattern::Host("dup.example".into()), None),
                rule("b", SitePattern::Domain("   ".into()), None),
                rule("c", SitePattern::UrlPrefix("".into()), None),
                rule("d", SitePattern::Domain("vimeo.com".into()), Some("gone")),
            ],
            ..SiteRules::default()
        };
        s.sanitize(&["left".to_string()]);
        let ids: Vec<&str> = s.rules.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["a", "d"]);
        assert_eq!(s.rules[0].pattern, SitePattern::Host("example.com".into()));
        assert_eq!(s.rules[0].zone_id.as_deref(), Some("left"));
        assert_eq!(s.rules[1].zone_id, None);
        assert_eq!(s.default_zone_id, None);
    }

    #[test]
    fn a_config_without_browser_loads_with_defaults_and_keeps_its_zones() {
        use super::super::config::AppConfig;
        let json = r#"{"version":2,"enabled":true,"zones":[{"id":"z1","name":"Left","rect":{"x":0,"y":0,"width":0.5,"height":1},"unit":"percent","reference":"monitor"}],"rules":[]}"#;
        let mut config: AppConfig = serde_json::from_str(json).unwrap();
        config = config.validated().unwrap();
        assert_eq!(config.browser, BrowserSettings::default());
        assert_eq!(config.zones.len(), 1);
    }

    #[test]
    fn validation_clears_stale_zone_ids_in_saved_rules() {
        use super::super::config::AppConfig;
        let mut config = AppConfig::default();
        config.browser.site_rules.default_zone_id = Some("missing".into());
        config.browser.site_rules.rules.push(rule("r", SitePattern::Domain("a.example".into()), Some("missing")));
        let config = config.validated().unwrap();
        assert_eq!(config.browser.site_rules.default_zone_id, None);
        assert_eq!(config.browser.site_rules.rules[0].zone_id, None);
    }

    #[test]
    fn zero_insets_map_the_viewport_onto_the_zone_exactly() {
        let zone = Rect::new(0, 0, 1440, 1080);
        let window = Rect::new(0, 0, 1920, 1080);
        let (w, region) = zone_window_plan(zone, window, window);
        // No chrome: the window is exactly the zone.
        assert_eq!(w, Rect::new(0, 0, 1440, 1080));
        assert_eq!(region, Rect::new(0, 0, 1440, 1080));
    }

    #[test]
    fn a_browser_chrome_inset_is_kept_and_clipped() {
        // 100 % scaling: about 80 px of tab strip and toolbar above the page.
        let zone = Rect::new(0, 0, 1440, 1080);
        let window = Rect::new(100, 100, 2020, 1180);
        let viewport = Rect::new(100, 180, 2020, 1180);
        let (w, region) = zone_window_plan(zone, window, viewport);
        assert_eq!(w, Rect::new(0, -80, 1440, 1080));
        assert_eq!(region, Rect::new(0, 80, 1440, 1160));
        // The viewport now sits exactly on the zone.
        assert_eq!(w.top + region.top, zone.top);
        assert_eq!(w.left + region.left, zone.left);
    }

    #[test]
    fn a_150_percent_inset_and_a_monitor_with_negative_origin() {
        let zone = Rect::new(-1920, 0, -480, 1080);
        let window = Rect::new(-1900, 60, -200, 1000);
        let viewport = Rect::new(-1900, 120, -200, 1000); // 60 px tabs/toolbar at 150 %
        let (w, region) = zone_window_plan(zone, window, viewport);
        assert_eq!(region, Rect::new(0, 60, 1440, 1140));
        assert_eq!(w.left + region.left, zone.left);
        assert_eq!(w.top + region.top, zone.top);
        assert_eq!(region.width(), zone.width());
        assert_eq!(region.height(), zone.height());
    }

    #[test]
    fn a_zone_at_the_top_edge_lets_the_window_extend_above_it() {
        let zone = Rect::new(0, 0, 960, 1080);
        let window = Rect::new(0, 0, 960, 1040);
        let viewport = Rect::new(0, 40, 960, 1040);
        let (w, region) = zone_window_plan(zone, window, viewport);
        assert!(w.top < 0, "window rect extends above the monitor top, which is allowed");
        assert_eq!(region.top, 40);
        assert_eq!(region.height(), 1080);
    }
}
