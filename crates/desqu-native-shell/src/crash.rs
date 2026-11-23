//! Says where a native crash happened. A Rust panic prints its message; an access violation inside
//! Windows or GPUI code ends the process silently. This writes the exception code, the faulting address
//! with the module it lies in, and the call chain (as module + offset) to stderr, which Desqu keeps in
//! the module's log file.

use std::fmt::Write as _;

use windows::Win32::Foundation::{HMODULE, MAX_PATH};
use windows::Win32::System::Diagnostics::Debug::{RtlCaptureStackBackTrace, SetUnhandledExceptionFilter, EXCEPTION_POINTERS};
use windows::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT};

/// `name+0xoffset` of the module that holds `address`.
fn locate(address: usize) -> String {
    // SAFETY: queries about an address; the buffer is ours.
    unsafe {
        let mut module = HMODULE::default();
        let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
        if GetModuleHandleExW(flags, windows::core::PCWSTR(address as *const u16), &mut module).is_err() {
            return format!("{address:#x} (no module)");
        }
        let mut buf = [0u16; MAX_PATH as usize];
        let n = GetModuleFileNameW(Some(module), &mut buf) as usize;
        let path = String::from_utf16_lossy(&buf[..n.min(buf.len())]);
        let name = path.rsplit('\\').next().unwrap_or(&path);
        format!("{name}+{:#x}", address - module.0 as usize)
    }
}

unsafe extern "system" fn filter(info: *const EXCEPTION_POINTERS) -> i32 {
    let mut text = String::from("--- native crash\n");
    if let Some(info) = unsafe { info.as_ref() } {
        if let Some(record) = unsafe { info.ExceptionRecord.as_ref() } {
            let _ = writeln!(text, "exception {:#x} at {}", record.ExceptionCode.0 as u32, locate(record.ExceptionAddress as usize));
            // For an access violation: 0 = read, 1 = write, and the address that was touched.
            if record.NumberParameters >= 2 {
                let _ = writeln!(text, "access {} of {:#x}", record.ExceptionInformation[0], record.ExceptionInformation[1]);
            }
        }
    }
    let mut frames = [std::ptr::null_mut::<std::ffi::c_void>(); 32];
    let count = unsafe { RtlCaptureStackBackTrace(0, &mut frames, None) } as usize;
    for frame in &frames[..count] {
        let _ = writeln!(text, "  at {}", locate(*frame as usize));
    }
    eprintln!("{text}");
    0 // EXCEPTION_CONTINUE_SEARCH: Windows ends the process as usual
}

/// Call once at startup (after [`crate::stdio::protect`]).
pub fn install() {
    // SAFETY: registers a process-wide filter that only reads the exception data.
    unsafe {
        SetUnhandledExceptionFilter(Some(filter));
    }
}
