//! The app's presence in Windows: one running copy (a second launch hands its `--action=<name>` to the
//! first and exits) and a tray icon with a small menu. A hidden window on a thread of its own owns
//! both; what happens in it reaches the app through a channel.

use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Shell::{Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW, FindWindowW, GetCursorPos,
    GetMessageW, LoadIconW, PostMessageW, PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SendMessageW,
    SetForegroundWindow, TrackPopupMenu, TranslateMessage, IDI_APPLICATION, MF_STRING, MSG, TPM_BOTTOMALIGN, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, WM_APP, WM_CLOSE, WM_COPYDATA, WM_DESTROY, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WNDCLASSW,
    WS_EX_TOOLWINDOW,
};

/// What the tray thread tells the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A second launch asked for this (`--action=create-vial`).
    Action(String),
    /// "New vial" in the tray menu.
    NewVial,
    /// "Settings" in the tray menu.
    Settings,
    /// "Quit" in the tray menu.
    Quit,
}

const CLASS: PCWSTR = w!("VialsGpuiTrayWindow");
const TRAY_MESSAGE: u32 = WM_APP + 1;
const CMD_NEW: usize = 1;
const CMD_QUIT: usize = 2;
const CMD_SETTINGS: usize = 3;
const COPYDATA_ID: usize = 0x5649_414C; // "VIAL"

static SINK: OnceLock<Mutex<Sender<Event>>> = OnceLock::new();
static TASKBAR_CREATED: OnceLock<u32> = OnceLock::new();

fn send(event: Event) {
    if let Some(sink) = SINK.get() {
        let _ = sink.lock().unwrap_or_else(|e| e.into_inner()).send(event);
    }
}

/// Held for as long as this is the running copy.
pub struct SingleInstance(#[allow(dead_code)] HANDLE);

/// `Some` for the first copy; `None` when another one is already running.
pub fn single_instance() -> Option<SingleInstance> {
    // SAFETY: a named mutex owned for the life of the process.
    unsafe {
        let handle = CreateMutexW(None, true, w!("Local\\vials-gpui-single-instance")).ok()?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return None;
        }
        Some(SingleInstance(handle))
    }
}

/// Hands an action to the running copy. Returns whether it was delivered.
pub fn forward_action(action: &str) -> bool {
    // SAFETY: the data outlives the synchronous SendMessage.
    unsafe {
        let Ok(target) = FindWindowW(CLASS, None) else { return false };
        let bytes = action.as_bytes();
        let data = COPYDATASTRUCT { dwData: COPYDATA_ID, cbData: bytes.len() as u32, lpData: bytes.as_ptr() as *mut _ };
        SendMessageW(target, WM_COPYDATA, Some(WPARAM(0)), Some(LPARAM(&data as *const _ as isize))).0 != 0
    }
}

pub struct Tray {
    hwnd: isize,
}

impl Drop for Tray {
    fn drop(&mut self) {
        // SAFETY: asks the tray thread's window to close itself (which removes the icon).
        unsafe {
            let _ = PostMessageW(Some(HWND(self.hwnd as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

fn add_icon(hwnd: HWND) {
    // SAFETY: plain shell calls with a fully initialised structure.
    unsafe {
        let mut nid = NOTIFYICONDATAW { cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32, hWnd: hwnd, uID: 1, ..Default::default() };
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        nid.uCallbackMessage = TRAY_MESSAGE;
        // The icon embedded by build.rs (resource 1); the stock application icon if it is missing.
        let instance = GetModuleHandleW(None).unwrap_or_default();
        #[allow(clippy::manual_dangling_ptr)] // resource id 1 as a pointer: the Windows MAKEINTRESOURCE idiom
        let resource = PCWSTR(1 as *const u16);
        nid.hIcon = LoadIconW(Some(instance.into()), resource).or_else(|_| LoadIconW(None, IDI_APPLICATION)).unwrap_or_default();
        for (i, c) in "Vials".encode_utf16().enumerate() {
            nid.szTip[i] = c;
        }
        if !Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
            eprintln!("Windows did not accept the tray icon");
        }
    }
}

fn remove_icon(hwnd: HWND) {
    // SAFETY: removes the icon added above.
    unsafe {
        let nid = NOTIFYICONDATAW { cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32, hWnd: hwnd, uID: 1, ..Default::default() };
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

fn show_menu(hwnd: HWND) {
    // SAFETY: a popup menu used and destroyed here; the window is brought forward first so that the
    // menu closes when the user clicks elsewhere (documented Shell_NotifyIcon behaviour).
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let _ = AppendMenuW(menu, MF_STRING, CMD_NEW, w!("New vial"));
        let _ = AppendMenuW(menu, MF_STRING, CMD_SETTINGS, w!("Settings"));
        let _ = AppendMenuW(menu, MF_STRING, CMD_QUIT, w!("Quit Vials"));
        let mut at = POINT::default();
        let _ = GetCursorPos(&mut at);
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, at.x, at.y, None, hwnd, None);
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        match cmd.0 as usize {
            CMD_NEW => send(Event::NewVial),
            CMD_SETTINGS => send(Event::Settings),
            CMD_QUIT => send(Event::Quit),
            _ => {}
        }
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        TRAY_MESSAGE => {
            match lparam.0 as u32 {
                WM_RBUTTONUP | WM_LBUTTONUP => show_menu(hwnd),
                _ => {}
            }
            LRESULT(0)
        }
        WM_COPYDATA => {
            // SAFETY: for WM_COPYDATA `lparam` points to the sender's COPYDATASTRUCT for this call.
            let data = unsafe { &*(lparam.0 as *const COPYDATASTRUCT) };
            if data.dwData == COPYDATA_ID && !data.lpData.is_null() {
                // SAFETY: the sender gave `cbData` readable bytes at `lpData`.
                let bytes = unsafe { std::slice::from_raw_parts(data.lpData as *const u8, data.cbData as usize) };
                send(Event::Action(String::from_utf8_lossy(bytes).into_owned()));
                return LRESULT(1);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            remove_icon(hwnd);
            // SAFETY: ends this thread's message loop.
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        m if TASKBAR_CREATED.get() == Some(&m) => {
            // Explorer restarted: its tray is empty again.
            add_icon(hwnd);
            LRESULT(0)
        }
        // SAFETY: default handling for everything else.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Starts the tray icon and the window that receives actions from second launches.
pub fn start(events: Sender<Event>) -> Result<Tray, String> {
    let _ = SINK.set(Mutex::new(events));
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<isize, String>>();
    std::thread::Builder::new()
        .name("vials-tray".into())
        .spawn(move || {
            // SAFETY: a window class and a hidden window used only on this thread.
            unsafe {
                let instance = GetModuleHandleW(None).unwrap_or_default();
                let class = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: instance.into(), lpszClassName: CLASS, ..Default::default() };
                RegisterClassW(&class);
                let _ = TASKBAR_CREATED.set(RegisterWindowMessageW(w!("TaskbarCreated")));
                let hwnd = match CreateWindowExW(WS_EX_TOOLWINDOW, CLASS, w!("Vials"), Default::default(), 0, 0, 0, 0, None, None, Some(instance.into()), None) {
                    Ok(h) => h,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.to_string()));
                        return;
                    }
                };
                add_icon(hwnd);
                let _ = ready_tx.send(Ok(hwnd.0 as isize));
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        })
        .map_err(|e| e.to_string())?;
    let hwnd = ready_rx.recv().map_err(|e| e.to_string())??;
    Ok(Tray { hwnd })
}
