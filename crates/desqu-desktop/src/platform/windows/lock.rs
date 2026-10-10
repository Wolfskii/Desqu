//! Cross-process lock: a named kernel mutex whose *existence* is the lock. It is not owned by a
//! thread, so it can be dropped from any thread, and closing the last handle frees it (also when
//! the process dies).

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;

pub(super) const LOCK_NAME: &str = r"Local\Desqu.DesktopLayer";

pub(super) struct NamedLock(HANDLE);

// SAFETY: a kernel handle value is valid process-wide; closing it from another thread is fine.
unsafe impl Send for NamedLock {}
unsafe impl Sync for NamedLock {}

impl Drop for NamedLock {
    fn drop(&mut self) {
        // SAFETY: created by `acquire`, closed exactly once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// `Ok(None)` when another holder (this or another process) already has the lock.
pub(super) fn acquire(name: &str) -> Result<Option<NamedLock>, String> {
    let name = HSTRING::from(name);
    // SAFETY: `name` outlives the call.
    let handle = unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) }
        .map_err(|e| format!("creating lock {name} failed: {e}"))?;
    // CreateMutexW succeeds and returns the existing mutex when the name is already taken.
    // SAFETY: read straight after the call, on the same thread.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return Ok(None);
    }
    Ok(Some(NamedLock(handle)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_acquire_fails_until_the_first_is_dropped() {
        let name = format!(r"Local\Desqu.DesktopLayerTest.{}", std::process::id());
        let first = acquire(&name).unwrap();
        assert!(first.is_some());
        assert!(acquire(&name).unwrap().is_none());
        drop(first);
        assert!(acquire(&name).unwrap().is_some());
    }
}
