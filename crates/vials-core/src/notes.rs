//! Notes and links as items of a vial. Both are ordinary files in one folder of the program (a `.txt` for a note,
//! a `.url` for a link), shown by a folder portal over that folder, so they open in the default program and
//! can be dragged out like any file. Pure helpers.

use std::path::PathBuf;

/// The folder the notes and links live in.
pub fn folder() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default().join("io.github.wolfskii.vials").join("notes")
}

/// Whether a portal over `source` is the notes vial.
pub fn is_notes(source: &str) -> bool {
    std::path::Path::new(source) == folder()
}

/// Whether the text is one web address (what a link can be made from).
pub fn looks_like_url(text: &str) -> bool {
    let t = text.trim();
    (t.starts_with("http://") || t.starts_with("https://")) && !t.contains(char::is_whitespace) && t.len() > 8
}

/// The content of a `.url` file for `url`.
pub fn url_file_text(url: &str) -> String {
    format!("[InternetShortcut]
URL={}
", url.trim())
}

/// A readable name for a link: the site, without `www.`.
pub fn link_name(url: &str) -> String {
    let rest = url.trim().trim_start_matches("https://").trim_start_matches("http://");
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = host.trim_start_matches("www.");
    crate::clip::file_safe(if host.is_empty() { "Link" } else { host })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_single_web_address_makes_a_link() {
        assert!(looks_like_url(" https://example.com/a?b=1 "));
        assert!(looks_like_url("http://localhost:8080"));
        assert!(!looks_like_url("example.com"));
        assert!(!looks_like_url("https://example.com and more"));
        assert!(!looks_like_url("https://"));
        assert!(!looks_like_url(r"C:\Users\x"));
    }

    #[test]
    fn a_link_file_has_the_address_and_a_name_from_the_site() {
        assert_eq!(url_file_text(" https://a.b/c "), "[InternetShortcut]
URL=https://a.b/c
");
        assert_eq!(link_name("https://www.wolfskii.dev/path?x=1"), "wolfskii.dev");
        assert_eq!(link_name("http://localhost:8080/"), "localhost_8080", "a colon cannot be in a file name");
    }

    #[test]
    fn the_notes_folder_is_recognised_in_any_spelling() {
        let f = folder();
        assert!(is_notes(&f.to_string_lossy()));
        assert!(!is_notes(r"C:\Somewhere\else"));
    }
}
