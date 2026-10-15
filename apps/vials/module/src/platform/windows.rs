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

/// Puts a window back at the bottom of the z-order without activating it (used after a vial took the
/// keyboard focus, which would otherwise leave it above other windows).
pub(crate) fn send_to_bottom(hwnd: isize) {
    let hwnd = HWND(hwnd as *mut std::ffi::c_void);
    // SAFETY: `hwnd` is a window of this process, owned by Tauri, that is still alive.
    unsafe {
        let _ = SetWindowPos(hwnd, Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

/// Where the mouse pointer is, in screen pixels.
pub(crate) fn cursor_position() -> Option<(i32, i32)> {
    let mut at = POINT::default();
    // SAFETY: plain cursor query into a local.
    unsafe { GetCursorPos(&mut at) }.ok()?;
    Some((at.x, at.y))
}

/// Calls the handler with the files that were cut when Ctrl+V is pressed on the desktop.
pub(crate) fn watch_cut_paste(on_paste: Box<dyn Fn(Vec<String>) + Send>) -> Result<desqu_desktop::CutPasteWatcher, String> {
    desqu_desktop::watch_cut_paste(on_paste).map_err(|e| e.to_string())
}

// ------------------------------------------------------------------------- the desktop marquee

use std::cell::RefCell;
use std::sync::mpsc;
use std::thread::JoinHandle;

use ::windows::Win32::Foundation::{LPARAM, LRESULT, POINT, WPARAM};
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::System::Threading::GetCurrentThreadId;
use ::windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetAncestor, GetClassNameW, GetMessageW, GetParent, PostThreadMessageW, SetWindowsHookExW,
    UnhookWindowsHookEx, WindowFromPoint, MSG, MSLLHOOKSTRUCT, GA_ROOT, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_QUIT, WM_RBUTTONDOWN,
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

/// Whether the point is on the desktop itself (not on a normal window above it). With the desktop's
/// icons showing that is its icon view; with them switched off (Vials draws them) it is whatever
/// the desktop window (`Progman`, or the `WorkerW` that hosts the icons) shows there, such as a live
/// wallpaper.
fn on_desktop_view(x: i32, y: i32) -> bool {
    // SAFETY: plain window queries.
    unsafe {
        let hwnd = WindowFromPoint(POINT { x, y });
        if hwnd.0.is_null() {
            return false;
        }
        let root = GetAncestor(hwnd, GA_ROOT);
        !root.0.is_null() && matches!(class_of(root).as_str(), "Progman" | "WorkerW")
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
                // Over a desktop folder the drop goes to Explorer, which moves the file into it.
                if desqu_desktop::folder_at(desqu_desktop::Point { x: at.x, y: at.y }).ok().flatten().is_some() {
                    return DRAGDROP_S_DROP;
                }
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
    /// `moved`: the target took the files with a move (they left their place).
    Dropped { at: (i32, i32), accepted: bool, moved: bool },
}

/// Starts the standard Windows drag of one file or folder (its icon follows the cursor, targets
/// see a real shell item, Ctrl/Shift/Alt choose copy/move/link) and returns when it ends. Must run
/// on the thread that owns the window the drag started from, because the drag runs a modal loop.
pub(crate) fn drag_items(paths: &[String]) -> Result<DragEnd, String> {
    let Some(first) = paths.first() else { return Err("there is nothing to drag".into()) };
    // SAFETY: COM/OLE calls on the calling (UI) thread; all interface pointers stay alive across
    // the drag, and the bitmap handle is released afterwards.
    unsafe {
        let _ = OleInitialize(None);
        let item: IShellItem =
            SHCreateItemFromParsingName(&HSTRING::from(first.as_str()), None).map_err(|e| e.to_string())?;
        // One data object for all of them, so the target sees every file at once.
        let array = super::clipboard::items_array(paths)?;
        let data: IDataObject = array.BindToHandler(None, &BHID_DataObject).map_err(|e| e.to_string())?;

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
            Ok(DragEnd::Dropped { at: (at.x, at.y), accepted: effect.0 != 0, moved: effect.0 & DROPEFFECT_MOVE.0 != 0 })
        } else if result == DRAGDROP_S_CANCEL || result.is_ok() {
            Ok(DragEnd::Cancelled)
        } else {
            Err(format!("the drag failed: {result:?}"))
        }
    }
}


// --------------------------------------------------------------- the shell's right-click menu

use ::windows::Win32::Foundation::{GetLastError, ERROR_CLASS_ALREADY_EXISTS};
use ::windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_SHIFT};
use ::windows::Win32::UI::Shell::{
    IContextMenu, IContextMenu2, IContextMenu3, BHID_SFUIObject, CMF_EXTENDEDVERBS, CMF_NORMAL, CMIC_MASK_PTINVOKE,
    SEE_MASK_UNICODE, CMINVOKECOMMANDINFO, CMINVOKECOMMANDINFOEX,
};
use ::windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow, PostMessageW, RegisterClassW,
    SetForegroundWindow, TrackPopupMenuEx, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_DRAWITEM, WM_INITMENUPOPUP, WM_MEASUREITEM,
    WM_MENUCHAR, WM_NULL, WNDCLASSW, WS_POPUP, SW_SHOWNORMAL,
};

/// Command ids the shell may use in the popup menu.
const MENU_FIRST: u32 = 1;
const MENU_LAST: u32 = 0x7FFF;

thread_local! {
    /// The menu being shown, so its owner window can forward the messages that fill and draw
    /// submenus ("Send to", "Open with") and owner-drawn entries. The menu loop runs on this thread.
    static OPEN_MENU: RefCell<Option<(IContextMenu2, Option<IContextMenu3>)>> = const { RefCell::new(None) };
}

unsafe extern "system" fn menu_host_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if matches!(msg, WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR) {
        let handled = OPEN_MENU.with(|open| {
            let open = open.borrow();
            let (menu2, menu3) = open.as_ref()?;
            if let Some(menu3) = menu3 {
                let mut result = LRESULT(0);
                return menu3.HandleMenuMsg2(msg, wparam, lparam, Some(&mut result)).ok().map(|()| result);
            }
            if msg == WM_MENUCHAR {
                return None;
            }
            menu2.HandleMenuMsg(msg, wparam, lparam).ok().map(|()| LRESULT(if msg == WM_INITMENUPOPUP { 0 } else { 1 }))
        });
        if let Some(result) = handled {
            return result;
        }
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// Shows Explorer's own right-click menu for a file, folder or shortcut at the cursor and runs the
/// entry the user picks. It is the real shell menu (`IContextMenu`), so "Open", "Cut", "Copy",
/// "Delete", "Properties", "Send to" and the entries other programs add are all there.
///
/// Must run on the UI thread: the menu runs a modal loop. A vial window never takes focus, so the
/// menu is owned by a small hidden window that exists only while it is open.
pub(crate) fn show_item_menu(path: &str) -> Result<(), String> {
    // SAFETY: COM and window calls on the calling (UI) thread; every interface pointer and the
    // helper window stay alive until the menu is closed and the chosen command has run.
    unsafe {
        let item: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(path), None).map_err(|e| e.to_string())?;
        let menu: IContextMenu = item.BindToHandler(None, &BHID_SFUIObject).map_err(|e| e.to_string())?;

        let class = ::windows::core::w!("DesquVialsMenuHost");
        let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
        let class_info =
            WNDCLASSW { lpfnWndProc: Some(menu_host_proc), hInstance: instance.into(), lpszClassName: class, ..Default::default() };
        let registered = RegisterClassW(&class_info);
        if registered == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            return Err("could not register the menu window".into());
        }
        let host = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class,
            ::windows::core::w!(""),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .map_err(|e| e.to_string())?;

        let result = run_item_menu(host, &menu);
        let _ = DestroyWindow(host);
        result
    }
}

unsafe fn run_item_menu(host: HWND, menu: &IContextMenu) -> Result<(), String> {
    let popup = CreatePopupMenu().map_err(|e| e.to_string())?;
    // Shift held: the longer menu Explorer shows for Shift+right-click.
    let extended = if GetKeyState(i32::from(VK_SHIFT.0)) < 0 { CMF_EXTENDEDVERBS } else { 0 };
    let queried = menu.QueryContextMenu(popup, 0, MENU_FIRST, MENU_LAST, CMF_NORMAL | extended).ok();
    if let Err(e) = queried {
        let _ = DestroyMenu(popup);
        return Err(e.to_string());
    }

    OPEN_MENU.with(|open| {
        let menu3 = menu.cast::<IContextMenu3>().ok();
        let menu2 = menu.cast::<IContextMenu2>().ok();
        *open.borrow_mut() = menu2.map(|m| (m, menu3));
    });

    let mut at = POINT::default();
    let _ = GetCursorPos(&mut at);
    // Without being the foreground window the menu would not close when the user clicks elsewhere.
    let _ = SetForegroundWindow(host);
    let picked = TrackPopupMenuEx(popup, (TPM_RETURNCMD | TPM_RIGHTBUTTON).0, at.x, at.y, host, None).0;
    let _ = PostMessageW(Some(host), WM_NULL, WPARAM(0), LPARAM(0));
    OPEN_MENU.with(|open| *open.borrow_mut() = None);

    let result = if picked >= MENU_FIRST as i32 {
        let offset = (picked as u32 - MENU_FIRST) as usize;
        let info = CMINVOKECOMMANDINFOEX {
            cbSize: std::mem::size_of::<CMINVOKECOMMANDINFOEX>() as u32,
            fMask: SEE_MASK_UNICODE | CMIC_MASK_PTINVOKE,
            hwnd: host,
            // MAKEINTRESOURCE: a command offset in place of a verb string.
            lpVerb: ::windows::core::PCSTR(offset as *const u8),
            lpVerbW: PCWSTR(offset as *const u16),
            nShow: SW_SHOWNORMAL.0,
            ptInvoke: at,
            ..Default::default()
        };
        menu.InvokeCommand(&info as *const CMINVOKECOMMANDINFOEX as *const CMINVOKECOMMANDINFO).map_err(|e| e.to_string())
    } else {
        Ok(())
    };
    let _ = DestroyMenu(popup);
    result
}


// ------------------------------------------------------------------------------- glass look

use ::windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DWMWCP_ROUND};

/// Rounded corners for a borderless window (Windows 11 only rounds decorated windows on its own).
/// The blur behind the window is clipped to them. Failure (Windows 10) just leaves square corners.
pub(crate) fn round_corners(hwnd: isize, round: bool) {
    let hwnd = HWND(hwnd as *mut std::ffi::c_void);
    let preference = if round { DWMWCP_ROUND } else { DWMWCP_DONOTROUND };
    // SAFETY: `hwnd` is a window of this process; the attribute value is a plain i32 enum.
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &preference as *const _ as *const std::ffi::c_void,
            std::mem::size_of_val(&preference) as u32,
        );
    }
}

// ------------------------------------------------------------ "Show desktop icons" on or off

use ::windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use ::windows::Win32::UI::WindowsAndMessaging::{EVENT_OBJECT_HIDE, EVENT_OBJECT_SHOW, WINEVENT_OUTOFCONTEXT};

thread_local! {
    // The WinEvent callback runs on the thread that installed the hook.
    static ICONS: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
}

unsafe extern "system" fn icons_proc(_: HWINEVENTHOOK, _: u32, hwnd: HWND, id_object: i32, _: i32, _: u32, _: u32) {
    // Only the window itself (OBJID_WINDOW = 0), and only the desktop's icon view.
    if id_object != 0 || hwnd.0.is_null() || class_of(hwnd) != "SysListView32" {
        return;
    }
    // SAFETY: plain window query.
    let on_desktop = unsafe { GetParent(hwnd) }.is_ok_and(|p| class_of(p) == "SHELLDLL_DefView");
    if on_desktop {
        ICONS.with(|i| {
            if let Some(f) = i.borrow().as_ref() {
                f();
            }
        });
    }
}

/// Calls the callback when the desktop's icon view is shown or hidden, which is what the desktop's
/// "Show desktop icons" switch does. Event-driven (a WinEvent hook).
pub(crate) struct IconsWatcher {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl IconsWatcher {
    pub(crate) fn start(on_change: Box<dyn Fn() + Send>) -> Result<Self, String> {
        let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();
        let thread = std::thread::Builder::new()
            .name("vials-icons-watch".into())
            .spawn(move || {
                ICONS.with(|i| *i.borrow_mut() = Some(on_change));
                // SAFETY: the hook is removed below on this same thread, before it ends.
                unsafe {
                    let hook = SetWinEventHook(
                        EVENT_OBJECT_SHOW,
                        EVENT_OBJECT_HIDE,
                        None,
                        Some(icons_proc),
                        0,
                        0,
                        WINEVENT_OUTOFCONTEXT,
                    );
                    if hook.0.is_null() {
                        let _ = ready_tx.send(Err("could not watch the desktop icon view".into()));
                        return;
                    }
                    let _ = ready_tx.send(Ok(GetCurrentThreadId()));
                    let mut msg = MSG::default();
                    while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
                    let _ = UnhookWinEvent(hook);
                }
                ICONS.with(|i| *i.borrow_mut() = None);
            })
            .map_err(|e| e.to_string())?;
        let thread_id = ready_rx.recv().map_err(|e| e.to_string())??;
        Ok(Self { thread_id, thread: Some(thread) })
    }
}

impl Drop for IconsWatcher {
    fn drop(&mut self) {
        // SAFETY: posts WM_QUIT to the watcher thread so its message loop ends.
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
