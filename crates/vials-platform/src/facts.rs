//! What a sort needs to know about an item, read from the file system.

use std::path::Path;

use vials_core::sort::ItemFacts;

/// Facts about the item `id` (a path or a `::{CLSID}` system item). `name` is the label the desktop
/// shows for it, when known.
pub fn item_facts(id: &str, name: Option<&str>) -> ItemFacts {
    let path = Path::new(id);
    let meta = std::fs::metadata(path).ok();
    let is_dir = meta.as_ref().is_some_and(|m| m.is_dir());
    let ext = if is_dir { String::new() } else { path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default() };
    let fallback = id.rsplit(['\\', '/']).next().unwrap_or(id);
    ItemFacts {
        id: id.to_string(),
        name: name.map(str::to_string).unwrap_or_else(|| fallback.to_string()),
        ext,
        is_dir,
        modified: meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs()),
        size: meta.as_ref().filter(|m| !m.is_dir()).map(|m| m.len()),
        present: meta.is_some(),
    }
}
