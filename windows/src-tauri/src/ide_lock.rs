// What Claude Code's IDE integration records about the editor window a session is
// connected to: `~/.claude/ide/<port>.lock`, where `<port>` is the value of
// CLAUDE_CODE_SSE_PORT in the session's environment.
//
// The format is not documented by Anthropic, so this is enrichment only — it
// tells us which editor and which workspace folder a session belongs to when the
// process chain alone cannot (Cursor is one process owning every window). Every
// failure here means "no extra information", never an error.
//
// The file also holds an `authToken` for the IDE's local socket. It is not
// modelled, never read into memory by this code, and so can never be logged.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// A real lock file is a few hundred bytes. Anything much larger is not one.
const MAX_LOCK_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IdeLock {
    /// The editor's main process — the one that owns every one of its windows.
    pub pid: u32,
    pub ide_name: String,
    #[serde(default)]
    pub workspace_folders: Vec<String>,
}

pub fn parse(bytes: &[u8]) -> Option<IdeLock> {
    serde_json::from_slice(bytes).ok()
}

/// `<home>\.claude\ide`.
pub fn dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".claude").join("ide"))
}

/// Reads `<dir>\<port>.lock`. `port` comes from a session's environment, so it is
/// checked to be a plain port number before it is ever put in a path.
pub fn read_from(dir: &Path, port: &str) -> Option<IdeLock> {
    if port.is_empty() || port.len() > 5 || !port.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let path = dir.join(format!("{port}.lock"));
    if std::fs::metadata(&path).ok()?.len() > MAX_LOCK_BYTES {
        return None;
    }
    parse(&std::fs::read(path).ok()?)
}

pub fn read(port: &str) -> Option<IdeLock> {
    read_from(&dir()?, port)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape found on a live machine (user name and token replaced).
    const LIVE: &str = r#"{"pid":43196,"workspaceFolders":["c:\\Users\\someone\\Documents\\work\\shop-api"],"ideName":"Cursor","transport":"ws","runningInWindows":true,"authToken":"SECRET-TOKEN-VALUE"}"#;

    #[test]
    fn reads_the_editor_pid_name_and_workspace() {
        let lock = parse(LIVE.as_bytes()).expect("the live shape must parse");
        assert_eq!(lock.pid, 43196);
        assert_eq!(lock.ide_name, "Cursor");
        assert_eq!(lock.workspace_folders, vec![r"c:\Users\someone\Documents\work\shop-api"]);
    }

    #[test]
    fn the_auth_token_is_never_held_so_it_can_never_be_printed() {
        let lock = parse(LIVE.as_bytes()).unwrap();
        assert!(!format!("{lock:?}").contains("SECRET-TOKEN-VALUE"));
    }

    #[test]
    fn a_window_without_a_folder_is_fine() {
        let lock = parse(br#"{"pid":7,"ideName":"Cursor","workspaceFolders":[]}"#).unwrap();
        assert!(lock.workspace_folders.is_empty());
        let lock = parse(br#"{"pid":7,"ideName":"Cursor"}"#).unwrap();
        assert!(lock.workspace_folders.is_empty());
    }

    #[test]
    fn anything_that_is_not_a_lock_file_is_nothing() {
        assert!(parse(b"").is_none());
        assert!(parse(b"not json").is_none());
        assert!(parse(br#"{"ideName":"Cursor"}"#).is_none(), "no pid");
        assert!(parse(br#"{"pid":"x","ideName":"Cursor"}"#).is_none(), "pid must be a number");
    }

    #[test]
    fn the_port_is_checked_before_it_becomes_a_path() {
        let dir = std::env::temp_dir().join(format!("coucou-ide-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("36005.lock"), LIVE).unwrap();
        // A file one level up, which a traversal would reach.
        std::fs::write(dir.parent().unwrap().join("coucou-ide-escape.lock"), LIVE).unwrap();

        assert!(read_from(&dir, "36005").is_some());
        for bad in ["", "..", "..\\coucou-ide-escape", "../coucou-ide-escape", "36005 ", "-1", "123456", "3600a"] {
            assert!(read_from(&dir, bad).is_none(), "{bad:?} must be refused");
        }
        assert!(read_from(&dir, "1").is_none(), "a port with no file is simply nothing");

        let _ = std::fs::remove_file(dir.parent().unwrap().join("coucou-ide-escape.lock"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_oversized_file_is_not_read() {
        let dir = std::env::temp_dir().join(format!("coucou-ide-big-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let padding = " ".repeat(70 * 1024);
        std::fs::write(dir.join("1.lock"), format!(r#"{{"pid":1,"ideName":"Cursor"}}{padding}"#)).unwrap();
        assert!(read_from(&dir, "1").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
