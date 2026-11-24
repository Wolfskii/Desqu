//! What Desqu's Workspaces page shows and does: one card per virtual desktop with the items tied
//! to it, and the items shown on every desktop, each of which can be given another desktop. A
//! running Workspaces takes the change itself (it also hides and shows the icons); with Workspaces
//! off the config file is changed, and the icons follow when it starts.

use std::path::Path;

use desqu_native_shell::tray;
use vials_core::model::VialsConfig;
use workspaces_core::model::{desktop_label, vial_item, vial_of, WorkspacesConfig};

use crate::modules::Kind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub id: String,
    pub name: String,
    pub current: bool,
    pub removed: bool,
    /// Its apps and folders open the first time you come to it in a session.
    pub open_on_visit: bool,
    /// The wallpaper picture it puts on, if any.
    pub wallpaper: Option<String>,
    pub items: Vec<Item>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Page {
    pub cards: Vec<Card>,
    /// Items on every desktop (nothing owns them).
    pub shared: Vec<Item>,
    /// Windows has more than one virtual desktop.
    pub has_desktops: bool,
    /// The first-run question has not been answered.
    pub first_run: bool,
}

/// A file by its name (a shortcut without `.lnk` / `.url`), a vial by its own name.
pub fn label(id: &str, vials: &VialsConfig) -> String {
    match vial_of(id) {
        Some(v) => format!("Vial: {}", vials.vial(v).map_or(v, |x| x.name.as_str())),
        None => {
            let name = id.rsplit(['\\', '/']).next().unwrap_or(id);
            [".lnk", ".url"].iter().find_map(|e| name.strip_suffix(e)).unwrap_or(name).to_string()
        }
    }
}

/// The items sitting in the Desktop folders (including the ones that are hidden because they are in a
/// vial or on another virtual desktop), without the folders' bookkeeping files.
fn desktop_files(folders: &[std::path::PathBuf]) -> Vec<String> {
    let mut out = Vec::new();
    for folder in folders {
        let Ok(entries) = std::fs::read_dir(folder) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.eq_ignore_ascii_case("desktop.ini") || name.eq_ignore_ascii_case("thumbs.db") {
                continue;
            }
            out.push(entry.path().to_string_lossy().into_owned());
        }
    }
    out.sort_by_key(|id| id.rsplit(['\\', '/']).next().unwrap_or(id).to_lowercase());
    out
}

/// Builds the page from its inputs (pure, so tested).
pub fn build(cfg: &WorkspacesConfig, desktops: &workspaces_platform::Desktops, vials: &VialsConfig, files: &[String]) -> Page {
    // A vial that was deleted is tied to nothing any more.
    let alive = |item: &String| vial_of(item).is_none_or(|v| vials.vial(v).is_some());
    let item = |id: &String| Item { id: id.clone(), label: label(id, vials) };
    let mut cards: Vec<Card> = desktops
        .list
        .iter()
        .enumerate()
        .map(|(i, d)| Card {
            id: d.id.clone(),
            name: desktop_label(i, &d.name),
            current: desktops.current.as_deref() == Some(d.id.as_str()),
            removed: false,
            open_on_visit: cfg.workspace(&d.id).is_some_and(|w| w.open_on_visit),
            wallpaper: cfg.workspace(&d.id).and_then(|w| w.wallpaper.clone()),
            items: cfg.workspace(&d.id).map(|w| w.items.iter().filter(|i| alive(i)).map(item).collect()).unwrap_or_default(),
        })
        .collect();
    for ws in cfg.workspaces.iter().filter(|w| !desktops.list.iter().any(|d| d.id == w.desktop_id)) {
        cards.push(Card {
            id: ws.desktop_id.clone(),
            name: if ws.name.is_empty() { "Removed desktop".into() } else { ws.name.clone() },
            current: false,
            removed: true,
            open_on_visit: false,
            wallpaper: None,
            items: ws.items.iter().filter(|i| alive(i)).map(item).collect(),
        });
    }
    // Files that a vial holds are not "on every desktop": the vial is what shows (or not).
    let in_vials: std::collections::HashSet<String> = vials.assigned_ids().into_iter().map(|i| i.to_lowercase()).collect();
    let mut shared: Vec<Item> = files
        .iter()
        .filter(|f| cfg.owner_of(f).is_none() && !in_vials.contains(&f.to_lowercase()))
        .map(item)
        .collect();
    shared.extend(vials.vials.iter().filter(|v| !v.inbox).map(|v| vial_item(&v.id)).filter(|i| cfg.owner_of(i).is_none()).map(|i| item(&i)));
    Page { cards, shared, has_desktops: !desktops.list.is_empty(), first_run: !cfg.initialized && !desktops.list.is_empty() }
}

/// The page as it is now.
pub fn load() -> Page {
    let cfg = WorkspacesConfig::load(&crate::app_settings::workspaces_path()).unwrap_or_default();
    let desktops = workspaces_platform::desktops().unwrap_or_default();
    build(&cfg, &desktops, &crate::app_settings::vials(), &desktop_files(&desqu_desktop::desktop_folders()))
}

/// Ties `item` to a virtual desktop, or with `None` shows it on every desktop.
pub fn assign(running: bool, item: &str, desktop: Option<&str>) -> Result<(), String> {
    if running && tray::forward_action(&Kind::Workspaces.identity(), &format!("assign-item:{item}:{}", desktop.unwrap_or("all"))) {
        return Ok(());
    }
    let path = crate::app_settings::workspaces_path();
    let desktops = workspaces_platform::desktops().unwrap_or_default();
    assign_in_file(&path, &desktops, item, desktop)
}

fn assign_in_file(path: &Path, desktops: &workspaces_platform::Desktops, item: &str, desktop: Option<&str>) -> Result<(), String> {
    let mut cfg = WorkspacesConfig::load(path).map_err(|e| e.to_string())?;
    let target = match desktop {
        None => None,
        Some(id) => {
            let index = desktops.list.iter().position(|d| d.id == id).ok_or("that virtual desktop does not exist")?;
            Some((id.to_string(), desktop_label(index, &desktops.list[index].name)))
        }
    };
    cfg.assign(item, target.as_ref().map(|(id, name)| (id.as_str(), name.as_str())));
    cfg.save(path).map_err(|e| e.to_string())
}

/// Turns "open its items the first time you come here" on or off for a desktop.
pub fn set_open_on_visit(running: bool, desktop: &str, on: bool) -> Result<(), String> {
    if running && tray::forward_action(&Kind::Workspaces.identity(), &format!("open-on-visit:{desktop}:{on}")) {
        return Ok(());
    }
    let path = crate::app_settings::workspaces_path();
    let desktops = workspaces_platform::desktops().unwrap_or_default();
    let index = desktops.list.iter().position(|d| d.id == desktop).ok_or("that virtual desktop does not exist")?;
    let mut cfg = WorkspacesConfig::load(&path).map_err(|e| e.to_string())?;
    cfg.set_open_on_visit((desktop, &desktop_label(index, &desktops.list[index].name)), on);
    cfg.save(&path).map_err(|e| e.to_string())
}

/// "Use the wallpaper I have now for this desktop" (`keep`), or take it away.
pub fn set_wallpaper(running: bool, desktop: &str, keep: bool) -> Result<(), String> {
    if running && tray::forward_action(&Kind::Workspaces.identity(), &format!("wallpaper:{desktop}:{}", if keep { "keep" } else { "clear" })) {
        return Ok(());
    }
    let path = crate::app_settings::workspaces_path();
    let desktops = workspaces_platform::desktops().unwrap_or_default();
    let index = desktops.list.iter().position(|d| d.id == desktop).ok_or("that virtual desktop does not exist")?;
    let picture = if keep { Some(workspaces_platform::current_wallpaper().ok_or("the wallpaper is a plain colour, not a picture")?) } else { None };
    let mut cfg = WorkspacesConfig::load(&path).map_err(|e| e.to_string())?;
    cfg.set_wallpaper((desktop, &desktop_label(index, &desktops.list[index].name)), picture.as_deref());
    cfg.save(&path).map_err(|e| e.to_string())
}

/// Shows everything a desktop holds on every desktop, and forgets the desktop.
pub fn release(running: bool, desktop: &str) -> Result<(), String> {
    if running && tray::forward_action(&Kind::Workspaces.identity(), &format!("release-desktop:{desktop}")) {
        return Ok(());
    }
    let path = crate::app_settings::workspaces_path();
    let mut cfg = WorkspacesConfig::load(&path).map_err(|e| e.to_string())?;
    cfg.remove_workspace(desktop);
    cfg.save(&path).map_err(|e| e.to_string())
}

/// The first-run question: every item nobody owns goes to this desktop (or stays everywhere). Needs
/// Workspaces to run, because it is what sees the desktop items.
pub fn first_run(desktop: Option<&str>) -> Result<(), String> {
    if tray::forward_action(&Kind::Workspaces.identity(), &format!("first-run:{}", desktop.unwrap_or("none"))) {
        Ok(())
    } else {
        Err("Turn Workspaces on to answer this".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use workspaces_platform::{DesktopInfo, Desktops};

    fn desktops() -> Desktops {
        Desktops {
            list: vec![DesktopInfo { id: "{00000000-0000-0000-0000-00000000000A}".into(), name: String::new() }, DesktopInfo { id: "{00000000-0000-0000-0000-00000000000B}".into(), name: "Work".into() }],
            current: Some("{00000000-0000-0000-0000-00000000000B}".into()),
        }
    }

    #[test]
    fn items_land_on_their_desktop_and_the_rest_are_shared() {
        let mut cfg = WorkspacesConfig::default();
        cfg.assign("C:/d/a.lnk", Some(("{00000000-0000-0000-0000-00000000000A}", "Desktop 1")));
        cfg.assign("C:/d/gone.txt", Some(("{00000000-0000-0000-0000-00000000000C}", "Old")));
        let page = build(&cfg, &desktops(), &VialsConfig::default(), &["C:/d/a.lnk".into(), "C:/d/b.txt".into()]);
        assert_eq!(page.cards[0].name, "Desktop 1");
        assert_eq!(page.cards[0].items, vec![Item { id: "C:/d/a.lnk".into(), label: "a".into() }]);
        assert!(page.cards[1].current && page.cards[1].name == "Work");
        assert!(page.cards[2].removed, "a workspace whose desktop is gone is listed as removed");
        assert_eq!(page.shared.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), vec!["b.txt"]);
        assert!(page.has_desktops && page.first_run);
    }

    #[test]
    fn assigning_in_the_file_moves_an_item_and_refuses_an_unknown_desktop() {
        let path = std::env::temp_dir().join(format!("desqu-ws-page-{}", std::process::id())).join("config.json");
        assign_in_file(&path, &desktops(), "C:/d/b.txt", Some("{00000000-0000-0000-0000-00000000000B}")).unwrap();
        assert_eq!(WorkspacesConfig::load(&path).unwrap().owner_of("C:/d/b.txt"), Some("{00000000-0000-0000-0000-00000000000B}"));
        assign_in_file(&path, &desktops(), "C:/d/b.txt", None).unwrap();
        assert_eq!(WorkspacesConfig::load(&path).unwrap().owner_of("C:/d/b.txt"), None);
        assert!(assign_in_file(&path, &desktops(), "C:/d/b.txt", Some("{00000000-0000-0000-0000-00000000000F}")).is_err());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
