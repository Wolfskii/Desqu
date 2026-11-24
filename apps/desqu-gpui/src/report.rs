//! "Save a report": a zip to attach to a bug report. It holds the versions, what each app is doing, a
//! summary of the settings (the options and counts, never the names of your files) and the tail of each
//! app's log. Anything with your user folder in it has that part hidden.

use std::path::{Path, PathBuf};

use screenbound_module::core::config::AppConfig;
use vials_core::model::VialsConfig;
use workspaces_core::model::WorkspacesConfig;

use crate::modules::{Kind, ALL};
use crate::supervisor::Status;

/// How many lines of each log go in.
const LOG_LINES: usize = 400;

/// `text` with the user's folder (and the user's name in the typical `C:\Users\name` form) hidden.
pub fn redact(text: &str, profile: &str) -> String {
    if profile.is_empty() {
        return text.to_string();
    }
    let lower = text.to_lowercase();
    let needle = profile.to_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(found) = lower[at..].find(&needle) {
        let start = at + found;
        out.push_str(&text[at..start]);
        out.push_str("%USERPROFILE%");
        at = start + needle.len();
    }
    out.push_str(&text[at..]);
    // The same folder written with forward slashes.
    let slashed = profile.replace('\\', "/");
    if slashed != profile {
        return redact(&out, &slashed);
    }
    out
}

/// The last `max` lines of `text`.
pub fn tail(text: &str, max: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(max)..].join("\n")
}

fn yes_no(on: bool) -> &'static str {
    if on {
        "on"
    } else {
        "off"
    }
}

pub fn vials_summary(c: &VialsConfig) -> String {
    format!(
        "Vials\n  vials: {}, items in vials: {}, rules: {}\n  theme: {:?}, glass tint: {}\n  sort new items: {}, hide items on the desktop: {}, create by dragging: {}, desktop menu entry: {}\n  quick-hide: {}, open on hover: {}, offer for selection: {}\n",
        c.vials.len(),
        c.assigned_ids().len(),
        c.rules.len(),
        c.theme,
        c.glass_tint,
        yes_no(c.auto_sort),
        yes_no(c.hide_on_desktop),
        yes_no(c.create_on_drag),
        yes_no(c.context_menu),
        yes_no(c.quick_hide),
        yes_no(c.hover_open),
        yes_no(c.offer_for_selection),
    )
}

pub fn workspaces_summary(c: &WorkspacesConfig) -> String {
    format!(
        "Workspaces\n  desktops with items: {}, items tied to a desktop: {}\n  new files join the current desktop: {}, desktop name note: {}, jump hotkeys: {}\n",
        c.workspaces.len(),
        c.workspaces.iter().map(|w| w.items.len()).sum::<usize>(),
        yes_no(c.auto_assign_new),
        yes_no(c.show_name),
        yes_no(c.jump_hotkeys),
    )
}

pub fn screenbound_summary(c: &AppConfig) -> String {
    format!(
        "Screenbound\n  on: {}, zones: {}, rules: {}, groups: {}, known apps: {}, apps turned on: {}\n  browser integration: {}, leave games alone: {}, zone hotkey: {}\n",
        yes_no(c.enabled),
        c.zones.len(),
        c.rules.len(),
        c.groups.len(),
        c.known_apps.len(),
        c.known_apps.iter().filter(|a| a.enabled).count(),
        yes_no(c.browser.integration_enabled),
        yes_no(c.leave_games_alone),
        if c.zone_hotkey.is_empty() { "off" } else { &c.zone_hotkey },
    )
}

fn windows_version() -> String {
    std::process::Command::new("cmd").args(["/C", "ver"]).output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

/// What the report says about versions and what is running.
pub fn overview(status: impl Fn(Kind) -> Status) -> String {
    let mut out = format!("Desqu {}\nWindows: {}\n\nApps\n", env!("CARGO_PKG_VERSION"), windows_version());
    for kind in ALL {
        out.push_str(&format!("  {}: {}\n", kind.name(), crate::view::status_text(status(kind))));
    }
    out
}

/// Writes the report into `folder` and returns the zip's path.
pub fn save(folder: &Path, status: impl Fn(Kind) -> Status) -> Result<PathBuf, String> {
    use std::os::windows::process::CommandExt;
    let profile = std::env::var("USERPROFILE").unwrap_or_default();
    let work = std::env::temp_dir().join(format!("desqu-report-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;

    let mut files: Vec<PathBuf> = Vec::new();
    let mut add = |name: &str, text: String| -> Result<(), String> {
        let path = work.join(name);
        std::fs::write(&path, redact(&text, &profile)).map_err(|e| e.to_string())?;
        files.push(path);
        Ok(())
    };
    add("overview.txt", overview(status))?;
    let settings = [
        vials_summary(&crate::app_settings::vials()),
        workspaces_summary(&crate::app_settings::workspaces()),
        AppConfig::load(&crate::screenbound_remote::config_path()).map(|c| screenbound_summary(&c)).unwrap_or_else(|e| format!("Screenbound\n  settings could not be read: {e}\n")),
    ]
    .join("\n");
    add("settings.txt", settings)?;
    for kind in ALL {
        let log = std::fs::read_to_string(crate::supervisor::log_path(kind)).unwrap_or_default();
        add(&format!("log-{}.txt", kind.name().to_lowercase()), tail(&log, LOG_LINES))?;
    }

    let mut name = String::from("desqu-report");
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    name.push_str(&format!("-{stamp}.zip"));
    let zip = folder.join(name);
    let list: Vec<String> = files.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", "Compress-Archive -LiteralPath ($env:DESQU_ZIP_ITEMS -split '\\|') -DestinationPath $env:DESQU_ZIP_TO -ErrorAction Stop"])
        .env("DESQU_ZIP_ITEMS", list.join("|"))
        .env("DESQU_ZIP_TO", &zip)
        .creation_flags(0x0800_0000)
        .output()
        .map_err(|e| e.to_string());
    let _ = std::fs::remove_dir_all(&work);
    let output = output?;
    if output.status.success() {
        Ok(zip)
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_user_folder_is_hidden_in_either_slash_style_and_any_case() {
        let text = r"error in C:\Users\Dawid\Projects\x and c:/users/dawid/Desktop/y, not C:\Windows";
        let out = redact(text, r"C:\Users\dawid");
        assert!(!out.to_lowercase().contains("dawid"), "{out}");
        assert!(out.contains(r"%USERPROFILE%\Projects\x") && out.contains("%USERPROFILE%/Desktop/y") && out.contains(r"C:\Windows"));
        assert_eq!(redact("nothing here", ""), "nothing here");
    }

    #[test]
    fn only_the_last_lines_of_a_log_are_kept() {
        let log = (1..=10).map(|n| n.to_string()).collect::<Vec<_>>().join("\n");
        assert_eq!(tail(&log, 3), "8\n9\n10");
        assert_eq!(tail("a\nb", 5), "a\nb");
    }

    #[test]
    fn the_summaries_hold_options_and_counts_but_no_file_names() {
        let mut v = VialsConfig::default();
        v.vials.push(vials_core::model::Vial {
            id: "v".into(),
            name: "Secret project".into(),
            monitor: None,
            rect: vials_core::model::NormalizedRect { x: 0.0, y: 0.0, width: 0.2, height: 0.2 },
            items: vec![r"C:\Users\dawid\Desktop\taxes 2026.pdf".into()],
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
        });
        let text = vials_summary(&v);
        assert!(text.contains("vials: 1, items in vials: 1"));
        assert!(!text.contains("taxes") && !text.contains("Secret"), "{text}");
        assert!(workspaces_summary(&WorkspacesConfig::default()).contains("jump hotkeys: off"));
        assert!(screenbound_summary(&AppConfig::default()).contains("zones: 1"));
    }

    #[test]
    fn a_report_is_written_as_a_zip_with_its_parts() {
        let dir = std::env::temp_dir().join(format!("desqu-report-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let result = save(&dir, |_| Status::Off);
        let size = result.as_ref().ok().and_then(|p| std::fs::metadata(p).ok()).map_or(0, |m| m.len());
        let _ = std::fs::remove_dir_all(&dir);
        result.expect("the report is saved");
        assert!(size > 200, "the zip has content: {size} bytes");
    }
}
