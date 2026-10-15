//! OS-specific window tweaks. Native code lives only here.

#[cfg(windows)]
pub(crate) mod clipboard;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use desqu_desktop::CutPasteWatcher;
#[cfg(windows)]
pub(crate) use self::windows::{send_to_bottom, watch_cut_paste, 
    drag_item, mark_as_tool_window, round_corners, set_context_menu, show_item_menu, DragEnd, IconsWatcher, MouseEvent, MouseWatcher,
};

#[cfg(not(windows))]
pub(crate) fn mark_as_tool_window(_hwnd: isize) -> Result<(), String> {
    Ok(())
}

/// No mouse hook off Windows.
#[cfg(not(windows))]
pub(crate) struct MouseWatcher;

#[cfg(not(windows))]
impl MouseWatcher {
    pub(crate) fn start(_on_event: MouseHandler) -> Result<Self, String> {
        Err("Vials is only available on Windows".into())
    }
}

#[cfg(not(windows))]
pub(crate) enum MouseEvent {
    Marquee((i32, i32), (i32, i32)),
    ContextDown((i32, i32)),
    ButtonDown((i32, i32)),
}

#[cfg(not(windows))]
pub(crate) type MouseHandler = Box<dyn Fn(MouseEvent) + Send>;

#[cfg(not(windows))]
pub(crate) fn set_context_menu(_exe: Option<&str>) -> Result<(), String> {
    Ok(())
}

#[cfg(not(windows))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DragEnd {
    Cancelled,
    OnDesktop((i32, i32)),
    Dropped { at: (i32, i32), accepted: bool },
}

#[cfg(not(windows))]
pub(crate) fn drag_item(_path: &str) -> Result<DragEnd, String> {
    Err("Vials is only available on Windows".into())
}


#[cfg(not(windows))]
pub(crate) fn show_item_menu(_path: &str) -> Result<(), String> {
    Err("Vials is only available on Windows".into())
}

#[cfg(not(windows))]
pub(crate) fn round_corners(_hwnd: isize, _round: bool) {}

#[cfg(not(windows))]
pub(crate) struct IconsWatcher;

#[cfg(not(windows))]
impl IconsWatcher {
    pub(crate) fn start(_on_change: Box<dyn Fn() + Send>) -> Result<Self, String> {
        Err("Vials is only available on Windows".into())
    }
}

/// No Ctrl+V hook off Windows.
#[cfg(not(windows))]
pub(crate) struct CutPasteWatcher;

#[cfg(not(windows))]
pub(crate) fn watch_cut_paste(_on_paste: Box<dyn Fn(Vec<String>) + Send>) -> Result<CutPasteWatcher, String> {
    Err("Vials is only available on Windows".into())
}

#[cfg(not(windows))]
pub(crate) fn send_to_bottom(_hwnd: isize) {}

/// No shell clipboard off Windows.
#[cfg(not(windows))]
pub(crate) mod clipboard {
    pub(crate) struct ClipboardFiles {
        pub paths: Vec<String>,
        pub cut: bool,
    }
    const UNSUPPORTED: &str = "Vials is only available on Windows";
    pub(crate) fn put_files(_paths: &[String], _cut: bool) -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }
    pub(crate) fn files() -> Result<Option<ClipboardFiles>, String> {
        Err(UNSUPPORTED.into())
    }
    pub(crate) fn clear() {}
    pub(crate) fn transfer(_paths: &[String], _cut: bool, _folder: &str, _rename: bool) -> Result<Vec<String>, String> {
        Err(UNSUPPORTED.into())
    }
}
