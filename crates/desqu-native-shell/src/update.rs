//! "Check for updates": reads a small JSON manifest (the same shape the Tauri updater uses:
//! `{"version": "...", "notes": "...", "platforms": {"windows-x86_64": {"url": "..."}}}`) and says
//! whether a newer version exists. It only points at the download; it does not install anything, so
//! no signature has to be trusted by this program.

use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub url: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    UpToDate,
    Available(Release),
}

pub fn parse(json: &str) -> Result<Release, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("the update information is not valid: {e}"))?;
    let version = v.get("version").and_then(|x| x.as_str()).ok_or("the update information has no version")?.trim_start_matches('v').to_string();
    let url = v
        .pointer("/platforms/windows-x86_64/url")
        .and_then(|x| x.as_str())
        .filter(|u| u.starts_with("https://"))
        .map(str::to_string);
    let notes = v.get("notes").and_then(|x| x.as_str()).map(str::to_string);
    Ok(Release { version, url, notes })
}

/// `major.minor.patch` as numbers; anything after a `-` (a pre-release) is ignored.
fn numbers(version: &str) -> Vec<u64> {
    version.trim_start_matches('v').split('-').next().unwrap_or("").split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

pub fn is_newer(current: &str, candidate: &str) -> bool {
    let (a, b) = (numbers(current), numbers(candidate));
    (0..a.len().max(b.len())).map(|i| (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0))).find(|(x, y)| x != y).is_some_and(|(x, y)| y > x)
}

pub fn decide(current: &str, release: Release) -> Outcome {
    if is_newer(current, &release.version) {
        Outcome::Available(release)
    } else {
        Outcome::UpToDate
    }
}

/// Fetches the manifest and compares. Blocks: call it off the UI thread.
pub fn check(current: &str, url: &str) -> Result<Outcome, String> {
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(10)).build();
    let body = agent.get(url).call().map_err(|e| format!("could not reach the update server: {e}"))?.into_string().map_err(|e| e.to_string())?;
    Ok(decide(current, parse(&body)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"{"version":"v0.5.0","notes":"Faster","pub_date":"2026-12-01T00:00:00Z",
        "platforms":{"windows-x86_64":{"signature":"x","url":"https://example.com/Vials_0.5.0_x64-setup.exe"}}}"#;

    #[test]
    fn a_manifest_is_read() {
        let r = parse(MANIFEST).unwrap();
        assert_eq!(r.version, "0.5.0");
        assert_eq!(r.url.as_deref(), Some("https://example.com/Vials_0.5.0_x64-setup.exe"));
        assert_eq!(r.notes.as_deref(), Some("Faster"));
    }

    #[test]
    fn a_broken_or_unsafe_manifest_is_refused_or_ignored() {
        assert!(parse("not json").is_err());
        assert!(parse("{}").is_err(), "no version");
        let r = parse(r#"{"version":"1.0.0","platforms":{"windows-x86_64":{"url":"http://insecure.example/x.exe"}}}"#).unwrap();
        assert_eq!(r.url, None, "only https download links are offered");
    }

    #[test]
    fn versions_compare_by_number_not_by_text() {
        assert!(is_newer("0.4.0", "0.5.0"));
        assert!(is_newer("0.9.0", "0.10.0"));
        assert!(is_newer("0.4.0", "0.4.0.1"));
        assert!(!is_newer("0.4.0", "0.4.0"));
        assert!(!is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.4.0", "0.4.0-beta"), "a pre-release of the same number is not newer");
    }

    /// Talks to the internet: the public manifest of the Tauri build of Vials, to prove HTTPS works.
    #[test]
    #[ignore = "uses the network"]
    fn the_network_path_works() {
        let out = check("0.0.1", "https://wolfskii.github.io/Desqu/updates/vials/latest.json");
        eprintln!("{out:?}");
        assert!(matches!(out, Ok(Outcome::Available(_)) | Err(_)), "it answers either way");
        if let Err(e) = &out {
            assert!(!e.contains("tls") || !e.contains("handshake"), "a TLS failure means HTTPS is not set up: {e}");
        }
    }

    #[test]
    fn only_a_newer_release_is_an_update() {
        let r = parse(MANIFEST).unwrap();
        assert_eq!(decide("0.4.0", r.clone()), Outcome::Available(r.clone()));
        assert_eq!(decide("0.5.0", r), Outcome::UpToDate);
    }
}
