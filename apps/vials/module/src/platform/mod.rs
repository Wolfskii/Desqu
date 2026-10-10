//! OS-specific window tweaks. Native code lives only here.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use self::windows::{mark_as_tool_window, MouseWatcher};

#[cfg(not(windows))]
pub(crate) fn mark_as_tool_window(_hwnd: isize) -> Result<(), String> {
    Ok(())
}

/// No mouse hook off Windows.
#[cfg(not(windows))]
pub(crate) struct MouseWatcher;

#[cfg(not(windows))]
impl MouseWatcher {
    pub(crate) fn start(_on_marquee: Box<dyn Fn((i32, i32), (i32, i32)) + Send>) -> Result<Self, String> {
        Err("Vials is only available on Windows".into())
    }
}
