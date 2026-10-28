//! Explicit clipboard-image attachments and the native Windows capture editor.
//! No background screen capture or clipboard monitoring.
use std::{io::Write, path::Path};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;
use tauri::WebviewWindow;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Limits { max_bytes: usize, max_pixels: u64, max_dimension: u32 }
fn limits() -> Limits { serde_json::from_str(include_str!("../../src/chat-image-limits.json")).expect("image limits") }

fn decode(data: &str) -> Result<Vec<u8>, String> {
    let limits = limits();
    if data.len() > limits.max_bytes.div_ceil(3) * 4 { return Err(crate::i18n::t("The image is too large. Capture a smaller area.")); }
    let bytes = STANDARD.decode(data).map_err(|_| crate::i18n::t("Paste a PNG image into the chat."))?;
    let png = bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.get(8..16) == Some(b"\0\0\0\rIHDR")
        && bytes.ends_with(b"\0\0\0\0IEND\xaeB`\x82");
    if !png || bytes.len() < 45 { return Err(crate::i18n::t("Paste a PNG image into the chat.")); }
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    if width == 0 || height == 0 || width > limits.max_dimension || height > limits.max_dimension
        || u64::from(width) * u64::from(height) > limits.max_pixels || bytes.len() > limits.max_bytes {
        return Err(crate::i18n::t("The image is too large. Capture a smaller area."));
    }
    Ok(bytes)
}

fn store(root: &Path, data: &str) -> Result<crate::files::DroppedFile, String> {
    let bytes = decode(data)?;
    crate::platform::ensure_private_dir(root).map_err(|_| "Could not prepare the attachment folder.")?;
    let name = format!("Capture {}.png", crate::flow::text::new_id());
    let path = root.join(&name);
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|_| "Could not save the image.")?;
    if file.write_all(&bytes).is_err() {
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err("Could not save the image.".into());
    }
    drop(file);
    crate::files::sweep(root);
    Ok(crate::files::DroppedFile { name, path: path.to_string_lossy().into(), size: bytes.len() as u64 })
}

fn from_chat(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == crate::island::WINDOW_LABEL { Ok(()) } else { Err("Open chat to attach an image.".into()) }
}

#[tauri::command]
pub async fn chat_image_attach(window: WebviewWindow, data: String) -> Result<crate::files::DroppedFile, String> {
    from_chat(&window)?;
    tauri::async_runtime::spawn_blocking(move || store(&crate::files::inbox_dir(), &data)).await.map_err(|_| "Could not save the image.".to_string())?
}

#[tauri::command]
pub async fn chat_capture_open(window: WebviewWindow) -> Result<(), String> {
    from_chat(&window)?;
    #[cfg(windows)]
    return tauri::async_runtime::spawn_blocking(native::open).await.map_err(|_| "Could not open Snipping Tool.".to_string())?;
    #[cfg(not(windows))]
    Err("Open your system screenshot tool, then paste its image into chat.".into())
}

#[cfg(windows)]
mod native {
    use windows::{core::HSTRING, ApplicationModel::Core::AppListEntry, Management::Deployment::PackageManager,
        Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED}};
    struct Runtime(bool);
    impl Drop for Runtime { fn drop(&mut self) { if self.0 { unsafe { RoUninitialize(); } } } }

    fn entry() -> Option<AppListEntry> {
        let manager = PackageManager::new().ok()?;
        for package in manager.FindPackagesByUserSecurityId(&HSTRING::new()).ok()? {
            let Ok(id) = package.Id() else { continue };
            if !id.Name().is_ok_and(|name| name.to_string_lossy() == "Microsoft.ScreenSketch")
                || !id.PublisherId().is_ok_and(|id| id.to_string_lossy() == "8wekyb3d8bbwe") { continue; }
            for entry in package.GetAppListEntriesAsync().ok()?.get().ok()? { return Some(entry); }
        }
        None
    }

    pub(super) fn open() -> Result<(), String> {
        let _runtime = Runtime(unsafe { RoInitialize(RO_INIT_MULTITHREADED).is_ok() });
        let app = entry().ok_or_else(|| crate::i18n::t("Install Windows Snipping Tool, or use Win+Shift+S and paste the capture here."))?;
        if app.LaunchAsync().and_then(|op| op.get()).unwrap_or(false) { Ok(()) }
        else { Err(crate::i18n::t("Could not open Snipping Tool. Use Win+Shift+S and paste the capture here.")) }
    }

    #[test]
    #[ignore = "Read-only check for the installed native Snipping Tool"]
    fn live_snipping_tool_is_available() {
        let _runtime = Runtime(unsafe { RoInitialize(RO_INIT_MULTITHREADED).is_ok() });
        assert!(entry().is_some());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clipboard_image_is_bounded_and_written_only_inside_the_inbox() {
        let bytes = include_bytes!("cli_agents/fixtures/blue.png");
        let data = STANDARD.encode(bytes);
        assert_eq!(decode(&data).unwrap(), bytes);
        for invalid in ["invalid".into(), STANDARD.encode(b"a script is not a PNG"), "A".repeat(limits().max_bytes * 2)] {
            assert!(decode(&invalid).is_err());
        }
        let mut oversized = bytes.to_vec();
        oversized[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(decode(&STANDARD.encode(oversized)).is_err());
        let root = std::env::temp_dir().join(format!("coucou-chat-image-{}", std::process::id()));
        let first = store(&root, &data).unwrap();
        let second = store(&root, &data).unwrap();
        assert_ne!(first.path, second.path);
        for file in [first, second] {
            assert_eq!(Path::new(&file.path).parent(), Some(root.as_path()));
            assert_eq!(std::fs::read(&file.path).unwrap(), bytes);
            std::fs::remove_file(file.path).unwrap();
        }
        std::fs::remove_dir(root).unwrap();
    }
}
