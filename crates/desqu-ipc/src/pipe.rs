//! Local named pipe between the native host and the engine (Windows only).
//!
//! The server creates the first instance with `FILE_FLAG_FIRST_PIPE_INSTANCE`, so another process
//! cannot take the name first. The DACL allows only SYSTEM and the owner (the current user).
//!
//! Handles are overlapped, and each operation waits on its own event. That lets one thread block in
//! `read` while another writes on a clone of the same pipe, which the bridge needs to push
//! unsolicited messages to the browser.

use std::io::{self, Read, Write};
use std::sync::Arc;

use windows::core::{HRESULT, PCWSTR};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_BROKEN_PIPE, ERROR_IO_PENDING, ERROR_NO_DATA, ERROR_PIPE_BUSY,
    ERROR_PIPE_CONNECTED, GENERIC_READ, GENERIC_WRITE, HANDLE, HLOCAL, LocalFree,
};
use windows::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED,
    FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_MODE, FlushFileBuffers, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
    ReadFile, WriteFile,
};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT, WaitNamedPipeW,
};
use windows::Win32::System::Threading::CreateEventW;

/// The name the extension's native host connects to.
pub const BRIDGE_PIPE_NAME: &str = r"\\.\pipe\desqu-browser-bridge";

/// SYSTEM and the owner (the current user) get full access. Nobody else.
pub const BRIDGE_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GA;;;OW)";

const BUFFER_SIZE: u32 = 64 * 1024;
/// Keep the client waiting briefly if all instances are busy (ms).
const BUSY_WAIT_MS: u32 = 2_000;
const CONNECT_ATTEMPTS: u32 = 3;

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn is_code(e: &windows::core::Error, code: windows::Win32::Foundation::WIN32_ERROR) -> bool {
    e.code() == HRESULT::from_win32(code.0)
}

struct Inner {
    handle: HANDLE,
    /// Server ends disconnect the client on drop; client ends only close.
    server: bool,
}

// Overlapped operations on one handle may run from several threads; each carries its own event.
unsafe impl Send for Inner {}
unsafe impl Sync for Inner {}

impl Drop for Inner {
    fn drop(&mut self) {
        unsafe {
            if self.server {
                let _ = DisconnectNamedPipe(self.handle);
            }
            let _ = CloseHandle(self.handle);
        }
    }
}

/// A connected pipe end. Clones share the connection: one can read while another writes.
#[derive(Clone)]
pub struct Pipe {
    inner: Arc<Inner>,
}

impl Pipe {
    fn new(handle: HANDLE, server: bool) -> Self {
        Self {
            inner: Arc::new(Inner { handle, server }),
        }
    }

    /// Breaks any read or write blocked on this pipe, on any clone. The peer sees the pipe close.
    pub fn close(&self) {
        unsafe {
            let _ = CancelIoEx(self.inner.handle, None);
            if self.inner.server {
                let _ = DisconnectNamedPipe(self.inner.handle);
            }
        }
    }
}

/// Runs one overlapped operation to completion, with its own event.
fn overlapped(
    handle: HANDLE,
    start: impl FnOnce(*mut OVERLAPPED) -> windows::core::Result<()>,
) -> windows::core::Result<u32> {
    let event = unsafe { CreateEventW(None, true, false, PCWSTR::null())? };
    let mut ov = OVERLAPPED {
        hEvent: event,
        ..Default::default()
    };
    let result = match start(&mut ov) {
        Ok(()) => Ok(()),
        Err(e) if is_code(&e, ERROR_IO_PENDING) => Ok(()),
        Err(e) => Err(e),
    };
    let outcome = result.and_then(|()| {
        let mut transferred = 0u32;
        unsafe { GetOverlappedResult(handle, &ov, &mut transferred, true) }
            .map(|()| transferred)
    });
    unsafe {
        let _ = CloseHandle(event);
    }
    outcome
}

impl Read for Pipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let len = buf.len().min(BUFFER_SIZE as usize);
        if len == 0 {
            return Ok(0);
        }
        let handle = self.inner.handle;
        let slice = &mut buf[..len];
        match overlapped(handle, |ov| unsafe { ReadFile(handle, Some(slice), None, Some(ov)) }) {
            Ok(n) => Ok(n as usize),
            // The other end closed: that is end of stream, not an error.
            Err(e) if is_code(&e, ERROR_BROKEN_PIPE) => Ok(0),
            Err(e) => Err(io::Error::other(e)),
        }
    }
}

impl Write for Pipe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let len = buf.len().min(BUFFER_SIZE as usize);
        let handle = self.inner.handle;
        let slice = &buf[..len];
        match overlapped(handle, |ov| unsafe { WriteFile(handle, Some(slice), None, Some(ov)) }) {
            Ok(n) => Ok(n as usize),
            Err(e) if is_code(&e, ERROR_NO_DATA) || is_code(&e, ERROR_BROKEN_PIPE) => {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, e))
            }
            Err(e) => Err(io::Error::other(e)),
        }
    }

    /// Waits until the other end has read everything written. Needed before a server disconnects:
    /// `DisconnectNamedPipe` throws away unread data.
    fn flush(&mut self) -> io::Result<()> {
        unsafe { FlushFileBuffers(self.inner.handle) }.map_err(io::Error::other)
    }
}

/// Owns a security descriptor built from SDDL; freed on drop.
struct SecurityDescriptor(PSECURITY_DESCRIPTOR);

impl SecurityDescriptor {
    fn from_sddl(sddl: &str) -> io::Result<Self> {
        let wide = to_wide(sddl);
        let mut sd = PSECURITY_DESCRIPTOR(std::ptr::null_mut());
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(wide.as_ptr()),
                1, // SDDL_REVISION_1
                &mut sd,
                None,
            )
        }
        .map_err(io::Error::other)?;
        Ok(Self(sd))
    }
}

impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        unsafe {
            let _ = LocalFree(Some(HLOCAL(self.0 .0)));
        }
    }
}

/// Server side: hands out connected `Pipe`s, one per client.
pub struct PipeListener {
    name: Vec<u16>,
    // Kept alive because `attributes` points into it.
    _descriptor: SecurityDescriptor,
    attributes: SECURITY_ATTRIBUTES,
    /// Instance created ahead of the next client, so a client never sees "no such pipe".
    next: Option<HANDLE>,
}

// The raw pointer in `attributes` points at `_descriptor`, owned by this same value, so moving the
// whole listener to another thread keeps it valid. It is still used from one thread at a time.
unsafe impl Send for PipeListener {}

impl PipeListener {
    /// Claims `name`. Fails if another process already owns it.
    pub fn bind(name: &str) -> io::Result<Self> {
        let descriptor = SecurityDescriptor::from_sddl(BRIDGE_SDDL)?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0 .0,
            bInheritHandle: false.into(),
        };
        let wide = to_wide(name);
        let first = create_instance(&wide, &attributes, true)?;
        Ok(Self {
            name: wide,
            _descriptor: descriptor,
            attributes,
            next: Some(first),
        })
    }

    /// Blocks until a client connects. Also used to wake a server that is shutting down: a
    /// throwaway `connect` makes this return.
    pub fn accept(&mut self) -> io::Result<Pipe> {
        let handle = match self.next.take() {
            Some(h) => h,
            None => create_instance(&self.name, &self.attributes, false)?,
        };
        let connected = overlapped(handle, |ov| unsafe { ConnectNamedPipe(handle, Some(ov)) });
        match connected {
            Ok(_) => {}
            // The client connected between create and connect: that is success.
            Err(e) if is_code(&e, ERROR_PIPE_CONNECTED) => {}
            Err(e) => {
                unsafe {
                    let _ = CloseHandle(handle);
                }
                return Err(io::Error::other(e));
            }
        }
        // Ready the next instance before handing this one out.
        self.next = Some(create_instance(&self.name, &self.attributes, false)?);
        Ok(Pipe::new(handle, true))
    }
}

impl Drop for PipeListener {
    fn drop(&mut self) {
        if let Some(h) = self.next.take() {
            unsafe {
                let _ = CloseHandle(h);
            }
        }
    }
}

fn create_instance(name: &[u16], attributes: &SECURITY_ATTRIBUTES, first: bool) -> io::Result<HANDLE> {
    let mut open_mode = PIPE_ACCESS_DUPLEX.0 | FILE_FLAG_OVERLAPPED.0;
    if first {
        open_mode |= FILE_FLAG_FIRST_PIPE_INSTANCE.0;
    }
    let handle = unsafe {
        CreateNamedPipeW(
            PCWSTR(name.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(open_mode),
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            PIPE_UNLIMITED_INSTANCES,
            BUFFER_SIZE,
            BUFFER_SIZE,
            0,
            Some(attributes as *const SECURITY_ATTRIBUTES),
        )
    };
    if handle.is_invalid() {
        return Err(io::Error::other(windows::core::Error::from_thread()));
    }
    Ok(handle)
}

/// Client side: connects to a running server, waiting briefly if every instance is busy.
pub fn connect(name: &str) -> io::Result<Pipe> {
    let wide = to_wide(name);
    let mut last = None;
    for _ in 0..CONNECT_ATTEMPTS {
        let opened = unsafe {
            CreateFileW(
                PCWSTR(wide.as_ptr()),
                GENERIC_READ.0 | GENERIC_WRITE.0,
                FILE_SHARE_MODE(0),
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(FILE_ATTRIBUTE_NORMAL.0 | FILE_FLAG_OVERLAPPED.0),
                None,
            )
        };
        match opened {
            Ok(handle) => return Ok(Pipe::new(handle, false)),
            Err(e) if is_code(&e, ERROR_PIPE_BUSY) => {
                unsafe {
                    let _ = WaitNamedPipeW(PCWSTR(wide.as_ptr()), BUSY_WAIT_MS);
                }
                last = Some(e);
            }
            Err(e) => return Err(io::Error::other(e)),
        }
    }
    Err(io::Error::other(last.unwrap_or_else(windows::core::Error::from_thread)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{read_frame, write_frame};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn unique_name() -> String {
        format!(
            r"\\.\pipe\desqu-ipc-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        )
    }

    #[test]
    fn a_client_and_server_exchange_frames() {
        let name = unique_name();
        let mut listener = PipeListener::bind(&name).unwrap();
        let server = std::thread::spawn(move || {
            let mut pipe = listener.accept().unwrap();
            let got = read_frame(&mut pipe).unwrap().unwrap();
            write_frame(&mut pipe, &got).unwrap();
            // Keep the listener alive until the client has read the reply.
            drop(pipe);
            listener
        });

        let mut client = connect(&name).unwrap();
        write_frame(&mut client, b"{\"type\":\"bridgeStatus\",\"connected\":true}").unwrap();
        let echoed = read_frame(&mut client).unwrap().unwrap();
        assert_eq!(echoed, b"{\"type\":\"bridgeStatus\",\"connected\":true}");
        drop(client);
        drop(server.join().unwrap());
    }

    #[test]
    fn the_server_can_push_while_its_reader_is_blocked() {
        // The bridge pushes rule changes to the browser while a reader waits for the next request.
        let name = unique_name();
        let mut listener = PipeListener::bind(&name).unwrap();
        let server = std::thread::spawn(move || {
            let pipe = listener.accept().unwrap();
            let mut reader = pipe.clone();
            let mut writer = pipe.clone();
            let reading = std::thread::spawn(move || read_frame(&mut reader));
            // Give the reader time to block, then push while it still waits.
            std::thread::sleep(Duration::from_millis(200));
            write_frame(&mut writer, b"pushed").unwrap();
            let request = reading.join().unwrap().unwrap().unwrap();
            (listener, request)
        });

        let mut client = connect(&name).unwrap();
        let client_reader = client.clone();
        let mut client_reader = client_reader;
        let pushed = std::thread::spawn(move || read_frame(&mut client_reader).unwrap().unwrap());
        // The client sends a request only after the push has arrived, so the server's reader
        // blocked the whole time.
        assert_eq!(pushed.join().unwrap(), b"pushed");
        write_frame(&mut client, b"request").unwrap();

        let (listener, request) = server.join().unwrap();
        assert_eq!(request, b"request");
        drop(listener);
    }

    #[test]
    fn close_releases_a_blocked_reader() {
        let name = unique_name();
        let mut listener = PipeListener::bind(&name).unwrap();
        let server = std::thread::spawn(move || {
            let pipe = listener.accept().unwrap();
            let mut reader = pipe.clone();
            let blocked = std::thread::spawn(move || read_frame(&mut reader));
            std::thread::sleep(Duration::from_millis(100));
            pipe.close();
            let outcome = blocked.join().unwrap();
            (listener, outcome.is_err() || matches!(outcome, Ok(None)))
        });
        let _client = connect(&name).unwrap();
        let (listener, released) = server.join().unwrap();
        assert!(released, "a closed pipe must end the blocked read");
        drop(listener);
    }

    #[test]
    fn a_second_server_cannot_take_a_name_that_is_held() {
        let name = unique_name();
        let _first = PipeListener::bind(&name).unwrap();
        let second = PipeListener::bind(&name);
        assert!(second.is_err(), "first-instance flag must stop name squatting");
    }

    #[test]
    fn connecting_to_a_missing_pipe_fails() {
        assert!(connect(&unique_name()).is_err());
    }

    #[test]
    fn the_descriptor_builds_from_sddl() {
        assert!(SecurityDescriptor::from_sddl(BRIDGE_SDDL).is_ok());
        assert!(SecurityDescriptor::from_sddl("not sddl").is_err());
    }
}
