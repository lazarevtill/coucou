//! User-initiated sharing of an already dropped file through the Windows share
//! sheet. Coucou never addresses or sends a message on the user's behalf.
use std::path::{Path, PathBuf};
use tauri::WebviewWindow;

fn checked_file(inbox: &Path, requested: &Path) -> Result<PathBuf, String> {
    let valid = || {
        let directory = inbox.canonicalize().ok()?;
        let file = requested.canonicalize().ok()?;
        (file.parent() == Some(directory.as_path()) && file.is_file()).then_some(file)
    };
    valid().ok_or_else(|| crate::i18n::t("Only files dropped on the island can be shared."))
}

#[tauri::command]
pub fn file_share_available() -> bool { cfg!(windows) }

#[tauri::command]
pub async fn share_file(window: WebviewWindow, path: String) -> Result<(), String> {
    if window.label() != crate::island::WINDOW_LABEL { return Err("Open the dropped file on the island to share it.".into()); }
    let path = checked_file(&crate::files::inbox_dir(), Path::new(&path))?;
    #[cfg(windows)]
    {
        let file = tauri::async_runtime::spawn_blocking(move || native::prepare(&path)).await.map_err(|e| e.to_string())??;
        let handle = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
        let (tx, rx) = tokio::sync::oneshot::channel();
        window.run_on_main_thread(move || { let _ = tx.send(native::show(handle, file).map_err(|e| e.to_string())); })
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    { let _ = path; Err("Native file sharing is available on Windows.".into()) }
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::cell::RefCell;
    use windows::{core::{factory, AgileReference, Interface, HSTRING}, ApplicationModel::DataTransfer::{DataRequestedEventArgs, DataTransferManager},
        Foundation::TypedEventHandler, Storage::{IStorageItem, StorageFile}, Win32::{Foundation::HWND,
        System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED}, UI::Shell::IDataTransferManagerInterop}};

    struct Registration { manager: DataTransferManager, token: i64 }
    impl Drop for Registration { fn drop(&mut self) { let _ = self.manager.RemoveDataRequested(self.token); } }
    thread_local! { static SHARE: RefCell<Option<Registration>> = const { RefCell::new(None) }; }

    pub fn prepare(path: &Path) -> Result<AgileReference<StorageFile>, String> {
        struct Com(bool);
        impl Drop for Com { fn drop(&mut self) { if self.0 { unsafe { CoUninitialize(); } } } }
        let _com = Com(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() });
        // WinRT expects a normal absolute path, not Rust's extended path prefix.
        let text = path.to_string_lossy();
        let name = text.strip_prefix(r"\\?\UNC\").map(|tail| format!(r"\\{tail}"))
            .unwrap_or_else(|| text.strip_prefix(r"\\?\").unwrap_or(&text).to_string());
        let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(name)).and_then(|op| op.get()).map_err(|e| e.to_string())?;
        AgileReference::new(&file).map_err(|e| e.to_string())
    }

    pub fn show(raw: isize, file: AgileReference<StorageFile>) -> windows::core::Result<()> {
        let interop: IDataTransferManagerInterop = factory::<DataTransferManager, IDataTransferManagerInterop>()?;
        let hwnd = HWND(raw as *mut _);
        let manager: DataTransferManager = unsafe { interop.GetForWindow(hwnd)? };
        let title = file.resolve()?.Name()?;
        let offered = std::time::Instant::now();
        let token = manager.DataRequested(&TypedEventHandler::<DataTransferManager, DataRequestedEventArgs>::new(move |_, args| {
            let Some(args) = args.as_ref() else { return Ok(()) };
            let request = args.Request()?;
            if offered.elapsed() > std::time::Duration::from_secs(120) {
                request.FailWithDisplayText(&HSTRING::from(crate::i18n::t("Retry")))?;
                return Ok(());
            }
            fill(&request.Data()?, &file, &title)
        }))?;
        SHARE.with(|state| *state.borrow_mut() = Some(Registration { manager, token }));
        if let Err(error) = unsafe { interop.ShowShareUIForWindow(hwnd) } {
            SHARE.with(|state| state.borrow_mut().take());
            return Err(error);
        }
        Ok(())
    }

    fn fill(package: &windows::ApplicationModel::DataTransfer::DataPackage, file: &AgileReference<StorageFile>, title: &HSTRING) -> windows::core::Result<()> {
        package.Properties()?.SetTitle(title)?;
        let items: windows_collections::IIterable<IStorageItem> = vec![Some(file.resolve()?.cast::<IStorageItem>()?)].into();
        package.SetStorageItems(&items, true)
    }

    #[test]
    fn a_native_share_package_contains_the_file_after_cross_thread_preparation() {
        use windows::ApplicationModel::DataTransfer::{DataPackage, StandardDataFormats};
        let directory = std::env::temp_dir().join(format!("coucou-share-native-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("attachment.txt");
        std::fs::write(&path, "public test fixture").unwrap();
        let prepared_path = path.clone();
        let file = std::thread::spawn(move || prepare(&prepared_path)).join().unwrap().unwrap();
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };
        {
            let package = DataPackage::new().unwrap();
            fill(&package, &file, &HSTRING::from("attachment.txt")).unwrap();
            let view = package.GetView().unwrap();
            assert!(view.Contains(&StandardDataFormats::StorageItems().unwrap()).unwrap());
            let items = view.GetStorageItemsAsync().unwrap().get().unwrap();
            assert_eq!(items.Size().unwrap(), 1);
            assert_eq!(items.GetAt(0).unwrap().Name().unwrap().to_string_lossy(), "attachment.txt");
        }
        drop(file);
        if initialized { unsafe { CoUninitialize(); } }
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sharing_rejects_files_outside_the_drop_inbox_and_nested_paths() {
        let base = std::env::temp_dir().join(format!("coucou-share-{}", std::process::id()));
        let inbox = base.join("inbox");
        std::fs::create_dir_all(inbox.join("nested")).unwrap();
        let shared = inbox.join("draft.txt");
        std::fs::write(&shared, "fixture").unwrap();
        std::fs::write(base.join("private.txt"), "private").unwrap();
        std::fs::write(inbox.join("nested/other.txt"), "nested").unwrap();
        assert_eq!(checked_file(&inbox, &shared).unwrap(), shared.canonicalize().unwrap());
        for file in [base.join("private.txt"), inbox.join("../private.txt"), inbox.join("nested/other.txt"), inbox.join("missing.txt"), inbox.clone()] {
            assert!(checked_file(&inbox, &file).is_err());
        }
        std::fs::remove_dir_all(base).unwrap();
    }
}
