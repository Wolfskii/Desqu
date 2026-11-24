//! Keeps a closed output pipe from killing the app. Rust's `eprintln!` panics when writing fails, and a
//! panic inside a callback ends the whole program. An app launched by something that has since gone away
//! (a terminal, a launcher) inherits a pipe nobody reads, so any message it prints would crash it.

/// Call first thing in `main`. Output that goes to a pipe is sent to NUL instead; a console, a file
/// (Desqu keeps module output in a log file) or no handle at all is left alone.
pub fn protect() {
    if std::env::var_os("DESQU_KEEP_STDIO").is_some() {
        return; // for diagnosing: leave the handles as the parent gave them
    }
    use windows::core::w;
    use windows::Win32::Foundation::{GENERIC_WRITE, HANDLE};
    use windows::Win32::Storage::FileSystem::{CreateFileW, GetFileType, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TYPE_PIPE, OPEN_EXISTING};
    use windows::Win32::System::Console::{GetStdHandle, SetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE};
    // SAFETY: plain handle queries and one NUL handle that lives as long as the process.
    unsafe {
        let mut nul: Option<HANDLE> = None;
        for which in [STD_ERROR_HANDLE, STD_OUTPUT_HANDLE] {
            let Ok(handle) = GetStdHandle(which) else { continue };
            if handle.is_invalid() || GetFileType(handle) != FILE_TYPE_PIPE {
                continue;
            }
            if nul.is_none() {
                nul = CreateFileW(w!("NUL"), GENERIC_WRITE.0, FILE_SHARE_READ | FILE_SHARE_WRITE, None, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, None).ok();
            }
            if let Some(nul) = nul {
                let _ = SetStdHandle(which, nul);
            }
        }
    }
}
