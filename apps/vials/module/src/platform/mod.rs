//! OS-specific window tweaks. Native code lives only here.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use self::windows::mark_as_tool_window;

#[cfg(not(windows))]
pub(crate) fn mark_as_tool_window(_hwnd: isize) -> Result<(), String> {
    Ok(())
}
