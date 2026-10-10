//! Event source: `ReadDirectoryChangesW` on the desktop folders. The spike measured it reporting a
//! new file immediately, while the shell's own change notifications never arrived. Nothing here
//! polls: each thread sleeps until the OS wakes it or the subscription stops it.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadDirectoryChangesW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OVERLAPPED, FILE_LIST_DIRECTORY,
    FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Threading::{
    CreateEventW, SetEvent, WaitForMultipleObjects, WaitForSingleObject, INFINITE,
};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};

use crate::layer::{DesktopEvent, DesktopResult, EventSink, Subscription};

/// A burst of changes (copying many files) becomes one event.
const COALESCE_MS: u32 = 150;

#[derive(Clone, Copy)]
struct SendHandle(HANDLE);
// SAFETY: kernel handle values are valid process-wide.
unsafe impl Send for SendHandle {}

pub(super) fn watch(folders: Vec<PathBuf>, sink: EventSink) -> DesktopResult<Subscription> {
    // SAFETY: plain event creation; closed when the subscription is dropped.
    let stop = unsafe { CreateEventW(None, true, false, None) }
        .map(SendHandle)
        .map_err(|e| super::shell::native("CreateEventW", e))?;
    let sink = Arc::new(Mutex::new(sink));
    let mut threads = Vec::new();
    for folder in folders {
        let sink = sink.clone();
        let handle = std::thread::Builder::new()
            .name("desqu-desktop-watch".into())
            .spawn(move || {
                // SAFETY: `stop` stays open until every watcher thread has been joined.
                unsafe { run(&folder, stop, &sink) }
            })?;
        threads.push(handle);
    }
    Ok(Subscription::new(move || {
        // Capture the whole `Send` wrapper, not just its raw-pointer field.
        let stop = stop;
        // SAFETY: wakes the watchers, waits for them, then closes the event they were using.
        unsafe {
            let _ = SetEvent(stop.0);
        }
        for t in threads {
            let _ = t.join();
        }
        unsafe {
            let _ = CloseHandle(stop.0);
        }
    }))
}

unsafe fn run(folder: &Path, stop: SendHandle, sink: &Arc<Mutex<EventSink>>) {
    let wide: Vec<u16> = folder.to_string_lossy().encode_utf16().chain(Some(0)).collect();
    let dir = match CreateFileW(
        PCWSTR(wide.as_ptr()),
        FILE_LIST_DIRECTORY.0,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        None,
        OPEN_EXISTING,
        FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
        None,
    ) {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!("cannot watch {}: {e}", folder.display());
            return;
        }
    };
    let Ok(event) = CreateEventW(None, false, false, None) else {
        let _ = CloseHandle(dir);
        return;
    };
    // 16 KiB, 8-byte aligned as the API requires.
    let mut buffer = vec![0u64; 2048];

    loop {
        let mut overlapped = OVERLAPPED { hEvent: event, ..Default::default() };
        let armed = ReadDirectoryChangesW(
            dir,
            buffer.as_mut_ptr().cast(),
            (buffer.len() * 8) as u32,
            false,
            FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_DIR_NAME,
            None,
            Some(&mut overlapped),
            None,
        );
        if armed.is_err() {
            break;
        }
        let which = WaitForMultipleObjects(&[event, stop.0], false, INFINITE);
        if which != WAIT_OBJECT_0 {
            // Stopped (or failed): cancel and wait for the cancellation before the buffer goes away.
            let _ = CancelIoEx(dir, Some(&overlapped));
            let mut bytes = 0u32;
            let _ = GetOverlappedResult(dir, &overlapped, &mut bytes, true);
            break;
        }
        // Coalesce a burst, but wake at once when asked to stop.
        if WaitForSingleObject(stop.0, COALESCE_MS) == WAIT_OBJECT_0 {
            break;
        }
        if let Ok(sink) = sink.lock() {
            sink(DesktopEvent::Changed);
        }
    }
    let _ = CloseHandle(event);
    let _ = CloseHandle(dir);
}
