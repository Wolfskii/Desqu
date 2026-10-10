//! Deciding which running apps belong in a workspace, and what is missing on restore. Pure.

use std::collections::HashSet;
use std::path::Path;

use super::model::AppLaunch;

/// A top-level window's process, as read from the OS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningApp {
    pub exe: String,
    pub command_line: String,
}

/// Windows shell parts and our own apps are never saved into a workspace.
const SKIP: &[&str] = &[
    "explorer.exe",
    "applicationframehost.exe",
    "textinputhost.exe",
    "systemsettings.exe",
    "shellexperiencehost.exe",
    "searchhost.exe",
    "startmenuexperiencehost.exe",
    "lockapp.exe",
    "taskmgr.exe",
    "desqu.exe",
    "workspaces.exe",
    "vials.exe",
    "screenbound.exe",
];

/// Splits a command line into the executable token and the rest. A quoted first token may contain
/// spaces; an unquoted one ends at the first space.
pub fn split_command_line(cmd: &str) -> (String, Option<String>) {
    let cmd = cmd.trim();
    let (exe, rest) = if let Some(stripped) = cmd.strip_prefix('"') {
        match stripped.find('"') {
            Some(end) => (&stripped[..end], &stripped[end + 1..]),
            None => (stripped, ""),
        }
    } else {
        match cmd.find(char::is_whitespace) {
            Some(i) => (&cmd[..i], &cmd[i..]),
            None => (cmd, ""),
        }
    };
    let rest = rest.trim();
    (
        exe.to_string(),
        (!rest.is_empty()).then(|| rest.to_string()),
    )
}

fn file_name_lower(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// Whether a window's process is something a user would want restarted.
pub fn is_candidate(exe: &str) -> bool {
    let lower = exe.to_ascii_lowercase();
    if !Path::new(exe).is_absolute() || !lower.ends_with(".exe") {
        return false;
    }
    if SKIP.contains(&file_name_lower(exe).as_str()) {
        return false;
    }
    // Anything under the Windows folder is part of the system, not a user's app.
    !lower.starts_with(r"c:\windows\") && !lower.contains(r"\windowsapps\")
}

/// Turns the apps with windows on a desktop into launch entries (one per exe + arguments).
pub fn launches_from(running: &[RunningApp]) -> Vec<AppLaunch> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for app in running {
        if !is_candidate(&app.exe) {
            continue;
        }
        let (_, args) = split_command_line(&app.command_line);
        if seen.insert((app.exe.to_ascii_lowercase(), args.clone())) {
            out.push(AppLaunch {
                exe: app.exe.clone(),
                args,
                name: String::new(),
            });
        }
    }
    out.sort_by_key(|a| a.exe.to_ascii_lowercase());
    out
}

/// The wanted apps that have no window yet. Matching is by executable only: an app that is open
/// with different arguments (another document) counts as running and is not started a second time.
pub fn missing_apps<'a>(wanted: &'a [AppLaunch], running: &[RunningApp]) -> Vec<&'a AppLaunch> {
    let open: HashSet<String> = running.iter().map(|r| r.exe.to_ascii_lowercase()).collect();
    wanted
        .iter()
        .filter(|w| !open.contains(&w.exe.to_ascii_lowercase()))
        .collect()
}

/// `file:///C:/Users/me/My%20Folder` -> `C:\Users\me\My Folder`. Anything that is not a local
/// `file:` URL (for example a `shell:` location) yields `None`.
pub fn folder_from_url(url: &str) -> Option<String> {
    let rest = url.strip_prefix("file:///")?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = rest.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    let text = String::from_utf8(out).ok()?;
    let path = text.replace('/', "\\");
    // `C:\...` only; network (`file://server/share`) and device paths are not restored.
    let mut chars = path.chars();
    (chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && chars.next() == Some(':'))
        .then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(exe: &str, cmd: &str) -> RunningApp {
        RunningApp {
            exe: exe.into(),
            command_line: cmd.into(),
        }
    }

    #[test]
    fn command_lines_split_into_exe_and_arguments() {
        assert_eq!(
            split_command_line(r#""C:\Program Files\App\a.exe" --flag "x y""#),
            (
                r"C:\Program Files\App\a.exe".into(),
                Some(r#"--flag "x y""#.into())
            )
        );
        assert_eq!(
            split_command_line(r"C:\a.exe -x"),
            (r"C:\a.exe".into(), Some("-x".into()))
        );
        assert_eq!(
            split_command_line(r#""winver.exe""#),
            ("winver.exe".into(), None)
        );
        assert_eq!(split_command_line("   "), (String::new(), None));
    }

    #[test]
    fn shell_parts_system_apps_and_ourselves_are_not_candidates() {
        assert!(is_candidate(
            r"C:\Program Files\Mozilla Firefox\firefox.exe"
        ));
        assert!(is_candidate(r"D:\Tools\code.exe"));
        assert!(!is_candidate(r"C:\Windows\explorer.exe"));
        assert!(!is_candidate(r"C:\Windows\System32\winver.exe"));
        assert!(!is_candidate(r"C:\Program Files\Desqu\desqu.exe"));
        assert!(!is_candidate(r"C:\Program Files\WindowsApps\x\y.exe"));
        assert!(!is_candidate("notepad.exe"));
        assert!(!is_candidate(r"C:\Tools\readme.txt"));
    }

    #[test]
    fn launches_are_deduplicated_and_keep_arguments() {
        let apps = launches_from(&[
            run(r"C:\Tools\a.exe", r#""C:\Tools\a.exe" file1"#),
            run(r"c:\tools\A.exe", r#""c:\tools\A.exe" file1"#),
            run(r"C:\Tools\a.exe", r#""C:\Tools\a.exe" file2"#),
            run(r"C:\Windows\explorer.exe", "explorer.exe"),
        ]);
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].args.as_deref(), Some("file1"));
        assert_eq!(apps[1].args.as_deref(), Some("file2"));
    }

    #[test]
    fn only_apps_without_a_window_are_missing() {
        let wanted = vec![
            AppLaunch {
                exe: r"C:\Tools\a.exe".into(),
                args: None,
                name: String::new(),
            },
            AppLaunch {
                exe: r"C:\Tools\b.exe".into(),
                args: Some("x".into()),
                name: String::new(),
            },
        ];
        let missing = missing_apps(&wanted, &[run(r"c:\tools\A.EXE", "a.exe other-document")]);
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].exe, r"C:\Tools\b.exe");
    }

    #[test]
    fn folder_urls_decode_and_unsupported_ones_are_dropped() {
        assert_eq!(
            folder_from_url("file:///C:/Users/me/My%20Folder").as_deref(),
            Some(r"C:\Users\me\My Folder")
        );
        assert_eq!(folder_from_url("file:///D:/").as_deref(), Some(r"D:\"));
        assert_eq!(folder_from_url("file://server/share/x"), None);
        assert_eq!(folder_from_url("shell:RecycleBinFolder"), None);
        assert_eq!(folder_from_url("file:///C:/bad%2"), None);
    }
}
