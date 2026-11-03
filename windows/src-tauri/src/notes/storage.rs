//! Commit the final transcript before releasing its source audio.
use std::path::Path;
use super::{Note, Status, builder::{Line, Speaker}};

pub(super) fn save(dir: &Path, note: &Note) -> Result<(), String> {
    let write = || -> Result<(), Box<dyn std::error::Error>> {
        crate::platform::ensure_private_dir(dir)?;
        let bytes = serde_json::to_vec_pretty(note)?;
        crate::config_file::replace(&dir.join("note.json"), &bytes)?;
        Ok(())
    };
    write().map_err(|error| format!("Could not save call notes: {error}"))
}

pub(super) fn finish(dir: &Path, note: &mut Note, speakers: Vec<Speaker>, lines: Vec<Line>) -> Result<(), String> {
    let mut completed = note.clone();
    completed.speakers = speakers;
    completed.lines = lines;
    completed.status = Status::Ready;
    completed.error = None;
    save(dir, &completed)?;
    *note = completed;
    remove_audio(dir);
    Ok(())
}

/// Only once the Ready transcript is saved; launch repeats it for audio left
/// by a failed removal or a crash right after the save.
pub(super) fn remove_audio(dir: &Path) {
    for name in ["mic.wav", "system.wav"] {
        if let Err(error) = std::fs::remove_file(dir.join(name)) {
            if error.kind() != std::io::ErrorKind::NotFound {
                crate::log::line(format!("notes: saved transcript; could not remove {name}: {error}"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_transcript_commit_preserves_audio_and_live_lines_then_retries() {
        let dir = std::env::temp_dir().join(format!("coucou-note-save-{}", crate::flow::text::new_id()));
        let mut note: Note = serde_json::from_value(serde_json::json!({
            "id": "test", "title": "Synthetic call", "start": 10, "status": "processing",
            "lines": [{"id":"live", "speaker":"me", "start":0, "end":1, "text":"Live preview"}]
        })).unwrap();
        save(&dir, &note).unwrap();
        let original = std::fs::read(dir.join("note.json")).unwrap();
        for name in ["mic.wav", "system.wav"] { std::fs::write(dir.join(name), b"recording").unwrap(); }
        // A failed staging write must not make the live or stored note Ready.
        let staging = dir.join(format!("note.json.coucou-{}", std::process::id()));
        std::fs::create_dir(&staging).unwrap();
        assert!(finish(&dir, &mut note, vec![], vec![]).is_err());
        assert_eq!(note.status, Status::Processing);
        assert_eq!(note.lines[0].text, "Live preview");
        assert_eq!(std::fs::read(dir.join("note.json")).unwrap(), original);
        for name in ["mic.wav", "system.wav"] { assert_eq!(std::fs::read(dir.join(name)).unwrap(), b"recording"); }
        std::fs::remove_dir(&staging).unwrap();
        finish(&dir, &mut note, vec![], vec![]).unwrap();
        let saved: Note = serde_json::from_slice(&std::fs::read(dir.join("note.json")).unwrap()).unwrap();
        assert_eq!(saved.status, Status::Ready);
        assert_eq!(note.status, Status::Ready);
        assert!(!dir.join("mic.wav").exists());
        assert!(!dir.join("system.wav").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failed_atomic_replace_keeps_both_recordings() {
        let dir = std::env::temp_dir().join(format!("coucou-note-rename-{}", crate::flow::text::new_id()));
        std::fs::create_dir_all(dir.join("note.json")).unwrap();
        let mut note: Note = serde_json::from_value(serde_json::json!({"id":"test", "title":"Test", "start":0, "status":"processing"})).unwrap();
        for name in ["mic.wav", "system.wav"] { std::fs::write(dir.join(name), b"audio").unwrap(); }
        assert!(finish(&dir, &mut note, vec![], vec![]).is_err());
        assert_eq!(note.status, Status::Processing);
        for name in ["mic.wav", "system.wav"] { assert!(dir.join(name).is_file()); }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
