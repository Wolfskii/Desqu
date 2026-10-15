//! The shell clipboard for files: put files there (copy or cut), read them back, and bring them into
//! a folder with Explorer's own file operation (so progress, conflict prompts and undo are Explorer's).
//!
//! Everything here needs a COM/OLE apartment on the calling thread; each entry point initialises one.

use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex};

use ::windows::core::{implement, w, HRESULT, HSTRING, PCWSTR};
use ::windows::Win32::Foundation::HWND;
use ::windows::Win32::System::Com::{
    CoCreateInstance, CoTaskMemFree, IDataObject, CLSCTX_ALL, DVASPECT_CONTENT, FORMATETC, STGMEDIUM, STGMEDIUM_0,
    TYMED_HGLOBAL,
};
use ::windows::Win32::System::DataExchange::RegisterClipboardFormatW;
use ::windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use ::windows::Win32::System::Ole::{
    OleFlushClipboard, OleGetClipboard, OleInitialize, OleSetClipboard, ReleaseStgMedium, DROPEFFECT_COPY, DROPEFFECT_MOVE,
};
use ::windows::Win32::UI::Shell::Common::ITEMIDLIST;
use ::windows::Win32::UI::Shell::{
    BHID_DataObject, FileOperation, IFileOperation, IFileOperationProgressSink, IFileOperationProgressSink_Impl,
    IShellItem, IShellItemArray, SHCreateItemFromParsingName, SHCreateShellItemArrayFromDataObject,
    SHCreateShellItemArrayFromIDLists, SHParseDisplayName, FILEOPERATION_FLAGS, SIGDN_FILESYSPATH,
};

/// Files found on the clipboard.
pub(crate) struct ClipboardFiles {
    pub paths: Vec<String>,
    /// They were cut: pasting moves them.
    pub cut: bool,
}

/// `FOF_ALLOWUNDO`: Explorer's Undo (Ctrl+Z) can take the operation back.
const ALLOW_UNDO: u32 = 0x0040;
/// `FOF_RENAMEONCOLLISION`: a copy next to its original becomes "name - Copy".
const RENAME_ON_COLLISION: u32 = 0x0008;
/// `FOFX_ADDUNDORECORD`
const ADD_UNDO_RECORD: u32 = 0x2000_0000;

fn effect_format() -> u16 {
    // SAFETY: registers (or looks up) a clipboard format by name.
    unsafe { RegisterClipboardFormatW(w!("Preferred DropEffect")) as u16 }
}

fn format_etc(format: u16) -> FORMATETC {
    FORMATETC { cfFormat: format, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex: -1, tymed: TYMED_HGLOBAL.0 as u32 }
}

/// An item array for absolute paths (they may be in different folders).
pub(crate) unsafe fn items_array(paths: &[String]) -> Result<IShellItemArray, String> {
    let mut pidls: Vec<*mut ITEMIDLIST> = Vec::new();
    let mut failure = None;
    for path in paths {
        let mut pidl = std::ptr::null_mut();
        match SHParseDisplayName(&HSTRING::from(path.as_str()), None, &mut pidl, 0, None) {
            Ok(()) => pidls.push(pidl),
            Err(e) => {
                failure = Some(format!("{path}: {e}"));
                break;
            }
        }
    }
    let array = match failure {
        None => {
            let absolute: Vec<*const ITEMIDLIST> = pidls.iter().map(|p| *p as *const ITEMIDLIST).collect();
            SHCreateShellItemArrayFromIDLists(&absolute).map_err(|e| e.to_string())
        }
        Some(message) => Err(message),
    };
    for pidl in pidls {
        CoTaskMemFree(Some(pidl as *const _));
    }
    array
}

unsafe fn file_system_path(item: &IShellItem) -> Option<String> {
    let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
    let path = name.to_string().ok();
    CoTaskMemFree(Some(name.0 as *const _));
    path
}

/// Puts the files on the clipboard the way Explorer does for Ctrl+C (`cut` false) or Ctrl+X: real
/// files, so any program can paste them, and Explorer moves them when they were cut.
pub(crate) fn put_files(paths: &[String], cut: bool) -> Result<(), String> {
    // SAFETY: COM calls on the calling thread; every pointer outlives the call that uses it.
    unsafe {
        let _ = OleInitialize(None);
        let array = items_array(paths)?;
        let data: IDataObject = array.BindToHandler(None, &BHID_DataObject).map_err(|e| e.to_string())?;

        let hglobal = GlobalAlloc(GMEM_MOVEABLE, std::mem::size_of::<u32>()).map_err(|e| e.to_string())?;
        let effect = GlobalLock(hglobal) as *mut u32;
        if effect.is_null() {
            return Err("could not allocate the drop effect".into());
        }
        *effect = if cut { DROPEFFECT_MOVE.0 } else { DROPEFFECT_COPY.0 };
        let _ = GlobalUnlock(hglobal);
        let medium = STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            u: STGMEDIUM_0 { hGlobal: hglobal },
            pUnkForRelease: ManuallyDrop::new(None),
        };
        data.SetData(&format_etc(effect_format()), &medium, true).map_err(|e| e.to_string())?;

        OleSetClipboard(&data).map_err(|e| e.to_string())?;
        // Render it now: the clipboard keeps the files after this process has gone.
        OleFlushClipboard().map_err(|e| e.to_string())
    }
}

/// The files on the clipboard, or `None` when there are none.
pub(crate) fn files() -> Result<Option<ClipboardFiles>, String> {
    // SAFETY: COM calls on the calling thread; the storage medium is released after it is read.
    unsafe {
        let _ = OleInitialize(None);
        let Ok(data) = OleGetClipboard() else { return Ok(None) };
        let Ok(array) = SHCreateShellItemArrayFromDataObject::<_, IShellItemArray>(&data) else { return Ok(None) };
        let count = array.GetCount().map_err(|e| e.to_string())?;
        let mut paths = Vec::new();
        for i in 0..count {
            let item = array.GetItemAt(i).map_err(|e| e.to_string())?;
            paths.extend(file_system_path(&item));
        }
        if paths.is_empty() {
            return Ok(None);
        }
        let mut cut = false;
        if let Ok(mut medium) = data.GetData(&format_etc(effect_format())) {
            let hglobal = medium.u.hGlobal;
            let effect = GlobalLock(hglobal) as *const u32;
            if !effect.is_null() {
                cut = *effect & DROPEFFECT_MOVE.0 != 0;
                let _ = GlobalUnlock(hglobal);
            }
            ReleaseStgMedium(&mut medium);
        }
        Ok(Some(ClipboardFiles { paths, cut }))
    }
}

/// Empties the clipboard (what Explorer does after a cut has been pasted).
pub(crate) fn clear() {
    // SAFETY: plain OLE call on the calling thread.
    unsafe {
        let _ = OleInitialize(None);
        let _ = OleSetClipboard(None::<&IDataObject>);
    }
}

/// Collects what the file operation made, so the new files can be put in a vial.
#[implement(IFileOperationProgressSink)]
struct Collector {
    created: Arc<Mutex<Vec<String>>>,
}

impl Collector {
    fn note(&self, done: HRESULT, item: Option<&IShellItem>) {
        if done.is_ok() {
            // SAFETY: `item` is a live shell item handed over by the operation.
            if let Some(path) = item.and_then(|i| unsafe { file_system_path(i) }) {
                self.created.lock().unwrap_or_else(|e| e.into_inner()).push(path);
            }
        }
    }
}

#[allow(non_snake_case)]
impl IFileOperationProgressSink_Impl for Collector_Impl {
    fn StartOperations(&self) -> ::windows::core::Result<()> {
        Ok(())
    }
    fn FinishOperations(&self, _hrresult: HRESULT) -> ::windows::core::Result<()> {
        Ok(())
    }
    fn PreRenameItem(&self, _flags: u32, _item: ::windows::core::Ref<'_, IShellItem>, _new_name: &PCWSTR) -> ::windows::core::Result<()> {
        Ok(())
    }
    fn PostRenameItem(
        &self,
        _flags: u32,
        _item: ::windows::core::Ref<'_, IShellItem>,
        _new_name: &PCWSTR,
        _result: HRESULT,
        _created: ::windows::core::Ref<'_, IShellItem>,
    ) -> ::windows::core::Result<()> {
        Ok(())
    }
    fn PreMoveItem(
        &self,
        _flags: u32,
        _item: ::windows::core::Ref<'_, IShellItem>,
        _destination: ::windows::core::Ref<'_, IShellItem>,
        _new_name: &PCWSTR,
    ) -> ::windows::core::Result<()> {
        Ok(())
    }
    fn PostMoveItem(
        &self,
        _flags: u32,
        _item: ::windows::core::Ref<'_, IShellItem>,
        _destination: ::windows::core::Ref<'_, IShellItem>,
        _new_name: &PCWSTR,
        result: HRESULT,
        created: ::windows::core::Ref<'_, IShellItem>,
    ) -> ::windows::core::Result<()> {
        self.note(result, created.as_ref());
        Ok(())
    }
    fn PreCopyItem(
        &self,
        _flags: u32,
        _item: ::windows::core::Ref<'_, IShellItem>,
        _destination: ::windows::core::Ref<'_, IShellItem>,
        _new_name: &PCWSTR,
    ) -> ::windows::core::Result<()> {
        Ok(())
    }
    fn PostCopyItem(
        &self,
        _flags: u32,
        _item: ::windows::core::Ref<'_, IShellItem>,
        _destination: ::windows::core::Ref<'_, IShellItem>,
        _new_name: &PCWSTR,
        result: HRESULT,
        created: ::windows::core::Ref<'_, IShellItem>,
    ) -> ::windows::core::Result<()> {
        self.note(result, created.as_ref());
        Ok(())
    }
    fn PreDeleteItem(&self, _flags: u32, _item: ::windows::core::Ref<'_, IShellItem>) -> ::windows::core::Result<()> {
        Ok(())
    }
    fn PostDeleteItem(
        &self,
        _flags: u32,
        _item: ::windows::core::Ref<'_, IShellItem>,
        _result: HRESULT,
        _created: ::windows::core::Ref<'_, IShellItem>,
    ) -> ::windows::core::Result<()> {
        Ok(())
    }
    fn PreNewItem(
        &self,
        _flags: u32,
        _destination: ::windows::core::Ref<'_, IShellItem>,
        _new_name: &PCWSTR,
    ) -> ::windows::core::Result<()> {
        Ok(())
    }
    fn PostNewItem(
        &self,
        _flags: u32,
        _destination: ::windows::core::Ref<'_, IShellItem>,
        _new_name: &PCWSTR,
        _template: &PCWSTR,
        _attributes: u32,
        _result: HRESULT,
        _created: ::windows::core::Ref<'_, IShellItem>,
    ) -> ::windows::core::Result<()> {
        Ok(())
    }
    fn UpdateProgress(&self, _total: u32, _so_far: u32) -> ::windows::core::Result<()> {
        Ok(())
    }
    fn ResetTimer(&self) -> ::windows::core::Result<()> {
        Ok(())
    }
    fn PauseTimer(&self) -> ::windows::core::Result<()> {
        Ok(())
    }
    fn ResumeTimer(&self) -> ::windows::core::Result<()> {
        Ok(())
    }
}

/// Moves (`cut`) or copies the files into `folder` with Explorer's file operation and returns the
/// paths it made. With `rename_on_collision` a copy beside its original is named "... - Copy" instead
/// of asking. The operation shows Explorer's own progress and conflict dialogs and can be undone with
/// Ctrl+Z in Explorer. Runs until it is finished, so call it off the UI thread.
pub(crate) fn transfer(paths: &[String], cut: bool, folder: &str, rename_on_collision: bool) -> Result<Vec<String>, String> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    // SAFETY: COM calls on the calling thread; the sink and the operation outlive `PerformOperations`.
    unsafe {
        let _ = OleInitialize(None);
        let items = items_array(paths)?;
        let destination: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(folder), None).map_err(|e| e.to_string())?;
        let operation: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(|e| e.to_string())?;
        let mut flags = ALLOW_UNDO | ADD_UNDO_RECORD;
        if rename_on_collision {
            flags |= RENAME_ON_COLLISION;
        }
        operation.SetOperationFlags(FILEOPERATION_FLAGS(flags)).map_err(|e| e.to_string())?;
        operation.SetOwnerWindow(HWND::default()).ok();

        let created = Arc::new(Mutex::new(Vec::new()));
        let sink: IFileOperationProgressSink = Collector { created: created.clone() }.into();
        let cookie = operation.Advise(&sink).map_err(|e| e.to_string())?;
        let queued = if cut { operation.MoveItems(&items, &destination) } else { operation.CopyItems(&items, &destination) };
        let performed = queued.and_then(|()| operation.PerformOperations());
        let _ = operation.Unadvise(cookie);
        performed.map_err(|e| e.to_string())?;

        let made = created.lock().unwrap_or_else(|e| e.into_inner()).clone();
        Ok(made)
    }
}
