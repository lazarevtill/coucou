use super::*;
use std::{fs, path::PathBuf, sync::atomic::{AtomicUsize, Ordering}};

struct Fixture { root: PathBuf, store: Store }
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("coucou-mcp-{}", crate::flow::text::new_id()));
        fs::create_dir_all(root.join("Notes")).unwrap();
        fs::create_dir_all(root.join("config")).unwrap();
        let store = Store::new(root.join("Notes"), root.join("config/mcp-access.json"));
        Self { root, store }
    }
    fn note(&self, id: &str, title: &str, start: u64, text: &str) {
        fs::create_dir_all(self.store.notes.join(id)).unwrap();
        fs::write(self.store.notes.join(id).join("note.json"), json!({
            "id":id, "title":title, "start":start, "status":"ready",
            "speakers":[{"id":"me","name":"Alex","color":0,"isMe":true}],
            "lines":[{"id":"line-1","speaker":"me","start":1.2,"end":3.4,"text":text}],
            "summary":"Launch decisions", "keyPoints":["Ship after review"],
            "todos":[{"id":"task-1","text":"Prepare release","owner":"Sam","due":"Friday","done":false}],
            "error":"private path: C:/never/expose-this", "app":"Private window title"
        }).to_string()).unwrap();
    }
    fn call(&self, name: &str, args: Value) -> Result<Value, String> { self.store.call(name, args, || false) }
}
impl Drop for Fixture {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.root); }
}

#[test]
fn access_is_off_by_default_and_reads_do_not_create_settings() {
    let f = Fixture::new();
    f.note("call-1", "Private call", 1, "private text");
    assert!(!f.store.enabled().unwrap());
    for (tool, args) in [("search", json!({"query":""})), ("fetch", json!({"id":"call-1"}))] {
        let error = f.call(tool, args).unwrap_err();
        assert!(error.contains("access is off"));
        assert!(!error.contains("private text"));
    }
    assert!(!f.store.access.exists());
}

#[test]
fn opt_in_is_persisted_and_revocation_affects_existing_servers() {
    let f = Fixture::new();
    f.note("call-1", "Call", 1, "Hello");
    let existing = f.store.clone();
    f.store.set_enabled(true).unwrap();
    assert!(existing.call("fetch", json!({"id":"call-1"}), || false).is_ok());
    f.store.set_enabled(false).unwrap();
    assert!(existing.call("fetch", json!({"id":"call-1"}), || false).is_err());
}

#[test]
fn access_read_errors_are_denials_and_failed_saves_preserve_permission() {
    let f = Fixture::new();
    fs::write(&f.store.access, b"{broken").unwrap();
    assert!(f.store.enabled().is_err());
    assert!(f.store.set_enabled(true).is_err());
    f.store.set_enabled(false).unwrap();
    assert!(!f.store.enabled().unwrap());
    fs::remove_file(&f.store.access).unwrap();
    fs::create_dir(&f.store.access).unwrap();
    assert!(f.store.set_enabled(true).is_err());
    assert!(f.store.set_enabled(false).is_err());
    assert!(f.store.access.is_dir());
    assert_eq!(fs::read_dir(f.store.access.parent().unwrap()).unwrap().count(), 1);
}

#[test]
fn search_covers_saved_text_and_fetch_preserves_actions_speakers_and_times() {
    let f = Fixture::new();
    f.note("one", "Roadmap", 1, "Привет 世界 🦉");
    f.store.set_enabled(true).unwrap();
    for query in ["roadmap", "LAUNCH", "review", "friday", "sam", "ПРИВЕТ", "世界", "Alex"] {
        let result = f.call("search", json!({"query":query})).unwrap();
        assert_eq!(result["results"][0]["id"], "one", "query {query}");
    }
    let note = f.call("fetch", json!({"id":"one"})).unwrap();
    let text = note["text"].as_str().unwrap();
    for expected in ["Привет 世界 🦉", "1.20–3.40s", "Alex", "Prepare release", "Sam", "Friday", "Ship after review"] { assert!(text.contains(expected)); }
    assert_eq!(note["nextOffset"], Value::Null);
    assert_eq!(note["partial"], false);
    assert!(!note.to_string().contains("expose-this"));
    assert!(!note.to_string().contains("Private window title"));
    assert_eq!(f.call("search", json!({"query":"Automatic speaker identities"})).unwrap()["total"], 0);
}

#[test]
fn recent_calls_have_stable_order_and_search_pages_require_the_current_revision() {
    let f = Fixture::new();
    for (id, start) in [("a", 1), ("b", 3), ("c", 3)] { f.note(id, id, start, "Hello"); }
    f.store.set_enabled(true).unwrap();
    let first = f.call("search", json!({"query":"", "limit":1})).unwrap();
    assert_eq!(first["results"][0]["id"], "b");
    assert_eq!(first["total"], 3);
    assert_eq!(first["nextOffset"], 1);
    let next = json!({"query":"", "limit":1,"offset":1,"revision":first["revision"]});
    assert_eq!(f.call("search", next.clone()).unwrap()["results"][0]["id"], "c");
    assert!(f.call("search", json!({"query":"", "offset":1})).is_err());
    f.note("d", "New call", 5, "Hello");
    assert!(f.call("search", next).unwrap_err().contains("changed"));
}

#[test]
fn unicode_transcripts_paginate_without_missing_characters_and_reject_stale_pages() {
    let f = Fixture::new();
    let source = "я🦉界".repeat(store::PAGE_CHARS);
    f.note("long", "Long call", 1, &source);
    f.store.set_enabled(true).unwrap();
    let first = f.call("fetch", json!({"id":"long"})).unwrap();
    let mut output = first["text"].as_str().unwrap().to_owned();
    let mut page = first.clone();
    while let Some(offset) = page["nextOffset"].as_u64() {
        page = f.call("fetch", json!({"id":"long", "offset":offset, "revision":first["revision"]})).unwrap();
        assert!(page["text"].as_str().unwrap().chars().count() <= store::PAGE_CHARS);
        output.push_str(page["text"].as_str().unwrap());
    }
    assert_eq!(output.chars().count() as u64, first["totalCharacters"].as_u64().unwrap());
    assert!(output.contains(&source));
    assert!(f.call("fetch", json!({"id":"long", "offset":1})).is_err());
    f.note("long", "Long call", 1, "New transcript");
    assert!(f.call("fetch", json!({"id":"long", "offset":1,"revision":first["revision"]})).is_err());
}

#[test]
fn invalid_paths_arguments_unknown_tools_and_id_mismatches_are_rejected() {
    let f = Fixture::new();
    f.note("good", "Call", 1, "Hello");
    f.store.set_enabled(true).unwrap();
    for id in ["../good", "..\\good", "C:\\notes", "/etc/passwd", "good/note.json", "good:secret", "", "."] {
        assert!(f.call("fetch", json!({"id":id})).is_err(), "{id}");
    }
    for args in [json!({"query":"", "limit":0}), json!({"query":"", "limit":26}), json!({"query":"x".repeat(257)}), json!({"query":"", "offset":-1}), json!({"query":"", "path":"x"})] {
        assert!(f.call("search", args).is_err());
    }
    assert!(f.call("delete", json!({"id":"good"})).is_err());
    fs::rename(f.store.notes.join("good"), f.store.notes.join("renamed")).unwrap();
    assert!(f.call("fetch", json!({"id":"renamed"})).unwrap_err().contains("ID"));
}

#[test]
fn corrupt_or_oversized_notes_make_search_visibly_incomplete() {
    let f = Fixture::new();
    f.note("good", "Call", 1, "Hello");
    f.note("broken", "Call", 2, "Hello");
    fs::write(f.store.notes.join("broken/note.json"), b"{broken").unwrap();
    f.note("huge", "Call", 3, "Hello");
    fs::File::create(f.store.notes.join("huge/note.json")).unwrap().set_len(16 * 1024 * 1024 + 1).unwrap();
    f.store.set_enabled(true).unwrap();
    let result = f.call("search", json!({"query":""})).unwrap();
    assert_eq!(result["results"].as_array().unwrap().len(), 1);
    assert_eq!(result["skippedNotes"], 2);
    assert_eq!(result["partial"], true);
    assert!(result["warning"].is_string());
    assert!(f.call("fetch", json!({"id":"huge"})).is_err());
}

#[test]
fn unfinished_notes_and_capture_gaps_are_not_presented_as_complete() {
    let f = Fixture::new();
    f.note("call", "Call", 1, "Saved so far");
    let file = f.store.notes.join("call/note.json");
    let mut note: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    note["status"] = json!("processing");
    note["captureWarning"] = json!("A private error with paths");
    fs::write(file, note.to_string()).unwrap();
    f.store.set_enabled(true).unwrap();
    let result = f.call("fetch", json!({"id":"call"})).unwrap();
    assert_eq!(result["partial"], true);
    assert_eq!(result["hasCaptureWarnings"], true);
    assert!(result["text"].as_str().unwrap().contains("Saved so far"));
    assert!(!result.to_string().contains("private error"));
}

#[test]
fn requests_check_cancellation_and_revocation_before_releasing_text() {
    let f = Fixture::new();
    f.note("call", "Call", 1, "Secret transcript");
    f.store.set_enabled(true).unwrap();
    assert!(f.store.call("fetch", json!({"id":"call"}), || true).unwrap_err().contains("cancelled"));
    let checks = AtomicUsize::new(0);
    let error = f.store.call("search", json!({"query":""}), || {
        if checks.fetch_add(1, Ordering::Relaxed) == 1 { f.store.set_enabled(false).unwrap(); }
        false
    }).unwrap_err();
    assert!(error.contains("access is off"));
    assert!(!error.contains("Secret transcript"));
}

#[test]
fn search_and_fetch_leave_every_saved_byte_unchanged() {
    let f = Fixture::new();
    f.note("call", "Call", 1, "Unchanged");
    f.store.set_enabled(true).unwrap();
    let path = f.store.notes.join("call/note.json");
    let before = fs::read(&path).unwrap();
    let access = fs::read(&f.store.access).unwrap();
    f.call("search", json!({"query":""})).unwrap();
    f.call("fetch", json!({"id":"call"})).unwrap();
    assert_eq!(fs::read(path).unwrap(), before);
    assert_eq!(fs::read(&f.store.access).unwrap(), access);
    assert_eq!(fs::read_dir(f.store.notes.join("call")).unwrap().count(), 1);
}

#[test]
fn protocol_advertises_only_read_only_local_tools_and_no_credentials_or_actions() {
    let tools = tools();
    assert_eq!(tools.iter().map(|t| t.name.as_ref()).collect::<Vec<_>>(), ["search", "fetch"]);
    for tool in tools {
        let annotations = tool.annotations.unwrap();
        assert_eq!(annotations.read_only_hint, Some(true));
        assert_eq!(annotations.destructive_hint, Some(false));
        assert_eq!(annotations.open_world_hint, Some(false));
        assert_eq!(tool.input_schema["additionalProperties"], false);
    }
}

#[cfg(unix)]
#[test]
fn symlinked_files_and_directories_cannot_escape_notes() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    f.note("outside", "Private", 1, "Must not leak");
    fs::rename(f.store.notes.join("outside"), f.root.join("outside")).unwrap();
    symlink(f.root.join("outside"), f.store.notes.join("outside")).unwrap();
    f.store.set_enabled(true).unwrap();
    assert!(f.call("fetch", json!({"id":"outside"})).is_err());
    assert_eq!(f.call("search", json!({"query":""})).unwrap()["skippedNotes"], 1);
}

#[cfg(windows)]
#[test]
fn windows_junctions_cannot_expose_files_outside_notes() {
    use std::os::windows::process::CommandExt;
    let f = Fixture::new();
    f.note("outside", "Private", 1, "Must not leak");
    fs::rename(f.store.notes.join("outside"), f.root.join("outside")).unwrap();
    let result = std::process::Command::new("powershell.exe").creation_flags(0x0800_0000)
        .args(["-NoProfile", "-NonInteractive", "-Command",
            "New-Item -ItemType Junction -Path $env:COUCOU_MCP_LINK -Target $env:COUCOU_MCP_TARGET -ErrorAction Stop | Out-Null"])
        .env("COUCOU_MCP_LINK", f.store.notes.join("outside"))
        .env("COUCOU_MCP_TARGET", f.root.join("outside"))
        .output().unwrap();
    assert!(result.status.success(), "junction fixture: {}", String::from_utf8_lossy(&result.stderr));
    f.store.set_enabled(true).unwrap();
    assert!(f.call("fetch", json!({"id":"outside"})).is_err());
    assert_eq!(f.call("search", json!({"query":""})).unwrap()["skippedNotes"], 1);
    fs::remove_dir(f.store.notes.join("outside")).unwrap();
    assert!(f.root.join("outside/note.json").is_file());
}
