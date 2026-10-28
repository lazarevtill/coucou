//! Visual attachments shared by both official CLI providers. Files are read
//! only for an explicit chat attachment. PDFs are rendered locally on Windows.
use std::{io::Read, path::{Path, PathBuf}, sync::atomic::{AtomicBool, Ordering}};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Limits {
    image_bytes: usize,
    total_image_bytes: usize,
    pdf_bytes: usize,
    pages: usize,
    #[cfg(windows)]
    page_long_edge: u32,
    #[cfg(windows)]
    render_seconds: u64,
}

fn limits() -> &'static Limits {
    static LIMITS: std::sync::OnceLock<Limits> = std::sync::OnceLock::new();
    LIMITS.get_or_init(|| serde_json::from_str(include_str!("attachment-limits.json")).expect("attachment limits"))
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Image { media_type: String, data: String }

impl Image {
    fn new(media_type: &str, bytes: &[u8]) -> Result<Self, String> {
        if bytes.is_empty() || bytes.len() > limits().image_bytes { return Err("An image exceeds the CLI attachment size limit.".into()); }
        Ok(Self { media_type: media_type.into(), data: STANDARD.encode(bytes) })
    }
    pub(super) fn block(&self) -> Value {
        serde_json::json!({"type":"image","source":{"type":"base64","media_type":self.media_type,"data":self.data}})
    }
}

fn image_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") { Some("image/png") }
    else if bytes.starts_with(&[0xff, 0xd8, 0xff]) { Some("image/jpeg") }
    else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") { Some("image/gif") }
    else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") { Some("image/webp") }
    else { None }
}

pub(super) fn read(path: &Path, cancel: &AtomicBool) -> Result<Option<Vec<Image>>, String> {
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
    let pdf = extension == "pdf";
    if !pdf && !matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp") { return Ok(None); }
    let limit = if pdf { limits().pdf_bytes } else { limits().image_bytes };
    let mut bytes = Vec::new();
    let file = std::fs::File::open(path).map_err(|_| "Could not read the attachment.")?;
    if !file.metadata().is_ok_and(|m| m.is_file()) { return Err("Choose a regular file attachment.".into()); }
    file.take(limit as u64 + 1).read_to_end(&mut bytes).map_err(|_| "Could not read the attachment.")?;
    if bytes.len() > limit { return Err("The attachment exceeds the CLI file size limit.".into()); }
    if cancel.load(Ordering::Acquire) { return Err("Reply stopped.".into()); }
    if pdf {
        if !bytes.starts_with(b"%PDF-") { return Err("The attachment is not a PDF document.".into()); }
        #[cfg(windows)]
        { native_pdf::render(&bytes, cancel).map(Some) }
        #[cfg(not(windows))]
        { Err("PDF attachments for CLI chat require Windows. Attach page images on this platform.".into()) }
    } else {
        let kind = image_type(&bytes).ok_or("The file is not a supported PNG, JPEG, GIF or WebP image.")?;
        Ok(Some(vec![Image::new(kind, &bytes)?]))
    }
}

pub(super) fn conversation(history: &[Value], current: &[Image]) -> Result<Vec<Image>, String> {
    let mut images = Vec::new();
    for turn in history {
        if let Some(stored) = turn.get("images") {
            images.extend(serde_json::from_value::<Vec<Image>>(stored.clone()).map_err(|_| "Could not read attachment history.")?);
        }
    }
    images.extend_from_slice(current);
    let total: usize = images.iter().map(|image| image.data.len()).sum();
    if images.len() > limits().pages || total > limits().total_image_bytes.div_ceil(3) * 4 {
        return Err("The conversation has too many image pages. Start a new chat with a smaller attachment.".into());
    }
    Ok(images)
}

/// Codex's official image option takes local files. Each request owns a private
/// random directory, removed on completion, cancellation, or error.
pub(super) struct Files { directory: PathBuf, paths: Vec<PathBuf> }
impl Files {
    pub(super) fn create(images: &[Image], root: &Path) -> Result<Self, String> {
        use ring::rand::{SecureRandom, SystemRandom};
        crate::platform::ensure_private_dir(root).map_err(|_| "Could not prepare attachments.")?;
        let mut nonce = [0u8; 16];
        SystemRandom::new().fill(&mut nonce).map_err(|_| "Could not prepare attachments.")?;
        let directory = root.join(nonce.iter().map(|b| format!("{b:02x}")).collect::<String>());
        std::fs::create_dir(&directory).map_err(|_| "Could not prepare attachments.")?;
        let mut files = Self { directory, paths: Vec::new() };
        for (index, image) in images.iter().enumerate() {
            let extension = match image.media_type.as_str() {
                "image/png" => "png", "image/jpeg" => "jpg", "image/gif" => "gif", "image/webp" => "webp",
                _ => return Err("Unsupported image attachment.".into()),
            };
            let bytes = STANDARD.decode(&image.data).map_err(|_| "Could not read image attachment.")?;
            if bytes.len() > limits().image_bytes { return Err("The image is too large.".into()); }
            let path = files.directory.join(format!("page-{}.{extension}", index + 1));
            // Remember the path before writing so a failed partial write is removed.
            files.paths.push(path.clone());
            std::fs::write(path, bytes).map_err(|_| "Could not prepare image attachment.")?;
        }
        Ok(files)
    }
    pub(super) fn add_args(&self, command: &mut std::process::Command) {
        for path in &self.paths {
            let mut arg = std::ffi::OsString::from("--image=");
            arg.push(path);
            command.arg(arg);
        }
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        for path in &self.paths { let _ = std::fs::remove_file(path); }
        let _ = std::fs::remove_dir(&self.directory);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_history_survives_followups_but_not_provider_changes() {
        let image = Image::new("image/png", include_bytes!("fixtures/blue.png")).unwrap();
        let chat = crate::chat::Chat::default();
        let turn = chat.begin("claude-code");
        chat.commit(&turn, serde_json::json!({"role":"user","content":"Read this", "images":[image]}),
            serde_json::json!({"role":"assistant","content":"Blue"}), "File: picture.png", "Blue");
        let same = conversation(&chat.begin("claude-code").history, &[]).unwrap();
        assert_eq!(same.len(), 1);
        assert_eq!(STANDARD.decode(&same[0].data).unwrap(), include_bytes!("fixtures/blue.png"));
        assert!(conversation(&chat.begin("codex").history, &[]).unwrap().is_empty());
        chat.reset();
        assert!(conversation(&chat.begin("claude-code").history, &[]).unwrap().is_empty());
    }

    #[test]
    fn temporary_images_are_private_request_files_and_removed_after_errors() {
        let root = std::env::temp_dir().join(format!("coucou-visual-cleanup-{}", std::process::id()));
        let image = Image::new("image/png", include_bytes!("fixtures/blue.png")).unwrap();
        let directory;
        {
            let files = Files::create(&[image.clone()], &root).unwrap();
            directory = files.directory.clone();
            assert_eq!(std::fs::read(&files.paths[0]).unwrap(), include_bytes!("fixtures/blue.png"));
            let mut command = std::process::Command::new("codex");
            files.add_args(&mut command);
            assert!(command.get_args().next().unwrap().to_string_lossy().starts_with("--image="));
        }
        assert!(!directory.exists());
        let bad = Image { media_type: "unsupported".into(), data: image.data.clone() };
        assert!(Files::create(&[image, bad], &root).is_err());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn attachments_are_bounded_and_declared_image_types_require_a_real_header() {
        assert!(Image::new("image/png", &vec![0; limits().image_bytes + 1]).is_err());
        let image = Image::new("image/png", include_bytes!("fixtures/blue.png")).unwrap();
        assert!(conversation(&[], &vec![image; limits().pages + 1]).is_err());
        assert!(image_type(b"arbitrary text named .png").is_none());
        assert_eq!(image_type(include_bytes!("fixtures/blue.png")), Some("image/png"));
    }

    #[cfg(windows)]
    #[test]
    fn pdf_renders_every_page_and_honors_cancellation_and_invalid_documents() {
        let pages = native_pdf::render(include_bytes!("fixtures/projects.pdf"), &AtomicBool::new(false)).unwrap();
        assert_eq!(pages.len(), 2);
        for page in pages { assert_eq!(image_type(&STANDARD.decode(page.data).unwrap()), Some("image/png")); }
        assert!(native_pdf::render(b"%PDF-invalid", &AtomicBool::new(false)).is_err());
        assert_eq!(native_pdf::render(include_bytes!("fixtures/projects.pdf"), &AtomicBool::new(true)).err().as_deref(), Some("Reply stopped."));
    }
}

#[cfg(windows)]
mod native_pdf {
    use super::*;
    use std::time::{Duration, Instant};
    use windows::{Data::Pdf::{PdfDocument, PdfPageRenderOptions}, Storage::Streams::{DataReader, DataWriter, InMemoryRandomAccessStream},
        Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED}};

    pub(super) fn render(bytes: &[u8], cancel: &AtomicBool) -> Result<Vec<Image>, String> {
        struct Runtime(bool);
        impl Drop for Runtime { fn drop(&mut self) { if self.0 { unsafe { RoUninitialize(); } } } }
        let _runtime = Runtime(unsafe { RoInitialize(RO_INIT_MULTITHREADED).is_ok() });
        let deadline = Instant::now() + Duration::from_secs(limits().render_seconds);
        let check = || {
            if cancel.load(Ordering::Acquire) { Err("Reply stopped.".to_string()) }
            else if Instant::now() >= deadline { Err("The PDF took too long to render. Try a smaller document.".into()) }
            else { Ok(()) }
        };
        let native_error = |_| "Could not render the PDF. Check that it opens without a password.".to_string();
        // Poll WinRT operations so Stop and the rendering deadline apply while
        // opening and rendering, including malformed or unusually complex PDFs.
        macro_rules! wait {
            ($operation:expr) => {{
                let operation = $operation.map_err(native_error)?;
                loop {
                    if let Err(error) = check() { let _ = operation.Cancel(); return Err(error); }
                    if operation.Status().map_err(native_error)?.0 != 0 { break; }
                    std::thread::sleep(Duration::from_millis(10));
                }
                operation.GetResults().map_err(native_error)?
            }};
        }
        let source = InMemoryRandomAccessStream::new().map_err(native_error)?;
        let writer = DataWriter::CreateDataWriter(&source).map_err(native_error)?;
        writer.WriteBytes(bytes).map_err(native_error)?;
        wait!(writer.StoreAsync());
        writer.DetachStream().map_err(native_error)?;
        source.Seek(0).map_err(native_error)?;
        let document = wait!(PdfDocument::LoadFromStreamAsync(&source));
        let count = document.PageCount().map_err(native_error)? as usize;
        if count == 0 || count > limits().pages {
            return Err(format!("CLI chat accepts PDFs with 1–{} pages. Split this document before attaching it.", limits().pages));
        }
        let mut images = Vec::with_capacity(count);
        let mut total = 0;
        for index in 0..count {
            check()?;
            let page = document.GetPage(index as u32).map_err(native_error)?;
            let size = page.Size().map_err(native_error)?;
            if !size.Width.is_finite() || !size.Height.is_finite() || size.Width <= 0.0 || size.Height <= 0.0 {
                return Err("The PDF has an invalid page size.".into());
            }
            let options = PdfPageRenderOptions::new().map_err(native_error)?;
            if size.Width >= size.Height { options.SetDestinationWidth(limits().page_long_edge) }
            else { options.SetDestinationHeight(limits().page_long_edge) }.map_err(native_error)?;
            let output = InMemoryRandomAccessStream::new().map_err(native_error)?;
            wait!(page.RenderWithOptionsToStreamAsync(&output, &options));
            page.Close().map_err(native_error)?;
            let size = output.Size().map_err(native_error)? as usize;
            total += size;
            if size > limits().image_bytes || total > limits().total_image_bytes { return Err("The rendered PDF exceeds the CLI attachment size limit.".into()); }
            output.Seek(0).map_err(native_error)?;
            let reader = DataReader::CreateDataReader(&output).map_err(native_error)?;
            let loaded = wait!(reader.LoadAsync(size as u32));
            if loaded as usize != size { return Err("Could not read a complete PDF page.".into()); }
            let mut bytes = vec![0; size];
            reader.ReadBytes(&mut bytes).map_err(native_error)?;
            images.push(Image::new("image/png", &bytes)?);
        }
        Ok(images)
    }
}
