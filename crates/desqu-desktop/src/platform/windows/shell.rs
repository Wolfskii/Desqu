//! The desktop as Explorer exposes it: the folder view (listing, positions), the list-view control
//! (parking), icons and the folders on disk. Everything here must run on a COM STA thread.

use std::path::PathBuf;

use windows::core::{w, Interface, BOOL, GUID, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, IServiceProvider, CLSCTX_ALL};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_PublicDesktop, IFolderView2, IShellBrowser, IShellItem, IShellItemImageFactory,
    IShellWindows, SHCreateItemFromIDList, SHCreateItemFromParsingName, SHGetKnownFolderPath, ShellWindows,
    KF_FLAG_DEFAULT, SIGDN, SIGDN_DESKTOPABSOLUTEPARSING, SIGDN_NORMALDISPLAY, SIIGBF_ICONONLY, SVGIO_ALLVIEW,
    SWC_DESKTOP, SWFO_NEEDDISPATCH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, SendMessageTimeoutW, SMTO_NORMAL,
};

use crate::layer::{DesktopError, DesktopResult};
use crate::model::Point;

/// `LVM_SETITEMPOSITION`: x and y are packed as signed 16-bit values in `lParam`, so the message
/// carries no pointer and works across processes.
const LVM_SETITEMPOSITION: u32 = 0x1000 + 15;
const FWF_AUTOARRANGE: u32 = 0x1;
/// `SID_STopLevelBrowser`
const SID_TOP_LEVEL_BROWSER: GUID = GUID::from_u128(0x4c96be40_915c_11cf_99d3_00aa004ae837);

pub(super) fn native(op: &'static str, e: windows::core::Error) -> DesktopError {
    DesktopError::Native { op, message: e.to_string() }
}

fn view_error(e: windows::core::Error) -> DesktopError {
    DesktopError::ViewUnavailable(e.to_string())
}

pub(super) struct RawItem {
    pub index: i32,
    /// Absolute parsing name (file path or `::{CLSID}`).
    pub id: String,
    pub name: String,
    pub pos: Point,
}

/// The desktop's `IFolderView2`, found fresh each time so an Explorer restart is harmless.
pub(super) unsafe fn desktop_view() -> DesktopResult<IFolderView2> {
    let windows: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_ALL).map_err(view_error)?;
    let mut hwnd = 0i32;
    let dispatch = windows
        .FindWindowSW(&VARIANT::from(0i32), &VARIANT::default(), SWC_DESKTOP, &mut hwnd, SWFO_NEEDDISPATCH)
        .map_err(view_error)?;
    let provider: IServiceProvider = dispatch.cast().map_err(view_error)?;
    let browser: IShellBrowser = provider.QueryService(&SID_TOP_LEVEL_BROWSER).map_err(view_error)?;
    let view = browser.QueryActiveShellView().map_err(view_error)?;
    view.cast::<IFolderView2>().map_err(view_error)
}

pub(super) unsafe fn auto_arrange(view: &IFolderView2) -> DesktopResult<bool> {
    let flags = view.GetCurrentFolderFlags().map_err(|e| native("GetCurrentFolderFlags", e))?;
    Ok(flags & FWF_AUTOARRANGE != 0)
}

unsafe fn display_name(item: &IShellItem, kind: SIGDN) -> String {
    match item.GetDisplayName(kind) {
        Ok(name) => {
            let s = name.to_string().unwrap_or_default();
            CoTaskMemFree(Some(name.0 as *const _));
            s
        }
        Err(_) => String::new(),
    }
}

pub(super) unsafe fn list(view: &IFolderView2) -> DesktopResult<Vec<RawItem>> {
    let count = view.ItemCount(SVGIO_ALLVIEW).map_err(|e| native("ItemCount", e))?;
    let mut items = Vec::with_capacity(count.max(0) as usize);
    for index in 0..count {
        let pidl = view.Item(index).map_err(|e| native("Item", e))?;
        let item = SHCreateItemFromIDList::<IShellItem>(pidl as *const ITEMIDLIST);
        let pos = view.GetItemPosition(pidl).unwrap_or_default();
        CoTaskMemFree(Some(pidl as *const _));
        let Ok(item) = item else { continue };
        let id = display_name(&item, SIGDN_DESKTOPABSOLUTEPARSING);
        if id.is_empty() {
            continue;
        }
        items.push(RawItem {
            index,
            id,
            name: display_name(&item, SIGDN_NORMALDISPLAY),
            pos: Point { x: pos.x, y: pos.y },
        });
    }
    Ok(items)
}

/// The control that holds the icons. On some builds `SHELLDLL_DefView` lives in `Progman`, on
/// others in a `WorkerW`, so look at every top-level window.
pub(super) unsafe fn listview() -> DesktopResult<HWND> {
    struct Found(Option<HWND>);
    unsafe extern "system" fn visit(hwnd: HWND, l: LPARAM) -> BOOL {
        let found = &mut *(l.0 as *mut Found);
        if let Ok(defview) = FindWindowExW(Some(hwnd), None, w!("SHELLDLL_DefView"), PCWSTR::null()) {
            if let Ok(lv) = FindWindowExW(Some(defview), None, w!("SysListView32"), PCWSTR::null()) {
                found.0 = Some(lv);
                return BOOL(0);
            }
        }
        BOOL(1)
    }
    let mut found = Found(None);
    let _ = EnumWindows(Some(visit), LPARAM(&mut found as *mut Found as isize));
    found.0.ok_or_else(|| DesktopError::ViewUnavailable("the desktop icon list was not found".into()))
}

/// Moves one item (by its index in the view). Negative coordinates park it off-screen.
pub(super) unsafe fn set_position(lv: HWND, index: i32, p: Point) -> DesktopResult<()> {
    let pack = |v: i32| v.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16 as u16 as u32;
    let lparam = LPARAM(((pack(p.y) << 16) | pack(p.x)) as isize);
    let mut result = 0usize;
    let sent = SendMessageTimeoutW(lv, LVM_SETITEMPOSITION, WPARAM(index as usize), lparam, SMTO_NORMAL, 2000, Some(&mut result));
    if sent.0 == 0 {
        return Err(DesktopError::Native { op: "LVM_SETITEMPOSITION", message: "Explorer did not respond".into() });
    }
    if result == 0 {
        return Err(DesktopError::Native { op: "LVM_SETITEMPOSITION", message: format!("item {index} was refused") });
    }
    Ok(())
}

pub(super) fn desktop_folders() -> Vec<PathBuf> {
    let mut folders = Vec::new();
    for id in [&FOLDERID_Desktop, &FOLDERID_PublicDesktop] {
        // SAFETY: the returned string is freed right after it is copied.
        unsafe {
            if let Ok(p) = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) {
                if let Ok(s) = p.to_string() {
                    folders.push(PathBuf::from(s));
                }
                CoTaskMemFree(Some(p.0 as *const _));
            }
        }
    }
    folders
}

/// The item's icon as a PNG (`size` x `size`).
pub(super) unsafe fn icon_png(id: &str, size: u32) -> DesktopResult<Vec<u8>> {
    let wide: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
    let item: IShellItem =
        SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None).map_err(|_| DesktopError::NotFound(id.to_string()))?;
    let factory: IShellItemImageFactory = item.cast().map_err(|e| native("IShellItemImageFactory", e))?;
    let size = size.clamp(16, 256) as i32;
    let hbitmap = factory
        .GetImage(SIZE { cx: size, cy: size }, SIIGBF_ICONONLY)
        .map_err(|e| native("GetImage", e))?;

    let mut bitmap = BITMAP::default();
    GetObjectW(
        HGDIOBJ(hbitmap.0),
        std::mem::size_of::<BITMAP>() as i32,
        Some(&mut bitmap as *mut BITMAP as *mut _),
    );
    let (width, height) = (bitmap.bmWidth.max(1), bitmap.bmHeight.max(1));
    let mut header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    let hdc = CreateCompatibleDC(None);
    let lines = GetDIBits(hdc, hbitmap, 0, height as u32, Some(pixels.as_mut_ptr().cast()), &mut header, DIB_RGB_COLORS);
    let _ = DeleteDC(hdc);
    let _ = DeleteObject(HGDIOBJ(hbitmap.0));
    if lines == 0 {
        return Err(DesktopError::Native { op: "GetDIBits", message: "no pixels returned".into() });
    }
    for px in pixels.chunks_exact_mut(4) {
        px.swap(0, 2); // BGRA -> RGBA
    }
    encode_png(width as u32, height as u32, &pixels)
}

fn encode_png(width: u32, height: u32, rgba: &[u8]) -> DesktopResult<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| DesktopError::Native { op: "png", message: e.to_string() })?;
        writer
            .write_image_data(rgba)
            .map_err(|e| DesktopError::Native { op: "png", message: e.to_string() })?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_encoding_produces_a_valid_png() {
        let rgba = vec![255u8; 4 * 4 * 4];
        let png = encode_png(4, 4, &rgba).unwrap();
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    }
}
