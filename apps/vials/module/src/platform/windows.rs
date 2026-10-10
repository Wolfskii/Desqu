use ::windows::Win32::Foundation::HWND;
use ::windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, IsWindowVisible, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, HWND_BOTTOM,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE, WS_EX_APPWINDOW,
    WS_EX_TOOLWINDOW,
};

/// Makes a window a tool window: it stays out of the taskbar *and* Alt+Tab. Tauri's
/// `skip_taskbar` alone leaves it in Alt+Tab, and its windows carry `WS_EX_APPWINDOW`, which forces
/// a taskbar button even on a tool window, so that flag is cleared too. Call it after the window
/// has been shown: Tauri's own window layer rewrites the extended style while showing it.
///
/// The window is hidden and shown again (without activating it) so the shell re-evaluates it, and
/// put back at the bottom of the z-order so it stays above the desktop icons only.
pub(crate) fn mark_as_tool_window(hwnd: isize) -> Result<(), String> {
    let hwnd = HWND(hwnd as *mut std::ffi::c_void);
    // SAFETY: `hwnd` is a window of this process, owned by Tauri, that is still alive.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let wanted = (ex | WS_EX_TOOLWINDOW.0 as isize) & !(WS_EX_APPWINDOW.0 as isize);
        if wanted == ex {
            return Ok(());
        }
        let was_visible = IsWindowVisible(hwnd).as_bool();
        if was_visible {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, wanted);
        let framed = SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED);
        if was_visible {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        let _ = SetWindowPos(hwnd, Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        framed.map_err(|e| e.to_string())
    }
}

// ------------------------------------------------------------------------- the desktop marquee

use std::cell::RefCell;
use std::sync::mpsc;
use std::thread::JoinHandle;

use ::windows::Win32::Foundation::{LPARAM, LRESULT, POINT, WPARAM};
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::System::Threading::GetCurrentThreadId;
use ::windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetClassNameW, GetMessageW, GetParent, PostThreadMessageW, SetWindowsHookExW,
    UnhookWindowsHookEx, WindowFromPoint, MSG, MSLLHOOKSTRUCT, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_QUIT, WM_RBUTTONDOWN,
};

/// Mouse gestures on the desktop icon view.
pub(crate) enum MouseEvent {
    /// A drag from the press point to the release point.
    Marquee((i32, i32), (i32, i32)),
    /// The right button went down here (the desktop's context menu is about to open).
    ContextDown((i32, i32)),
    /// Any mouse button went down anywhere (used to close the popup when you click elsewhere).
    ButtonDown((i32, i32)),
}

pub(crate) type MouseHandler = Box<dyn Fn(MouseEvent) + Send>;

struct HookState {
    down: Option<(i32, i32)>,
    on_event: MouseHandler,
}

thread_local! {
    // The low-level hook procedure runs on the thread that installed it.
    static HOOK: RefCell<Option<HookState>> = const { RefCell::new(None) };
}

fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 64];
    // SAFETY: valid window handle and a local buffer.
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Whether the point is on the desktop's icon view (not on a normal window above it).
fn on_desktop_view(x: i32, y: i32) -> bool {
    // SAFETY: plain window queries.
    unsafe {
        let hwnd = WindowFromPoint(POINT { x, y });
        if hwnd.0.is_null() || class_of(hwnd) != "SysListView32" {
            return false;
        }
        GetParent(hwnd).is_ok_and(|p| class_of(p) == "SHELLDLL_DefView")
    }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        // SAFETY: for WH_MOUSE_LL `lparam` points to a MSLLHOOKSTRUCT valid for this call.
        let pt = unsafe { (*(lparam.0 as *const MSLLHOOKSTRUCT)).pt };
        match wparam.0 as u32 {
            WM_LBUTTONDOWN => {
                let on_desktop = on_desktop_view(pt.x, pt.y);
                HOOK.with(|h| {
                    if let Some(state) = h.borrow_mut().as_mut() {
                        state.down = on_desktop.then_some((pt.x, pt.y));
                        (state.on_event)(MouseEvent::ButtonDown((pt.x, pt.y)));
                    }
                });
            }
            WM_RBUTTONDOWN => {
                let on_desktop = on_desktop_view(pt.x, pt.y);
                HOOK.with(|h| {
                    if let Some(state) = h.borrow_mut().as_ref() {
                        (state.on_event)(MouseEvent::ButtonDown((pt.x, pt.y)));
                        if on_desktop {
                            (state.on_event)(MouseEvent::ContextDown((pt.x, pt.y)));
                        }
                    }
                });
            }
            WM_LBUTTONUP => HOOK.with(|h| {
                if let Some(state) = h.borrow_mut().as_mut() {
                    if let Some(down) = state.down.take() {
                        (state.on_event)(MouseEvent::Marquee(down, (pt.x, pt.y)));
                    }
                }
            }),
            _ => {}
        }
    }
    // SAFETY: always pass the event on.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Watches left-button presses and releases system-wide, only to see a drag that starts on the
/// desktop icon view. The callback runs inside the hook, so it must only hand the data over.
pub(crate) struct MouseWatcher {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl MouseWatcher {
    pub(crate) fn start(on_event: MouseHandler) -> Result<Self, String> {
        let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();
        let thread = std::thread::Builder::new()
            .name("vials-mouse-hook".into())
            .spawn(move || {
                HOOK.with(|h| *h.borrow_mut() = Some(HookState { down: None, on_event }));
                // SAFETY: the hook is removed below on this same thread, before it ends.
                unsafe {
                    let module = GetModuleHandleW(None).unwrap_or_default();
                    let hook = match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), Some(module.into()), 0) {
                        Ok(hook) => hook,
                        Err(e) => {
                            let _ = ready_tx.send(Err(e.to_string()));
                            return;
                        }
                    };
                    let _ = ready_tx.send(Ok(GetCurrentThreadId()));
                    let mut msg = MSG::default();
                    while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
                    let _ = UnhookWindowsHookEx(hook);
                }
                HOOK.with(|h| *h.borrow_mut() = None);
            })
            .map_err(|e| e.to_string())?;
        let thread_id = ready_rx.recv().map_err(|e| e.to_string())??;
        Ok(Self { thread_id, thread: Some(thread) })
    }
}

impl Drop for MouseWatcher {
    fn drop(&mut self) {
        // SAFETY: posts WM_QUIT to the hook thread so its message loop ends.
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

// --------------------------------------------------------------- the desktop's right-click menu

use ::windows::core::{HSTRING, PCWSTR};
use ::windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use ::windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_WRITE,
    REG_OPTION_NON_VOLATILE, REG_SZ,
};

/// `DesktopBackground` is the desktop only (not Explorer folder windows). Per user, so no admin.
const VERB_KEY: &str = r"Software\Classes\DesktopBackground\Shell\DesquVials";

fn set_string(key: HKEY, name: Option<&str>, value: &str) -> Result<(), String> {
    let name = name.map(HSTRING::from);
    let data: Vec<u8> = value.encode_utf16().chain(Some(0)).flat_map(u16::to_le_bytes).collect();
    // SAFETY: `key` is open for writing; the buffers outlive the call.
    let status = unsafe {
        RegSetValueExW(
            key,
            name.as_ref().map_or(PCWSTR::null(), |n| PCWSTR(n.as_ptr())),
            None,
            REG_SZ,
            Some(&data),
        )
    };
    status.ok().map_err(|e| e.to_string())
}

fn create_key(path: &str) -> Result<HKEY, String> {
    let path = HSTRING::from(path);
    let mut key = HKEY::default();
    // SAFETY: creates or opens a key under the current user's hive.
    unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(path.as_ptr()),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut key,
            None,
        )
    }
    .ok()
    .map_err(|e| e.to_string())?;
    Ok(key)
}

/// Adds (`Some(exe)`) or removes (`None`) a "Create vial here" entry in the desktop's right-click
/// menu. It runs `exe --action=create-vial`, which the running app turns into a vial at the spot
/// that was right-clicked. On Windows 11 the entry is under "Show more options".
pub(crate) fn set_context_menu(exe: Option<&str>) -> Result<(), String> {
    match exe {
        Some(exe) => {
            let key = create_key(VERB_KEY)?;
            let result = set_string(key, None, "Create vial here")
                .and_then(|()| set_string(key, Some("Icon"), &format!("\"{exe}\",0")));
            // SAFETY: closes the key opened above.
            unsafe {
                let _ = RegCloseKey(key);
            }
            result?;
            let command = create_key(&format!(r"{VERB_KEY}\command"))?;
            let result = set_string(command, None, &format!("\"{exe}\" --action=create-vial"));
            // SAFETY: closes the key opened above.
            unsafe {
                let _ = RegCloseKey(command);
            }
            result
        }
        None => {
            let path = HSTRING::from(VERB_KEY);
            // SAFETY: deletes only our own verb key and its `command` subkey.
            let status = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(path.as_ptr())) };
            if status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND {
                Ok(())
            } else {
                Err(format!("could not remove the menu entry: {status:?}"))
            }
        }
    }
}

// ------------------------------------------------------------------- dragging an item out (OLE)

use ::windows::core::{implement, BOOL, HRESULT};
use ::windows::Win32::Foundation::{S_OK, SIZE};
use ::windows::Win32::Graphics::Gdi::DeleteObject;
use ::windows::Win32::System::Com::{CoCreateInstance, IDataObject, CLSCTX_INPROC_SERVER};
use ::windows::Win32::System::Ole::{
    DoDragDrop, IDropSource, IDropSource_Impl, OleInitialize, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK,
    DROPEFFECT_MOVE,
};
use ::windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
use ::windows::Win32::UI::Shell::{
    IDragSourceHelper, IShellItem, IShellItemImageFactory, SHCreateItemFromParsingName,
    BHID_DataObject, SHDRAGIMAGE, SIIGBF_ICONONLY,
};
use ::windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
use ::windows::core::Interface;

/// CLSID_DragDropHelper: the shell's drag image helper.
const DRAG_DROP_HELPER: ::windows::core::GUID = ::windows::core::GUID::from_u128(0x4657278a_411b_11d2_839a_00c04fd918d0);

const DRAGDROP_S_DROP: HRESULT = HRESULT(0x0004_0100);
const DRAGDROP_S_CANCEL: HRESULT = HRESULT(0x0004_0101);
const DRAGDROP_S_USEDEFAULTCURSORS: HRESULT = HRESULT(0x0004_0102);

/// The mouse button and Esc rules of a native drag: let go to drop, Esc to cancel.
///
/// Letting go over the empty desktop is not passed on to Explorer: the file already lives in the
/// Desktop folder, so Explorer would answer "the destination folder is the same as the source
/// folder". The drag is cancelled instead and the position is recorded for the caller to place the
/// icon itself.
#[implement(IDropSource)]
struct DropSource {
    released_on_desktop: std::sync::Arc<std::sync::Mutex<Option<(i32, i32)>>>,
}

impl IDropSource_Impl for DropSource_Impl {
    fn QueryContinueDrag(&self, escape_pressed: BOOL, key_state: MODIFIERKEYS_FLAGS) -> HRESULT {
        if escape_pressed.as_bool() {
            DRAGDROP_S_CANCEL
        } else if key_state.0 & MK_LBUTTON.0 == 0 {
            let mut at = POINT::default();
            // SAFETY: plain cursor query into a local.
            let _ = unsafe { GetCursorPos(&mut at) };
            if on_desktop_view(at.x, at.y) {
                *self.released_on_desktop.lock().unwrap_or_else(|e| e.into_inner()) = Some((at.x, at.y));
                DRAGDROP_S_CANCEL
            } else {
                DRAGDROP_S_DROP
            }
        } else {
            S_OK
        }
    }

    fn GiveFeedback(&self, _effect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

/// How a drag ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DragEnd {
    Cancelled,
    /// Let go over the empty desktop (nothing was handed to Explorer): place the icon here.
    OnDesktop((i32, i32)),
    /// Dropped at this screen position and `moved_or_copied` tells whether a target accepted it.
    Dropped { at: (i32, i32), accepted: bool },
}

/// Starts the standard Windows drag of one file or folder (its icon follows the cursor, targets
/// see a real shell item, Ctrl/Shift/Alt choose copy/move/link) and returns when it ends. Must run
/// on the thread that owns the window the drag started from, because the drag runs a modal loop.
pub(crate) fn drag_item(path: &str) -> Result<DragEnd, String> {
    // SAFETY: COM/OLE calls on the calling (UI) thread; all interface pointers stay alive across
    // the drag, and the bitmap handle is released afterwards.
    unsafe {
        let _ = OleInitialize(None);
        let item: IShellItem =
            SHCreateItemFromParsingName(&HSTRING::from(path), None).map_err(|e| e.to_string())?;
        let data: IDataObject = item.BindToHandler(None, &BHID_DataObject).map_err(|e| e.to_string())?;

        // The ghost icon under the cursor, like Explorer shows. Optional: a drag without it still works.
        let helper = CoCreateInstance::<_, IDragSourceHelper>(&DRAG_DROP_HELPER, None, CLSCTX_INPROC_SERVER).ok();
        let mut bitmap = None;
        if let (Some(helper), Ok(factory)) = (&helper, item.cast::<IShellItemImageFactory>()) {
            if let Ok(hbmp) = factory.GetImage(SIZE { cx: 48, cy: 48 }, SIIGBF_ICONONLY) {
                let image = SHDRAGIMAGE {
                    sizeDragImage: SIZE { cx: 48, cy: 48 },
                    ptOffset: POINT { x: 24, y: 24 },
                    hbmpDragImage: hbmp,
                    // No colour key: the bitmap carries its own alpha.
                    crColorKey: ::windows::Win32::Foundation::COLORREF(0xFFFF_FFFF),
                };
                if helper.InitializeFromBitmap(&image, &data).is_ok() {
                    bitmap = Some(hbmp);
                }
            }
        }

        let released = std::sync::Arc::new(std::sync::Mutex::new(None));
        let source: IDropSource = DropSource { released_on_desktop: released.clone() }.into();
        let mut effect = DROPEFFECT(0);
        let result = DoDragDrop(&data, &source, DROPEFFECT_COPY | DROPEFFECT_MOVE | DROPEFFECT_LINK, &mut effect);
        let mut at = POINT::default();
        let _ = GetCursorPos(&mut at);
        if let Some(hbmp) = bitmap {
            // The drag helper released its copy when the drag ended; this handle is ours.
            let _ = DeleteObject(hbmp.into());
        }
        drop(helper);

        let on_desktop = released.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(at) = on_desktop {
            Ok(DragEnd::OnDesktop(at))
        } else if result == DRAGDROP_S_DROP {
            Ok(DragEnd::Dropped { at: (at.x, at.y), accepted: effect.0 != 0 })
        } else if result == DRAGDROP_S_CANCEL || result.is_ok() {
            Ok(DragEnd::Cancelled)
        } else {
            Err(format!("the drag failed: {result:?}"))
        }
    }
}

