//! OS-specific window tweaks. Native code lives only here.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use self::windows::{
    drag_item, mark_as_tool_window, round_corners, set_context_menu, DragEnd, IconsWatcher, MouseEvent, MouseWatcher,
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
pub(crate) fn round_corners(_hwnd: isize, _round: bool) {}

#[cfg(not(windows))]
pub(crate) struct IconsWatcher;

#[cfg(not(windows))]
impl IconsWatcher {
    pub(crate) fn start(_on_change: Box<dyn Fn() + Send>) -> Result<Self, String> {
        Err("Vials is only available on Windows".into())
    }
}
