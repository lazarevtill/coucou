// What Rust remembers about each live Claude Code session: enough to find its
// window. The island keeps everything it needs to *display* a session; this is
// only the part that touches the operating system.
//
// Every hook payload that carries a `session_id` passes through `observe`, which
// also decides where the session lives (see host.rs) and hands that back so the
// island can label it.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::host::{self, ChainEntry, HostInfo, Signals};
use crate::ide_lock::IdeLock;
use crate::platform::Candidate;

/// More sessions than this and the oldest is forgotten.
const MAX_SESSIONS: usize = 64;
/// A session that has said nothing for this long is gone, whether or not it
/// ever said goodbye.
const STALE_AFTER: Duration = Duration::from_secs(6 * 60 * 60);
const MAX_CHAIN: usize = 12;
const MAX_ID_LEN: usize = 128;

#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub id: String,
    pub cwd: String,
    pub chain: Vec<ChainEntry>,
    pub host: HostInfo,
    pub ide: Option<IdeLock>,
    pub last_seen: Instant,
}

#[derive(Default)]
pub struct Sessions(Mutex<HashMap<String, SessionRecord>>);

/// The chain the relay sent, with anything that is not a plain pid and a plain
/// executable file name thrown away.
pub fn chain_from(value: Option<&Value>) -> Vec<ChainEntry> {
    let Some(items) = value.and_then(Value::as_array) else { return Vec::new() };
    items
        .iter()
        .filter_map(|item| {
            let pid = u32::try_from(item.get("pid")?.as_u64()?).ok().filter(|p| *p != 0)?;
            let exe = item.get("exe")?.as_str()?;
            // A file name, never a path: the relay only ever sends names, and a
            // path here would mean something else wrote this payload.
            if exe.is_empty() || exe.len() > 260 || exe.contains(['\\', '/']) {
                return None;
            }
            Some(ChainEntry { pid, exe: exe.to_string() })
        })
        .take(MAX_CHAIN)
        .collect()
}

fn text<'a>(payload: &'a Value, key: &str) -> &'a str {
    payload.get(key).and_then(Value::as_str).unwrap_or("")
}

impl Sessions {
    /// Records what a hook payload says about its session and returns where that
    /// session lives. `None` when the payload has no usable session id.
    pub fn observe(
        &self,
        payload: &Value,
        now: Instant,
        read_lock: &dyn Fn(&str) -> Option<IdeLock>,
    ) -> Option<HostInfo> {
        let id = payload.get("session_id")?.as_str()?;
        if id.is_empty() || id.len() > MAX_ID_LEN {
            return None;
        }
        let port = text(payload, "ide_port");

        // The lock file is read once per session, and not while holding the table.
        let known_ide = self.0.lock().unwrap().get(id).map(|r| r.ide.is_some()).unwrap_or(false);
        let fresh_ide = if !known_ide && !port.is_empty() { read_lock(port) } else { None };

        let mut map = self.0.lock().unwrap();
        let previous = map.get(id).cloned();
        let ide = fresh_ide.or_else(|| previous.as_ref().and_then(|r| r.ide.clone()));

        // An event without a chain (an older relay) must not erase a known one.
        let sent = chain_from(payload.get("host_chain"));
        let chain = if sent.is_empty() { previous.as_ref().map(|r| r.chain.clone()).unwrap_or_default() } else { sent };
        let cwd = match text(payload, "cwd") {
            "" => previous.as_ref().map(|r| r.cwd.clone()).unwrap_or_default(),
            c => c.to_string(),
        };

        let host = host::classify(&Signals {
            chain: &chain,
            hint: text(payload, "host_hint"),
            term_program: text(payload, "term_program"),
            has_wt_session: !text(payload, "wt_session").is_empty(),
            ide_name: ide.as_ref().map(|i| i.ide_name.as_str()),
        });

        if previous.is_none() && map.len() >= MAX_SESSIONS {
            if let Some(oldest) = map.values().min_by_key(|r| r.last_seen).map(|r| r.id.clone()) {
                map.remove(&oldest);
            }
        }
        map.insert(
            id.to_string(),
            SessionRecord { id: id.to_string(), cwd, chain, host: host.clone(), ide, last_seen: now },
        );
        Some(host)
    }

    pub fn get(&self, id: &str) -> Option<SessionRecord> {
        self.0.lock().unwrap().get(id).cloned()
    }

    pub fn remove(&self, id: &str) {
        self.0.lock().unwrap().remove(id);
    }

    pub fn len(&self) -> usize {
        self.0.lock().unwrap().len()
    }

    /// Forgets sessions whose Claude Code process has exited (a crash or a closed
    /// terminal sends no SessionEnd) and ones that went quiet long ago. Returns
    /// the ids that were dropped.
    pub fn prune(&self, now: Instant, alive: &dyn Fn(&ChainEntry) -> bool) -> Vec<String> {
        let mut dropped = Vec::new();
        self.0.lock().unwrap().retain(|id, record| {
            let stale = now.saturating_duration_since(record.last_seen) > STALE_AFTER;
            // Without a Claude Code entry there is nothing to ask the OS about.
            let dead = host::claude_entry(&record.chain).map(|c| !alive(c)).unwrap_or(false);
            if stale || dead {
                dropped.push(id.clone());
            }
            !(stale || dead)
        });
        dropped.sort();
        dropped
    }
}

/// The editor executable a lock file's `ideName` stands for. Only editors whose
/// image we can verify at click time are trusted with a pid from a file.
fn ide_exe(ide_name: &str) -> Option<&'static str> {
    ide_name.eq_ignore_ascii_case("cursor").then_some("Cursor.exe")
}

fn last_segment(path: &str) -> Option<String> {
    path.split(['\\', '/']).filter(|s| !s.is_empty()).last().map(str::to_string)
}

/// The processes whose windows could be the session's, best first, and the words
/// that tell its window from the host's others.
pub fn focus_target(record: &SessionRecord) -> (Vec<Candidate>, Vec<String>) {
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut add = |pid: u32, exe: &str| {
        if seen.insert(pid) {
            candidates.push(Candidate { pid, exe: Some(exe.to_string()) });
        }
    };

    // The editor's main process owns every one of its windows, so it comes first.
    if let Some(ide) = &record.ide {
        if let Some(exe) = ide_exe(&ide.ide_name) {
            add(ide.pid, exe);
        }
    }
    for entry in &record.chain {
        if host::host_of_exe(&entry.exe).is_some() {
            add(entry.pid, &entry.exe);
        }
    }

    let mut hints: Vec<String> = Vec::new();
    let folders = record.ide.iter().flat_map(|i| i.workspace_folders.iter());
    for name in folders.filter_map(|f| last_segment(f)).chain(last_segment(&record.cwd)) {
        if !hints.iter().any(|h| h.eq_ignore_ascii_case(&name)) {
            hints.push(name);
        }
    }
    (candidates, hints)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn no_lock(_: &str) -> Option<IdeLock> {
        None
    }

    fn payload(id: &str, chain: &[(u32, &str)]) -> Value {
        json!({
            "session_id": id,
            "cwd": r"C:\Users\u\proj",
            "host_hint": "",
            "term_program": "",
            "wt_session": "",
            "host_chain": chain.iter().map(|(p, e)| json!({"pid": p, "exe": e})).collect::<Vec<_>>(),
        })
    }

    const WT_CHAIN: &[(u32, &str)] =
        &[(90, "bash.exe"), (80, "claude.exe"), (70, "powershell.exe"), (60, "WindowsTerminal.exe")];

    // ── chain_from ────────────────────────────────────────────────────────────

    #[test]
    fn only_plain_pids_and_file_names_survive() {
        let v = json!([
            {"pid": 90, "exe": "bash.exe"},
            {"pid": 0, "exe": "zero.exe"},
            {"pid": -5, "exe": "negative.exe"},
            {"pid": 70, "exe": r"C:\Users\someone\evil.exe"},
            {"pid": 71, "exe": "a/b.exe"},
            {"pid": 72, "exe": ""},
            {"pid": 73},
            {"exe": "nopid.exe"},
            "not an object",
            {"pid": 80, "exe": "claude.exe", "extra": "ignored"},
        ]);
        assert_eq!(
            chain_from(Some(&v)),
            vec![
                ChainEntry { pid: 90, exe: "bash.exe".into() },
                ChainEntry { pid: 80, exe: "claude.exe".into() },
            ]
        );
    }

    #[test]
    fn a_missing_or_wrong_typed_chain_is_empty_and_a_long_one_is_capped() {
        assert!(chain_from(None).is_empty());
        assert!(chain_from(Some(&json!("x"))).is_empty());
        assert!(chain_from(Some(&json!({}))).is_empty());
        let long: Vec<Value> = (1..=40u32).map(|p| json!({"pid": p, "exe": "node.exe"})).collect();
        assert_eq!(chain_from(Some(&Value::Array(long))).len(), MAX_CHAIN);
    }

    // ── observe / get / remove ────────────────────────────────────────────────

    #[test]
    fn observing_a_payload_records_the_session_and_says_where_it_lives() {
        let s = Sessions::default();
        let host = s.observe(&payload("s1", WT_CHAIN), Instant::now(), &no_lock).unwrap();
        assert_eq!(host.label, "Windows Terminal");
        let rec = s.get("s1").unwrap();
        assert_eq!(rec.cwd, r"C:\Users\u\proj");
        assert_eq!(rec.chain.len(), 4);
    }

    #[test]
    fn a_payload_without_a_usable_session_id_is_ignored() {
        let s = Sessions::default();
        let now = Instant::now();
        assert!(s.observe(&json!({"hook_event_name": "Stop"}), now, &no_lock).is_none());
        assert!(s.observe(&json!({"session_id": ""}), now, &no_lock).is_none());
        assert!(s.observe(&json!({"session_id": 5}), now, &no_lock).is_none());
        assert!(s.observe(&json!({"session_id": "x".repeat(MAX_ID_LEN + 1)}), now, &no_lock).is_none());
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn a_later_event_refreshes_the_chain_but_the_ide_lock_is_read_once() {
        let s = Sessions::default();
        let reads = std::cell::Cell::new(0);
        let lock = |port: &str| {
            assert_eq!(port, "36005");
            reads.set(reads.get() + 1);
            Some(IdeLock { pid: 40, ide_name: "Cursor".into(), workspace_folders: vec![r"c:\p\shop-api".into()] })
        };
        let mut p = payload("s1", &[(80, "claude.exe"), (50, "Cursor.exe"), (40, "Cursor.exe")]);
        p["ide_port"] = json!("36005");
        let t = Instant::now();
        s.observe(&p, t, &lock);
        s.observe(&p, t + Duration::from_secs(1), &lock);
        s.observe(&p, t + Duration::from_secs(2), &lock);
        assert_eq!(reads.get(), 1, "the lock file must be read once per session, not once per event");
        assert_eq!(s.get("s1").unwrap().ide.unwrap().pid, 40);
    }

    #[test]
    fn remove_forgets_only_that_session() {
        let s = Sessions::default();
        let now = Instant::now();
        s.observe(&payload("a", WT_CHAIN), now, &no_lock);
        s.observe(&payload("b", WT_CHAIN), now, &no_lock);
        s.remove("a");
        assert!(s.get("a").is_none());
        assert!(s.get("b").is_some());
    }

    #[test]
    fn the_oldest_session_is_evicted_when_the_table_is_full() {
        let s = Sessions::default();
        let t = Instant::now();
        for i in 0..MAX_SESSIONS {
            s.observe(&payload(&format!("s{i}"), WT_CHAIN), t + Duration::from_secs(i as u64), &no_lock);
        }
        assert_eq!(s.len(), MAX_SESSIONS);
        s.observe(&payload("newest", WT_CHAIN), t + Duration::from_secs(1000), &no_lock);
        assert_eq!(s.len(), MAX_SESSIONS);
        assert!(s.get("s0").is_none(), "the least recently seen must go");
        assert!(s.get("newest").is_some());
        assert!(s.get("s1").is_some());
    }

    // ── prune ─────────────────────────────────────────────────────────────────

    #[test]
    fn a_session_whose_claude_process_is_gone_is_pruned() {
        let s = Sessions::default();
        let t = Instant::now();
        s.observe(&payload("alive", WT_CHAIN), t, &no_lock);
        s.observe(&payload("dead", &[(81, "claude.exe"), (60, "WindowsTerminal.exe")]), t, &no_lock);
        // Only claude 80 is still running.
        let dropped = s.prune(t, &|e| e.pid == 80);
        assert_eq!(dropped, vec!["dead".to_string()]);
        assert!(s.get("alive").is_some());
        assert!(s.get("dead").is_none());
    }

    #[test]
    fn without_a_claude_entry_liveness_cannot_be_judged_so_only_age_prunes() {
        let s = Sessions::default();
        let t = Instant::now();
        s.observe(&payload("nochain", &[]), t, &no_lock);
        assert!(s.prune(t + Duration::from_secs(60), &|_| false).is_empty());
        assert_eq!(s.prune(t + STALE_AFTER + Duration::from_secs(1), &|_| true), vec!["nochain".to_string()]);
    }

    // ── focus_target ──────────────────────────────────────────────────────────

    fn record(chain: &[(u32, &str)], ide: Option<IdeLock>, cwd: &str) -> SessionRecord {
        let s = Sessions::default();
        let mut p = payload("s", chain);
        p["cwd"] = json!(cwd);
        let has_ide = ide.is_some();
        let lock = move |_: &str| ide.clone();
        if has_ide {
            p["ide_port"] = json!("1");
        }
        s.observe(&p, Instant::now(), &lock);
        s.get("s").unwrap()
    }

    #[test]
    fn a_windows_terminal_session_targets_the_terminal_process() {
        let (c, hints) = focus_target(&record(WT_CHAIN, None, r"C:\Users\u\coucou"));
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].pid, c[0].exe.as_deref()), (60, Some("WindowsTerminal.exe")));
        assert_eq!(hints, vec!["coucou"]);
    }

    #[test]
    fn a_cursor_session_prefers_the_editor_main_process_and_hints_with_its_workspace() {
        let lock = IdeLock {
            pid: 40,
            ide_name: "Cursor".into(),
            workspace_folders: vec![r"c:\Users\u\Documents\work\shop-api".into()],
        };
        let chain = &[(90, "bash.exe"), (80, "claude.exe"), (70, "powershell.exe"), (50, "Cursor.exe"), (40, "Cursor.exe")];
        let (c, hints) = focus_target(&record(chain, Some(lock), r"C:\Users\u\Documents\work\shop-api\api"));
        let pids: Vec<u32> = c.iter().map(|x| x.pid).collect();
        assert_eq!(pids, vec![40, 50], "main process first, each pid once");
        assert!(c.iter().all(|x| x.exe.as_deref() == Some("Cursor.exe")));
        assert_eq!(hints, vec!["shop-api", "api"]);
    }

    #[test]
    fn an_ide_lock_pid_is_only_trusted_with_a_known_executable() {
        // A lock file naming some other editor's pid gives us no image to verify,
        // so it must not become a candidate.
        let lock = IdeLock { pid: 999, ide_name: "SomethingElse".into(), workspace_folders: vec![] };
        let (c, _) = focus_target(&record(WT_CHAIN, Some(lock), r"C:\proj"));
        assert!(c.iter().all(|x| x.pid != 999));
    }

    #[test]
    fn a_session_with_no_known_host_has_nothing_to_focus() {
        let (c, _) = focus_target(&record(&[(80, "claude.exe")], None, r"C:\proj"));
        assert!(c.is_empty());
    }
}
