//! Windows' own file and folder pickers, opened over the launcher window.

use std::path::PathBuf;
use tauri::Manager;

/// The launcher window, as the pickers' owner (0 when it can't be found).
pub fn owner(app: &tauri::AppHandle) -> isize {
    #[cfg(windows)]
    {
        app.get_webview_window("main")
            .and_then(|window| window.hwnd().ok())
            .map_or(0, |hwnd| hwnd.0 as isize)
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        0
    }
}

pub fn pick_folder(owner: isize, title: &str) -> Result<Option<PathBuf>, String> {
    pick(owner, title, None)
}

/// `filter` is a name and patterns, like ("Images", "*.png;*.jpg").
pub fn pick_file(owner: isize, title: &str, filter: (&str, &str)) -> Result<Option<PathBuf>, String> {
    pick(owner, title, Some(filter))
}

#[cfg(windows)]
fn pick(owner: isize, title: &str, filter: Option<(&str, &str)>) -> Result<Option<PathBuf>, String> {
    let (title, filter) = (title.to_owned(), filter.map(|(name, spec)| (name.to_owned(), spec.to_owned())));
    // COM dialogs need a single-threaded apartment of their own.
    std::thread::spawn(move || unsafe { show(owner, &title, filter) })
        .join()
        .map_err(|_| "La fenêtre de choix s'est fermée brutalement.".to_string())?
}

#[cfg(windows)]
unsafe fn show(owner: isize, title: &str, filter: Option<(String, String)>) -> Result<Option<PathBuf>, String> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
        COINIT_DISABLE_OLE1DDE,
    };
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH};

    /// HRESULT_FROM_WIN32(ERROR_CANCELLED)
    const CANCELLED: i32 = 0x8007_04C7_u32 as i32;
    let fail = |e: windows::core::Error| format!("Fenêtre de choix indisponible : {e}");

    let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).is_ok();
    let result = (|| {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).map_err(fail)?;
        let mut options = dialog.GetOptions().map_err(fail)? | FOS_FORCEFILESYSTEM;
        if filter.is_none() {
            options |= FOS_PICKFOLDERS;
        }
        dialog.SetOptions(options).map_err(fail)?;
        dialog.SetTitle(&HSTRING::from(title)).map_err(fail)?;
        if let Some((name, spec)) = &filter {
            let (name, spec) = (HSTRING::from(name.as_str()), HSTRING::from(spec.as_str()));
            let filters = [COMDLG_FILTERSPEC { pszName: PCWSTR(name.as_ptr()), pszSpec: PCWSTR(spec.as_ptr()) }];
            dialog.SetFileTypes(&filters).map_err(fail)?;
        }
        let parent = (owner != 0).then_some(HWND(owner as *mut _));
        match dialog.Show(parent) {
            Ok(()) => {}
            Err(e) if e.code().0 == CANCELLED => return Ok(None),
            Err(e) => return Err(fail(e)),
        }
        let item = dialog.GetResult().map_err(fail)?;
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).map_err(fail)?;
        let path = name.to_string();
        CoTaskMemFree(Some(name.0 as *const _));
        path.map(|path| Some(PathBuf::from(path))).map_err(|_| "Chemin illisible.".to_string())
    })();
    if initialized {
        CoUninitialize();
    }
    result
}

#[cfg(not(windows))]
fn pick(_owner: isize, _title: &str, _filter: Option<(&str, &str)>) -> Result<Option<PathBuf>, String> {
    Err("Disponible seulement sous Windows.".into())
}
