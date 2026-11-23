//! Ordering of the items inside a vial, the way Explorer orders a folder: folders first, then by the
//! chosen key (name, type, date modified or size). Pure, so it is unit tested.

use std::cmp::Ordering;
use std::iter::Peekable;
use std::str::Chars;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Type,
    Modified,
    Size,
}

impl SortKey {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "name" => Some(Self::Name),
            "type" => Some(Self::Type),
            "modified" => Some(Self::Modified),
            "size" => Some(Self::Size),
            _ => None,
        }
    }
}

/// What the sort needs to know about one item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemFacts {
    pub id: String,
    pub name: String,
    /// Lower case extension without the dot; empty for folders and extension-less files.
    pub ext: String,
    pub is_dir: bool,
    /// Seconds since the Unix epoch.
    pub modified: Option<u64>,
    pub size: Option<u64>,
    /// On disk. Anything else (a deleted file, a system item) goes last, in its current order.
    pub present: bool,
}

/// Names compare case-insensitively, with digit runs as numbers ("file2" before "file10").
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(p), Some(q)) if p.is_ascii_digit() && q.is_ascii_digit() => {
                let left = take_digits(&mut x);
                let right = take_digits(&mut y);
                let (left, right) = (left.trim_start_matches('0'), right.trim_start_matches('0'));
                let ord = left.len().cmp(&right.len()).then_with(|| left.cmp(right));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(p), Some(q)) => {
                if p != q {
                    return p.cmp(&q);
                }
                x.next();
                y.next();
            }
        }
    }
}

fn take_digits(it: &mut Peekable<Chars<'_>>) -> String {
    let mut digits = String::new();
    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
        digits.push(c);
        it.next();
    }
    digits
}

fn compare(a: &ItemFacts, b: &ItemFacts, key: SortKey) -> Ordering {
    let by_name = || natural_cmp(&a.name, &b.name);
    match key {
        SortKey::Name => by_name(),
        SortKey::Type => a.ext.cmp(&b.ext).then_with(by_name),
        SortKey::Modified => a.modified.cmp(&b.modified).then_with(by_name),
        SortKey::Size => a.size.cmp(&b.size).then_with(by_name),
    }
}

/// The item ids in the requested order, with folders first as Explorer does for the name. `descending`
/// reverses the chosen key only. Items that are not on disk follow, in the order they had.
pub fn sorted(items: &[ItemFacts], key: SortKey, descending: bool) -> Vec<String> {
    sorted_with(items, key, descending, false)
}

/// Like [`sorted`]. With `folders_first` folders stay on top whatever the key and direction; without it
/// only the name keeps them on top, and the other fields sort folders in with the files.
pub fn sorted_with(items: &[ItemFacts], key: SortKey, descending: bool, folders_first: bool) -> Vec<String> {
    let (mut here, gone): (Vec<&ItemFacts>, Vec<&ItemFacts>) = items.iter().partition(|i| i.present);
    let group_folders = folders_first || key == SortKey::Name;
    here.sort_by(|a, b| {
        let folders = if group_folders { b.is_dir.cmp(&a.is_dir) } else { Ordering::Equal };
        let by_key = compare(a, b, key);
        let by_key = if descending { by_key.reverse() } else { by_key };
        folders.then(by_key)
    });
    here.into_iter().chain(gone).map(|i| i.id.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(id: &str, ext: &str, modified: u64, size: u64) -> ItemFacts {
        ItemFacts {
            id: id.into(),
            name: id.rsplit_once('.').map_or(id, |(stem, _)| stem).into(),
            ext: ext.into(),
            is_dir: false,
            modified: Some(modified),
            size: Some(size),
            present: true,
        }
    }

    fn folder(id: &str) -> ItemFacts {
        ItemFacts { id: id.into(), name: id.into(), ext: String::new(), is_dir: true, modified: Some(1), size: None, present: true }
    }

    #[test]
    fn natural_order_reads_numbers_as_numbers() {
        assert_eq!(natural_cmp("file2", "file10"), Ordering::Less);
        assert_eq!(natural_cmp("File", "file"), Ordering::Equal);
        assert_eq!(natural_cmp("a01", "a1"), Ordering::Equal);
        assert_eq!(natural_cmp("b", "a10"), Ordering::Greater);
    }

    #[test]
    fn name_sort_puts_folders_first_then_natural_order() {
        let items = vec![file("file10.txt", "txt", 1, 1), file("file2.txt", "txt", 1, 1), folder("Games")];
        assert_eq!(sorted(&items, SortKey::Name, false), ["Games", "file2.txt", "file10.txt"]);
        assert_eq!(sorted(&items, SortKey::Name, true), ["Games", "file10.txt", "file2.txt"]);
    }

    #[test]
    fn folders_first_overrides_every_field_and_direction() {
        let items = vec![file("a.txt", "txt", 1, 1), folder("Games"), file("b.lnk", "lnk", 2, 2)];
        assert_eq!(sorted_with(&items, SortKey::Size, false, true)[0], "Games");
        assert_eq!(sorted_with(&items, SortKey::Type, true, true)[0], "Games");
        assert_eq!(sorted_with(&items, SortKey::Modified, true, true), ["Games", "b.lnk", "a.txt"]);
        assert_eq!(sorted_with(&items, SortKey::Type, true, false)[0], "a.txt");
    }

    #[test]
    fn only_name_sorting_keeps_folders_first() {
        let items = vec![file("a.txt", "txt", 1, 1), folder("Games"), file("b.lnk", "lnk", 2, 2)];
        assert_eq!(sorted(&items, SortKey::Name, false), ["Games", "a.txt", "b.lnk"]);
        // By type, the folder has no extension and sorts with the others, not in front of them.
        assert_eq!(sorted(&items, SortKey::Type, true), ["a.txt", "b.lnk", "Games"]);
        assert_eq!(sorted(&items, SortKey::Type, false), ["Games", "b.lnk", "a.txt"]);
    }

    #[test]
    fn type_date_and_size_use_their_own_key() {
        let items = vec![file("b.pdf", "pdf", 30, 5), file("a.txt", "txt", 10, 50), file("c.pdf", "pdf", 20, 1)];
        assert_eq!(sorted(&items, SortKey::Type, false), vec!["b.pdf", "c.pdf", "a.txt"]);
        assert_eq!(sorted(&items, SortKey::Modified, false), vec!["a.txt", "c.pdf", "b.pdf"]);
        assert_eq!(sorted(&items, SortKey::Size, false), vec!["c.pdf", "b.pdf", "a.txt"]);
        assert_eq!(sorted(&items, SortKey::Size, true), vec!["a.txt", "b.pdf", "c.pdf"]);
    }

    #[test]
    fn items_not_on_disk_go_last_in_their_order() {
        let mut gone = file("gone.txt", "txt", 0, 0);
        gone.present = false;
        let items = vec![gone, file("z.txt", "txt", 1, 1), folder("::{645FF040-5081-101B-9F08-00AA002F954E}")];
        let order = sorted(&items, SortKey::Name, false);
        assert_eq!(order[0], "::{645FF040-5081-101B-9F08-00AA002F954E}");
        assert_eq!(order[1..], ["z.txt", "gone.txt"]);
    }
}
