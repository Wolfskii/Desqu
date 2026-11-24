//! Searching the items of a vial: what the user typed, and how it is matched. Pure, apart from walking a
//! folder for a portal.
//!
//! The text is words (each must be in the name), extensions (`.pdf`) and kinds (`kind:images`):
//! `report .pdf` finds PDF files with "report" in the name, `kind:folders` finds folders.

use std::path::Path;

/// A group of file types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Images,
    Documents,
    Videos,
    Audio,
    Archives,
    Programs,
    Shortcuts,
    Folders,
}

impl Kind {
    fn from_word(word: &str) -> Option<Kind> {
        Some(match word {
            "images" | "image" | "pictures" | "photos" => Kind::Images,
            "documents" | "document" | "docs" => Kind::Documents,
            "videos" | "video" => Kind::Videos,
            "audio" | "music" => Kind::Audio,
            "archives" | "archive" | "zips" => Kind::Archives,
            "programs" | "apps" => Kind::Programs,
            "shortcuts" | "links" => Kind::Shortcuts,
            "folders" | "folder" | "dirs" => Kind::Folders,
            _ => return None,
        })
    }

    /// The file extensions of the kind (none for folders).
    fn extensions(self) -> &'static [&'static str] {
        match self {
            Kind::Images => &["png", "jpg", "jpeg", "gif", "bmp", "webp", "svg", "ico", "tif", "tiff", "heic", "avif"],
            Kind::Documents => &["pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "md", "rtf", "odt", "ods", "csv"],
            Kind::Videos => &["mp4", "mkv", "avi", "mov", "wmv", "webm", "m4v"],
            Kind::Audio => &["mp3", "wav", "flac", "ogg", "m4a", "aac", "wma"],
            Kind::Archives => &["zip", "rar", "7z", "tar", "gz", "iso"],
            Kind::Programs => &["exe", "msi", "bat", "cmd", "ps1"],
            Kind::Shortcuts => &["lnk", "url"],
            Kind::Folders => &[],
        }
    }
}

/// What was typed, taken apart.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    words: Vec<String>,
    extensions: Vec<String>,
    kinds: Vec<Kind>,
}

impl Query {
    pub fn parse(text: &str) -> Query {
        let mut q = Query::default();
        for part in text.split_whitespace().map(str::to_lowercase) {
            if let Some(kind) = part.strip_prefix("kind:") {
                // A kind that is not known (yet, while typing) matches nothing rather than everything.
                match Kind::from_word(kind) {
                    Some(k) => q.kinds.push(k),
                    None if !kind.is_empty() => q.words.push(part),
                    None => {}
                }
            } else if let Some(ext) = part.strip_prefix('.') {
                if !ext.is_empty() {
                    q.extensions.push(ext.to_string());
                }
            } else {
                q.words.push(part);
            }
        }
        q
    }

    /// Nothing to filter by.
    pub fn is_empty(&self) -> bool {
        self.words.is_empty() && self.extensions.is_empty() && self.kinds.is_empty()
    }

    /// Whether an item with this name (as shown), path and folder-ness fits: every word is in the name, one of
    /// the extensions (when given) is the file's, and the file is one of the kinds (when given).
    pub fn matches(&self, name: &str, path: &str, is_dir: bool) -> bool {
        let lower = name.to_lowercase();
        if !self.words.iter().all(|w| lower.contains(w.as_str())) {
            return false;
        }
        let ext = Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if !self.extensions.is_empty() && (is_dir || !self.extensions.contains(&ext)) {
            return false;
        }
        self.kinds.is_empty() || self.kinds.iter().any(|k| if *k == Kind::Folders { is_dir } else { !is_dir && k.extensions().contains(&ext.as_str()) })
    }
}

/// The paths below `root` (at most `max_depth` folders deep) that fit `query`, up to `limit`, folders first as
/// they are found. `cancelled` is asked now and then: the search stops when it says so (the user typed on).
pub fn search_dir(root: &Path, query: &Query, limit: usize, max_depth: usize, cancelled: &dyn Fn() -> bool) -> Vec<String> {
    let mut found = Vec::new();
    let mut queue = std::collections::VecDeque::from([(root.to_path_buf(), 0usize)]);
    while let Some((dir, depth)) = queue.pop_front() {
        if cancelled() || found.len() >= limit {
            break;
        }
        let Ok(listing) = crate::portal::list_dir(&dir) else { continue };
        for path in listing {
            let p = Path::new(&path);
            let is_dir = p.is_dir();
            let name = crate::portal::title_of(p);
            if query.matches(&name, &path, is_dir) {
                found.push(path.clone());
                if found.len() >= limit {
                    break;
                }
            }
            if is_dir && depth < max_depth {
                queue.push_back((p.to_path_buf(), depth + 1));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_extensions_and_kinds_are_taken_from_the_text() {
        assert!(Query::parse("   ").is_empty());
        let q = Query::parse("Report .PDF");
        assert!(q.matches("Quarterly report.pdf", r"C:\x\Quarterly report.pdf", false));
        assert!(!q.matches("Quarterly report.docx", r"C:\x\Quarterly report.docx", false), "wrong extension");
        assert!(!q.matches("notes.pdf", r"C:\x\notes.pdf", false), "word missing");
        assert!(!q.matches("report", r"C:\x\report", true), "a folder has no extension");
    }

    #[test]
    fn every_word_must_be_in_the_name_in_any_order_and_case() {
        let q = Query::parse("final REPORT");
        assert!(q.matches("Report (final).txt", "a.txt", false));
        assert!(!q.matches("Report.txt", "a.txt", false));
    }

    #[test]
    fn kinds_match_by_extension_and_folders_by_being_one() {
        let images = Query::parse("kind:images");
        assert!(images.matches("a.PNG", "a.PNG", false) && !images.matches("a.pdf", "a.pdf", false));
        let folders = Query::parse("kind:folders");
        assert!(folders.matches("src", "src", true) && !folders.matches("a.png", "a.png", false));
        let either = Query::parse("kind:images kind:videos");
        assert!(either.matches("a.mp4", "a.mp4", false) && either.matches("a.jpg", "a.jpg", false) && !either.matches("a.txt", "a.txt", false));
        let half_typed = Query::parse("kind:imag");
        assert!(!half_typed.matches("a.png", "a.png", false), "an unfinished kind is a word that no name has");
    }

    #[test]
    fn a_portal_search_walks_subfolders_up_to_a_depth_and_a_limit_and_can_be_stopped() {
        let root = std::env::temp_dir().join(format!("desqu-search-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("a").join("b").join("c")).unwrap();
        for f in ["top.txt", "a/mid.txt", "a/b/deep.txt", "a/b/c/deeper.txt", "a/b/c/other.png"] {
            std::fs::write(root.join(f), "x").unwrap();
        }
        let q = Query::parse(".txt");
        let names = |v: Vec<String>| v.iter().map(|p| crate::portal::title_of(Path::new(p))).collect::<Vec<_>>();
        let never = || false;
        assert_eq!(names(search_dir(&root, &q, 50, 9, &never)), vec!["top.txt", "mid.txt", "deep.txt", "deeper.txt"]);
        assert_eq!(names(search_dir(&root, &q, 50, 1, &never)), vec!["top.txt", "mid.txt"], "one folder level below the top");
        assert_eq!(search_dir(&root, &q, 2, 9, &never).len(), 2, "the limit");
        assert!(search_dir(&root, &q, 50, 9, &|| true).is_empty(), "stopped before it began");
        let _ = std::fs::remove_dir_all(&root);
    }
}
