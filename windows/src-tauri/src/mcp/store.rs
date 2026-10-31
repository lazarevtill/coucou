use std::{fs::{self, File, OpenOptions}, io::{self, Read, Write}, path::{Path, PathBuf}, sync::Mutex};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::notes::{Note, Status};

const MAX_NOTE_BYTES: usize = 16 * 1024 * 1024;
const MAX_SCAN_BYTES: usize = 256 * 1024 * 1024;
const MAX_NOTES: usize = 10_000;
pub(super) const PAGE_CHARS: usize = 12_000;
const DISABLED: &str = "Transcript access is off. Enable Local agents (MCP) in Coucou Settings > Integrations.";
static SAVE_ACCESS: Mutex<()> = Mutex::new(());

struct ReadBudget { remaining: usize, exhausted: bool }
impl ReadBudget {
    fn new(remaining: usize) -> Self { Self { remaining, exhausted: false } }
}

#[derive(Clone)]
pub(super) struct Store {
    pub(super) notes: PathBuf,
    pub(super) access: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Access { enabled: bool }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Search {
    query: String,
    #[serde(default)] offset: usize,
    #[serde(default = "default_limit")] limit: usize,
    #[serde(default)] revision: Option<String>,
}
fn default_limit() -> usize { 10 }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fetch {
    id: String,
    #[serde(default)] offset: usize,
    #[serde(default)] revision: Option<String>,
}

impl Store {
    pub(super) fn new(notes: PathBuf, access: PathBuf) -> Self { Self { notes, access } }

    pub(super) fn enabled(&self) -> Result<bool, String> {
        let root = self.access.parent().ok_or("The MCP access folder is unavailable.")?;
        let name = self.access.file_name().ok_or("The MCP access file is unavailable.")?;
        match read_scoped(root, Path::new(name), 1024, &mut ReadBudget::new(1024)) {
            Ok(bytes) => serde_json::from_slice::<Access>(&bytes).map(|v| v.enabled)
                .map_err(|_| "The MCP access setting is invalid. Access is blocked.".into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(_) => Err("Could not read the MCP access setting. Access is blocked.".into()),
        }
    }

    pub(super) fn check_enabled(&self) -> Result<(), String> {
        if self.enabled()? { Ok(()) } else { Err(DISABLED.into()) }
    }

    pub(super) fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        let _lock = SAVE_ACCESS.lock().map_err(|_| "Could not save MCP access.")?;
        // A corrupt setting never silently becomes permission to read calls.
        if enabled { self.enabled()?; }
        let root = self.access.parent().ok_or("The MCP access folder is unavailable.")?;
        crate::platform::ensure_private_dir(root).map_err(|_| "Could not create the MCP access folder.")?;
        let temp = root.join(format!("mcp-access-{}.tmp", crate::flow::text::new_id()));
        let written = (|| -> io::Result<()> {
            let mut file = OpenOptions::new().write(true).create_new(true).open(&temp)?;
            file.write_all(if enabled { b"{\"enabled\":true}\n" } else { b"{\"enabled\":false}\n" })?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temp, &self.access)
        })();
        if written.is_err() { let _ = fs::remove_file(&temp); }
        written.map_err(|_| "Could not save MCP access. The saved permission is unchanged.".into())
    }

    pub(super) fn call(&self, name: &str, args: Value, cancelled: impl Fn() -> bool) -> Result<Value, String> {
        self.check_enabled()?;
        let checkpoint = || {
            if cancelled() { return Err("The notes request was cancelled.".into()); }
            self.check_enabled()
        };
        let output = match name {
            "search" => self.search(serde_json::from_value(args).map_err(|_| "Invalid search arguments.")?, &checkpoint, &mut ReadBudget::new(MAX_SCAN_BYTES)),
            "fetch" => self.fetch(serde_json::from_value(args).map_err(|_| "Invalid fetch arguments.")?),
            _ => Err("Unknown tool.".into()),
        }?;
        checkpoint()?;
        Ok(output)
    }

    fn read_note(&self, id: &str, budget: &mut ReadBudget) -> Result<Note, String> {
        if !valid_id(id) { return Err("Invalid note ID.".into()); }
        let bytes = read_scoped(&self.notes, &Path::new(id).join("note.json"), MAX_NOTE_BYTES, budget)
            .map_err(|_| "The note is missing, unreadable, linked outside its folder or exceeds the 16 MiB limit.")?;
        let note: Note = serde_json::from_slice(&bytes).map_err(|_| "The saved note is invalid.")?;
        if note.id != id { return Err("The note ID does not match its folder.".into()); }
        Ok(note)
    }

    fn search(&self, args: Search, checkpoint: &impl Fn() -> Result<(), String>, budget: &mut ReadBudget) -> Result<Value, String> {
        if args.query.chars().count() > 256 || !(1..=25).contains(&args.limit) || args.offset > MAX_NOTES {
            return Err("Use a query of at most 256 characters, a limit of 1–25 and a valid result offset.".into());
        }
        let query = args.query.trim().to_lowercase();
        let entries = match fs::read_dir(&self.notes) {
            Ok(entries) => Some(entries),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(_) => return Err("Could not read saved calls.".into()),
        };
        let mut results: Vec<(u64, String, Value)> = Vec::new();
        let mut skipped = 0;
        for (index, entry) in entries.into_iter().flatten().enumerate() {
            if index >= MAX_NOTES { return Err("The call library exceeds the 10,000-entry search limit.".into()); }
            checkpoint()?;
            let entry = entry.map_err(|_| "Could not list saved calls.")?;
            let Some(id) = entry.file_name().to_str().filter(|id| valid_id(id)).map(str::to_owned) else { skipped += 1; continue; };
            let note = self.read_note(&id, budget);
            if budget.exhausted { return Err("The call library exceeds the 256 MiB search limit. Fetch a known call by ID instead.".into()); }
            let note = match note {
                Ok(note) => note,
                Err(_) => { skipped += 1; continue; }
            };
            let text = searchable_text(&note);
            if !query.is_empty() && !text.to_lowercase().contains(&query) { continue; }
            let excerpt = snippet(&text, &query);
            let value = json!({"id":note.id, "title":clip(&note.title, 240), "url":format!("coucou://notes/{}", note.id),
                "start":note.start, "status":note.status, "partial":incomplete(&note), "snippet":excerpt});
            results.push((note.start, note.id, value));
        }
        results.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let revision = digest(&json!([query, results]).to_string());
        if args.offset > 0 && args.revision.as_deref() != Some(&revision) {
            return Err("Search results changed or their revision is missing. Restart at offset 0.".into());
        }
        if args.offset > results.len() { return Err("Search results changed or the offset is invalid. Restart at offset 0.".into()); }
        let total = results.len();
        let end = (args.offset + args.limit).min(total);
        let page: Vec<_> = results.into_iter().skip(args.offset).take(args.limit).map(|(_, _, value)| value).collect();
        Ok(json!({"results":page, "total":total, "revision":revision, "nextOffset":(end < total).then_some(end),
            "skippedNotes":skipped, "partial":skipped > 0,
            "warning":(skipped > 0).then_some("Some saved calls could not be read. Search results are incomplete; inspect them in Coucou.")}))
    }

    fn fetch(&self, args: Fetch) -> Result<Value, String> {
        let note = self.read_note(&args.id, &mut ReadBudget::new(MAX_NOTE_BYTES))?;
        let text = document(&note);
        let revision = digest(&text);
        if args.offset > 0 && args.revision.as_deref() != Some(&revision) {
            return Err("The note changed or its revision is missing. Restart fetch at offset 0.".into());
        }
        let total = text.chars().count();
        if args.offset > total { return Err("Invalid transcript offset.".into()); }
        let page: String = text.chars().skip(args.offset).take(PAGE_CHARS).collect();
        let end = args.offset + page.chars().count();
        Ok(json!({"id":note.id, "title":clip(&note.title, 240), "url":format!("coucou://notes/{}", note.id),
            "text":page, "revision":revision, "offset":args.offset, "nextOffset":(end < total).then_some(end),
            "totalCharacters":total, "status":note.status, "partial":incomplete(&note),
            "hasCaptureWarnings":has_warnings(&note)}))
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn incomplete(note: &Note) -> bool { note.status != Status::Ready }
fn has_warnings(note: &Note) -> bool {
    note.capture_warning.is_some() || note.audio_sources.iter().any(|s| s.error.is_some() || !s.warnings.is_empty())
}
fn clip(text: &str, count: usize) -> String { text.chars().take(count).collect() }
fn digest(text: &str) -> String {
    ring::digest::digest(&ring::digest::SHA256, text.as_bytes()).as_ref()
        .iter().map(|byte| format!("{byte:02x}")).collect()
}

fn snippet(text: &str, query: &str) -> String {
    // Lowercasing can change byte/character counts (e.g. Turkish dotted I), so
    // find the matching line and slice the original only on character boundaries.
    let line = text.lines().find(|line| !query.is_empty() && line.to_lowercase().contains(query)).unwrap_or(text);
    clip(line, 280)
}

fn searchable_text(note: &Note) -> String {
    // Only saved content is searchable, not generated headings/status labels.
    let mut fields = vec![note.title.as_str()];
    fields.extend(note.summary.as_deref());
    fields.extend(note.key_points.iter().map(String::as_str));
    for todo in &note.todos {
        fields.push(&todo.text);
        fields.extend(todo.owner.as_deref());
        fields.extend(todo.due.as_deref());
    }
    fields.extend(note.speakers.iter().map(|speaker| speaker.name.as_str()));
    fields.extend(note.lines.iter().map(|line| line.text.as_str()));
    fields.join("\n")
}

fn document(note: &Note) -> String {
    use std::fmt::Write;
    let mut text = format!("# {}\n\nStarted (Unix ms): {}\nStatus: {:?}\n", note.title, note.start, note.status);
    if let Some(end) = note.end { let _ = writeln!(text, "Ended (Unix ms): {end}"); }
    if incomplete(note) { text.push_str("This saved call is incomplete. Processing may change its text.\n"); }
    if has_warnings(note) { text.push_str("Capture warnings were saved. Some audio may be missing; inspect Capture details in Coucou.\n"); }
    text.push_str("\n## Summary\n");
    text.push_str(note.summary.as_deref().unwrap_or("No saved summary."));
    text.push_str("\n\n## Key points\n");
    for point in &note.key_points { let _ = writeln!(text, "- {point}"); }
    text.push_str("\n## Action items\n");
    for todo in &note.todos {
        let _ = writeln!(text, "- [{}] {} | Owner: {} | Due: {}", if todo.done { "x" } else { " " }, todo.text,
            todo.owner.as_deref().unwrap_or("Unassigned"), todo.due.as_deref().unwrap_or("Not set"));
    }
    text.push_str("\n## Speakers\nAutomatic speaker identities are estimates.\n");
    let mut speakers = std::collections::HashMap::new();
    for speaker in &note.speakers { speakers.entry(speaker.id.as_str()).or_insert(speaker.name.as_str()); }
    for speaker in &note.speakers { let _ = writeln!(text, "- {}: {}{}", speaker.id, speaker.name, if speaker.is_me { " (microphone)" } else { "" }); }
    text.push_str("\n## Transcript\n");
    for line in &note.lines {
        let speaker = speakers.get(line.speaker.as_str()).copied().unwrap_or(&line.speaker);
        let _ = writeln!(text, "[{:.2}–{:.2}s] {}: {}", line.start, line.end, speaker, line.text);
    }
    text
}

fn denied() -> io::Error { io::Error::new(io::ErrorKind::PermissionDenied, "Not a regular file in the allowed folder") }

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0 // FILE_ATTRIBUTE_REPARSE_POINT, including junctions
    }
    #[cfg(not(windows))]
    { metadata.file_type().is_symlink() }
}

/// Validate every relative component and then the *opened handle*, so replacing
/// a directory with a junction/symlink between checking and opening cannot leak
/// an unrelated file. The caller provides only generated paths, never MCP paths.
fn read_scoped(root: &Path, relative: &Path, maximum: usize, budget: &mut ReadBudget) -> io::Result<Vec<u8>> {
    if !root.is_absolute() || is_link(&fs::symlink_metadata(root)?) { return Err(denied()); }
    let root = fs::canonicalize(root)?;
    let mut expected = root.clone();
    for component in relative.components() {
        let std::path::Component::Normal(name) = component else { return Err(denied()); };
        expected.push(name);
        if is_link(&fs::symlink_metadata(&expected)?) { return Err(denied()); }
    }
    if !fs::symlink_metadata(&expected)?.is_file() { return Err(denied()); }
    let file = File::open(&expected)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > maximum as u64 || opened_path(&file)? != expected { return Err(denied()); }
    if metadata.len() > budget.remaining as u64 { budget.exhausted = true; return Err(denied()); }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    // Charge every byte read, including corrupt JSON, partial I/O failures and
    // files that grow after the metadata check. One guard byte detects growth.
    let read = file.take((maximum as u64 + 1).min(budget.remaining as u64 + 1)).read_to_end(&mut bytes);
    if bytes.len() > budget.remaining { budget.exhausted = true; }
    budget.remaining = budget.remaining.saturating_sub(bytes.len());
    read?;
    if budget.exhausted { return Err(denied()); }
    if bytes.len() > maximum { return Err(denied()); }
    Ok(bytes)
}

#[cfg(test)]
mod budget_tests {
    use super::*;

    #[test]
    fn malformed_notes_count_toward_the_aggregate_search_read_limit() {
        let root = std::env::temp_dir().join(format!("coucou-mcp-budget-{}", crate::flow::text::new_id()));
        fs::create_dir_all(root.join("Notes/a")).unwrap();
        fs::create_dir_all(root.join("Notes/b")).unwrap();
        fs::write(root.join("Notes/a/note.json"), b"{broken").unwrap();
        fs::write(root.join("Notes/b/note.json"), b"{broken").unwrap();
        let store = Store::new(root.join("Notes"), root.join("mcp-access.json"));
        let args = Search { query: String::new(), offset: 0, limit: 10, revision: None };
        let mut budget = ReadBudget::new(7);
        let result = store.search(args, &|| Ok(()), &mut budget);
        assert!(result.unwrap_err().contains("search limit"));
        assert_eq!(budget.remaining, 0);
        assert!(budget.exhausted);
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(windows)]
fn opened_path(file: &File) -> io::Result<PathBuf> {
    use std::os::windows::{io::AsRawHandle, ffi::OsStringExt};
    use windows::Win32::{Foundation::HANDLE, Storage::FileSystem::{GetFinalPathNameByHandleW, FILE_NAME_NORMALIZED}};
    let mut path = vec![0u16; 32768];
    let length = unsafe { GetFinalPathNameByHandleW(HANDLE(file.as_raw_handle()), &mut path, FILE_NAME_NORMALIZED) } as usize;
    if length == 0 || length >= path.len() { return Err(io::Error::last_os_error()); }
    Ok(PathBuf::from(std::ffi::OsString::from_wide(&path[..length])))
}

#[cfg(target_os = "linux")]
fn opened_path(file: &File) -> io::Result<PathBuf> {
    use std::os::fd::AsRawFd;
    fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd()))
}
