//! Things to do with the files of a vial that need Windows: text on the clipboard, a folder picker and
//! a zip file. Each entry point sets up what it needs on the calling thread; the picker and the zip
//! block until they are done, so call them off the UI thread.

use std::path::{Path, PathBuf};

use ::windows::core::PWSTR;
use ::windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use ::windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED};
use ::windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
use ::windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use ::windows::Win32::UI::Shell::{FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH};

/// `CF_UNICODETEXT`
const CF_UNICODETEXT: u32 = 13;

/// Puts plain text on the clipboard.
pub fn put_text(text: &str) -> Result<(), String> {
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let bytes = wide.len() * 2;
    // SAFETY: the clipboard is opened and closed here; the global memory is either handed to the clipboard
    // (which then owns it) or freed.
    unsafe {
        OpenClipboard(None).map_err(|e| e.to_string())?;
        let result = (|| {
            EmptyClipboard().map_err(|e| e.to_string())?;
            let memory: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, bytes).map_err(|e| e.to_string())?;
            let target = GlobalLock(memory) as *mut u16;
            if target.is_null() {
                let _ = GlobalFree(Some(memory));
                return Err("could not lock the clipboard memory".to_string());
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), target, wide.len());
            let _ = GlobalUnlock(memory);
            match SetClipboardData(CF_UNICODETEXT, Some(HANDLE(memory.0))) {
                Ok(_) => Ok(()),
                Err(e) => {
                    let _ = GlobalFree(Some(memory));
                    Err(e.to_string())
                }
            }
        })();
        let _ = CloseClipboard();
        result
    }
}

/// Lets the user choose a folder; `None` when the dialog was cancelled.
pub fn pick_folder(title: &str) -> Result<Option<PathBuf>, String> {
    // SAFETY: COM on this thread; the dialog and the string it returns are released here.
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let result = (|| {
            let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).map_err(|e| e.to_string())?;
            let options = dialog.GetOptions().map_err(|e| e.to_string())?;
            dialog.SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM).map_err(|e| e.to_string())?;
            let _ = dialog.SetTitle(&::windows::core::HSTRING::from(title));
            // Cancelled is an error code, not a failure.
            if dialog.Show(Some(HWND::default())).is_err() {
                return Ok(None);
            }
            let item = dialog.GetResult().map_err(|e| e.to_string())?;
            let name: PWSTR = item.GetDisplayName(SIGDN_FILESYSPATH).map_err(|e| e.to_string())?;
            let path = name.to_string().map_err(|e| e.to_string());
            CoTaskMemFree(Some(name.0 as *const _));
            path.map(|p| Some(PathBuf::from(p)))
        })();
        if initialized {
            CoUninitialize();
        }
        result
    }
}

/// Packs `items` (files and folders, by absolute path) into the zip file `to`.
pub fn zip(items: &[String], to: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    if items.is_empty() {
        return Err("there is nothing to pack".into());
    }
    // The paths go in through the environment, so no quoting of odd file names can go wrong.
    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", "Compress-Archive -LiteralPath ($env:DESQU_ZIP_ITEMS -split '\\|') -DestinationPath $env:DESQU_ZIP_TO -ErrorAction Stop"])
        .env("DESQU_ZIP_ITEMS", items.join("|"))
        .env("DESQU_ZIP_TO", to)
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zip_holds_the_files_it_was_given() {
        let dir = std::env::temp_dir().join(format!("desqu-zip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b) = (dir.join("a.txt"), dir.join("b with space.txt"));
        std::fs::write(&a, "a").unwrap();
        std::fs::write(&b, "b").unwrap();
        let out = dir.join("out.zip");
        let result = zip(&[a.to_string_lossy().into_owned(), b.to_string_lossy().into_owned()], &out);
        let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
        let _ = std::fs::remove_dir_all(&dir);
        result.expect("Compress-Archive works");
        assert!(size > 100, "the zip has content: {size} bytes");
    }

    #[test]
    fn nothing_to_pack_is_refused() {
        assert!(zip(&[], Path::new("x.zip")).is_err());
    }
}
