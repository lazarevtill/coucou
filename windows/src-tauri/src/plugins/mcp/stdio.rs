// MCP over a server's standard streams: one JSON-RPC message per line.
//
// `Conn` works over any writer and any source of lines, so it is tested with
// canned lines; `spawn` starts the real server process. The server is started
// without a shell, with a small environment plus what the plugin declares; its
// stderr is drained so a chatty server cannot block on a full pipe; on Windows
// it is put in a job object that kills it — and anything it started — when
// the session ends.

use std::io::Write;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::proto::{self, CallResult, Reply, ToolInfo};

/// How long a server gets to answer the `server/discover` probe before it is
/// taken for an older one.
pub const PROBE: Duration = Duration::from_secs(3);
/// How long any other request may take.
pub const REQUEST: Duration = Duration::from_secs(20);
/// Pages of `tools/list` followed at most.
const MAX_PAGES: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Era {
    Modern,
    Legacy,
}

#[derive(Debug, PartialEq)]
pub enum Fail {
    Timeout,
    /// The server closed its output (it exited).
    Closed,
    Error(String),
}

impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fail::Timeout => write!(f, "The server did not answer in time."),
            Fail::Closed => write!(f, "The server stopped."),
            Fail::Error(e) => write!(f, "{e}"),
        }
    }
}

/// What the probe found out.
#[derive(Debug, PartialEq)]
pub enum Probe {
    Modern,
    /// An older server; `dead` when it exited on the probe and must be restarted.
    Legacy { dead: bool },
}

pub struct Conn {
    writer: Box<dyn Write + Send>,
    lines: Receiver<String>,
    next_id: u64,
    era: Era,
}

impl Conn {
    pub fn new(writer: Box<dyn Write + Send>, lines: Receiver<String>) -> Self {
        Conn { writer, lines, next_id: 1, era: Era::Modern }
    }

    fn send(&mut self, message: &Value) -> Result<(), Fail> {
        let mut line = message.to_string();
        line.push('\n');
        self.writer.write_all(line.as_bytes()).and_then(|_| self.writer.flush()).map_err(|_| Fail::Closed)
    }

    /// Waits for the reply to `id`, answering the server's own requests and
    /// skipping notifications and anything that is not JSON.
    fn await_reply(&mut self, id: u64, timeout: Duration) -> Result<Reply, Fail> {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = match self.lines.recv_timeout(left) {
                Ok(line) => line,
                Err(RecvTimeoutError::Timeout) => return Err(Fail::Timeout),
                Err(RecvTimeoutError::Disconnected) => return Err(Fail::Closed),
            };
            let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
            if let Some(reply) = proto::reply_to(&message, id) {
                return Ok(reply);
            }
            if let Some(answer) = proto::answer_server_request(&message) {
                self.send(&answer)?;
            }
        }
    }

    fn request(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Reply, Fail> {
        let id = self.next_id;
        self.next_id += 1;
        let message = match self.era {
            Era::Modern => proto::modern_request(id, method, params),
            Era::Legacy => proto::legacy_request(id, method, params),
        };
        self.send(&message)?;
        self.await_reply(id, timeout)
    }

    /// Asks `server/discover`. A current server answers it (or refuses our
    /// version with its own error); anything else means an older server.
    pub fn probe(&mut self, timeout: Duration) -> Result<Probe, Fail> {
        self.era = Era::Modern;
        let speaks = |versions: Option<&Value>| -> Vec<String> {
            versions.and_then(Value::as_array).map(|v| v.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
        };
        match self.request("server/discover", json!({}), timeout) {
            Ok(Reply::Result(result)) => {
                let versions = speaks(result.get("supportedVersions"));
                if versions.is_empty() || versions.iter().any(|v| v == proto::MODERN) {
                    Ok(Probe::Modern)
                } else {
                    Err(Fail::Error(format!("The server speaks {}, not {}.", versions.join(", "), proto::MODERN)))
                }
            }
            Ok(reply @ Reply::Error { .. }) if proto::is_modern_error(&reply) => {
                let Reply::Error { data, message, .. } = reply else { unreachable!() };
                let versions = speaks(data.as_ref().and_then(|d| d.get("supported")));
                Err(Fail::Error(if versions.is_empty() {
                    format!("The server refused the request: {message}")
                } else {
                    format!("The server speaks {}, not {}.", versions.join(", "), proto::MODERN)
                }))
            }
            Ok(Reply::Error { .. }) | Err(Fail::Timeout) => Ok(Probe::Legacy { dead: false }),
            Err(Fail::Closed) => Ok(Probe::Legacy { dead: true }),
            Err(other) => Err(other),
        }
    }

    /// The handshake older servers need before anything else.
    pub fn start_legacy(&mut self) -> Result<(), Fail> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&proto::initialize(id))?;
        let reply = self.await_reply(id, REQUEST)?;
        proto::complete(reply).map_err(Fail::Error)?;
        self.send(&proto::initialized())?;
        self.era = Era::Legacy;
        Ok(())
    }

    pub fn list_tools(&mut self) -> Result<Vec<ToolInfo>, Fail> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let params = match &cursor {
                Some(c) => json!({ "cursor": c }),
                None => json!({}),
            };
            let result = proto::complete(self.request("tools/list", params, REQUEST)?).map_err(Fail::Error)?;
            let (page, next) = proto::tools_page(&result);
            tools.extend(page);
            match next {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        Ok(tools)
    }

    pub fn call_tool(&mut self, name: &str, arguments: &Value) -> Result<CallResult, Fail> {
        let reply = self.request("tools/call", json!({ "name": name, "arguments": arguments }), REQUEST)?;
        Ok(proto::call_result(&proto::complete(reply).map_err(Fail::Error)?))
    }
}

// ── The server process ────────────────────────────────────────────────────────

/// Lines of the server's stderr kept to explain a failure.
const STDERR_TAIL: usize = 20;

pub struct Server {
    child: std::process::Child,
    _job: Option<crate::platform::ProcessJob>,
    stderr: std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<String>>>,
}

impl Server {
    /// The last thing the server wrote to stderr, for an error message.
    pub fn last_words(&self) -> String {
        self.stderr.lock().unwrap().back().cloned().unwrap_or_default()
    }

    #[cfg(test)]
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Its stdin is already closed (the connection is gone): wait a moment for
    /// it to exit on its own, then end it.
    fn finish(&mut self) {
        for _ in 0..20 {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Starts a server: no shell, the small inherited environment plus `env`.
pub fn spawn(command: &str, args: &[String], env: &[(String, String)]) -> Result<(Server, Conn), String> {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};

    let mut cmd = Command::new(command);
    cmd.args(args).env_clear();
    for name in crate::platform::INHERITED_ENV {
        if let Some(value) = std::env::var_os(name) {
            cmd.env(name, value);
        }
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    crate::platform::no_console(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| format!("The server could not be started: {e}"))?;
    let job = crate::platform::contain(&child);

    let stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let tail = std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new()));
    if let Some(stderr) = child.stderr.take() {
        let tail = tail.clone();
        // Drained to the end, whatever the server writes: a full pipe would stop it.
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                let mut t = tail.lock().unwrap();
                t.push_back(line.chars().take(300).collect());
                if t.len() > STDERR_TAIL {
                    t.pop_front();
                }
            }
        });
    }
    Ok((Server { child, _job: job, stderr: tail }, Conn::new(Box::new(stdin), rx)))
}

/// A server, started and spoken to in whichever revision it understands.
pub struct Session {
    conn: Option<Conn>,
    server: Option<Server>,
}

impl Session {
    pub fn open(command: &str, args: &[String], env: &[(String, String)]) -> Result<Session, String> {
        let (server, mut conn) = spawn(command, args, env)?;
        let mut session = Session { conn: None, server: Some(server) };
        match conn.probe(PROBE) {
            Ok(Probe::Modern) => {}
            Ok(Probe::Legacy { dead }) => {
                if dead {
                    // Some older servers exit on a request they did not expect.
                    drop(session);
                    let (server, fresh) = spawn(command, args, env)?;
                    session = Session { conn: None, server: Some(server) };
                    conn = fresh;
                }
                conn.start_legacy().map_err(|e| session.explain(e))?;
            }
            Err(e) => return Err(session.explain(e)),
        }
        session.conn = Some(conn);
        Ok(session)
    }

    pub fn conn(&mut self) -> &mut Conn {
        self.conn.as_mut().expect("an open session has a connection")
    }

    #[cfg(test)]
    pub fn pid(&self) -> Option<u32> {
        self.server.as_ref().map(Server::pid)
    }

    /// A failure, with what the server last said about it.
    pub fn explain(&self, fail: Fail) -> String {
        let words = self.server.as_ref().map(Server::last_words).unwrap_or_default();
        if words.is_empty() {
            fail.to_string()
        } else {
            format!("{fail} The server said: {words}")
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        drop(self.conn.take()); // closes the server's stdin: the polite way to say goodbye
        if let Some(mut server) = self.server.take() {
            server.finish();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::sync::{Arc, Mutex};

    // ── Against a real server process ─────────────────────────────────────────

    /// Node on PATH, as an absolute path. These tests need it and say so.
    fn node() -> String {
        let found = crate::platform::find_all_on_path("node");
        found.into_iter().next().map(|p| p.to_string_lossy().to_string()).expect("these tests need Node.js on PATH")
    }

    fn fixture() -> String {
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/mcp-test-server.mjs").to_string()
    }

    #[test]
    fn a_real_current_server_lists_and_answers_and_gets_only_its_own_environment() {
        // A value Coucou has that the server must not see.
        unsafe { std::env::set_var("SECRET_PARENT_VALUE", "do-not-leak") };
        let mut s = Session::open(&node(), &[fixture(), "modern".into()], &[("TICKETS_KEY".into(), "k-123".into())]).unwrap();
        let tools = s.conn().list_tools().unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].read_only_hint, Some(true));
        let result = s.conn().call_tool("open_tickets", &json!({ "limit": 2 })).unwrap();
        let data = result.structured.unwrap();
        assert_eq!(data["tickets"].as_array().unwrap().len(), 2);
        assert_eq!(data["key"], "k-123", "the declared secret reached the server");
        assert_eq!(data["home"], Value::Null, "nothing else of Coucou's environment did");
    }

    #[test]
    fn a_real_older_server_that_floods_stderr_and_pings_still_answers() {
        let mut s = Session::open(&node(), &[fixture(), "legacy".into()], &[]).unwrap();
        let result = s.conn().call_tool("open_tickets", &json!({})).unwrap();
        assert!(result.text.contains("open tickets"), "{}", result.text);
    }

    #[test]
    fn a_real_server_that_dies_on_the_probe_is_restarted_and_initialized() {
        let mut s = Session::open(&node(), &[fixture(), "strict".into()], &[]).unwrap();
        assert_eq!(s.conn().list_tools().unwrap().len(), 2);
    }

    #[test]
    fn the_server_is_gone_when_the_session_ends() {
        let s = Session::open(&node(), &[fixture(), "modern".into()], &[]).unwrap();
        let pid = s.pid().unwrap();
        let exe = std::path::Path::new(&node()).file_name().unwrap().to_string_lossy().to_string();
        assert!(crate::platform::process_alive(pid, &exe));
        drop(s);
        assert!(!crate::platform::process_alive(pid, &exe), "the server must not outlive its session");
    }

    #[test]
    fn a_server_that_cannot_start_says_why() {
        let missing = if cfg!(windows) { r"C:\nowhere\missing.exe" } else { "/nowhere/missing" };
        let err = Session::open(missing, &[], &[]).err().unwrap();
        assert!(err.contains("could not be started"), "{err}");
    }

    /// A writer the test can read back.
    #[derive(Clone, Default)]
    struct Sink(Arc<Mutex<Vec<u8>>>);
    impl Write for Sink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl Sink {
        fn sent(&self) -> Vec<Value> {
            String::from_utf8(self.0.lock().unwrap().clone())
                .unwrap()
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect()
        }
    }

    /// A connection whose server says `lines`, in order.
    fn conn(lines: &[Value]) -> (Conn, Sink) {
        let (tx, rx) = channel();
        for l in lines {
            tx.send(l.to_string()).unwrap();
        }
        // Keep the sender alive so an empty queue is a timeout, not a closed server.
        std::mem::forget(tx);
        let sink = Sink::default();
        (Conn::new(Box::new(sink.clone()), rx), sink)
    }

    const SHORT: Duration = Duration::from_millis(200);

    #[test]
    fn a_current_server_answers_the_probe_and_lists_its_tools() {
        let (mut c, sink) = conn(&[
            json!({ "jsonrpc": "2.0", "id": 1, "result": { "resultType": "complete", "supportedVersions": ["2026-07-28"], "capabilities": { "tools": {} } } }),
            json!({ "jsonrpc": "2.0", "id": 2, "result": { "tools": [{ "name": "status", "inputSchema": {} }], "nextCursor": "p2" } }),
            json!({ "jsonrpc": "2.0", "id": 3, "result": { "tools": [{ "name": "close", "inputSchema": {} }] } }),
        ]);
        assert_eq!(c.probe(SHORT).unwrap(), Probe::Modern);
        let tools = c.list_tools().unwrap();
        assert_eq!(tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), vec!["status", "close"], "pages are followed");
        let sent = sink.sent();
        assert_eq!(sent[0]["method"], "server/discover");
        assert_eq!(sent[1]["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"], "2026-07-28");
        assert_eq!(sent[2]["params"]["cursor"], "p2");
    }

    #[test]
    fn a_server_that_does_not_know_discover_is_an_older_one() {
        let (mut c, _) = conn(&[json!({ "jsonrpc": "2.0", "id": 1, "error": { "code": -32601, "message": "Method not found" } })]);
        assert_eq!(c.probe(SHORT).unwrap(), Probe::Legacy { dead: false });
        let (mut silent, _) = conn(&[]);
        assert_eq!(silent.probe(SHORT).unwrap(), Probe::Legacy { dead: false }, "no answer in time");
    }

    #[test]
    fn a_server_that_dies_on_the_probe_must_be_restarted() {
        let (tx, rx) = channel::<String>();
        drop(tx);
        let mut c = Conn::new(Box::new(Sink::default()), rx);
        assert_eq!(c.probe(SHORT).unwrap(), Probe::Legacy { dead: true });
    }

    #[test]
    fn a_current_server_that_refuses_the_version_is_not_mistaken_for_an_older_one() {
        let (mut c, _) = conn(&[json!({ "jsonrpc": "2.0", "id": 1, "error": { "code": -32022, "message": "Unsupported protocol version", "data": { "supported": ["2027-01-01"], "requested": "2026-07-28" } } })]);
        let err = c.probe(SHORT).unwrap_err().to_string();
        assert!(err.contains("2027-01-01"), "{err}");
    }

    #[test]
    fn an_older_server_is_initialized_and_its_own_requests_answered() {
        let (mut c, sink) = conn(&[
            json!({ "jsonrpc": "2.0", "id": 1, "result": { "protocolVersion": "2025-11-25", "capabilities": { "tools": {} }, "serverInfo": { "name": "old" } } }),
            json!({ "jsonrpc": "2.0", "id": 77, "method": "ping" }),
            json!({ "jsonrpc": "2.0", "method": "notifications/message", "params": { "level": "info", "data": "hi" } }),
            json!({ "jsonrpc": "2.0", "id": 2, "result": { "content": [{ "type": "text", "text": "{\"open\":4}" }] } }),
        ]);
        c.start_legacy().unwrap();
        let result = c.call_tool("open_tickets", &json!({ "limit": 5 })).unwrap();
        assert_eq!(result.text, "{\"open\":4}");
        let sent = sink.sent();
        assert_eq!(sent[0]["method"], "initialize");
        assert_eq!(sent[1]["method"], "notifications/initialized");
        assert_eq!(sent[2]["method"], "tools/call");
        assert!(sent[2]["params"].get("_meta").is_none(), "no per-request metadata for an older server");
        assert_eq!(sent[2]["params"]["arguments"], json!({ "limit": 5 }));
        assert_eq!(sent[3], json!({ "jsonrpc": "2.0", "id": 77, "result": {} }), "the server's ping was answered");
    }

    #[test]
    fn a_tool_that_asks_for_input_is_an_error_for_a_poll() {
        let (mut c, _) = conn(&[
            json!({ "jsonrpc": "2.0", "id": 1, "result": { "supportedVersions": ["2026-07-28"] } }),
            json!({ "jsonrpc": "2.0", "id": 2, "result": { "resultType": "input_required", "inputRequests": {} } }),
        ]);
        c.probe(SHORT).unwrap();
        let err = c.call_tool("status", &json!({})).unwrap_err().to_string();
        assert!(err.contains("input"), "{err}");
    }

    #[test]
    fn noise_on_the_output_is_skipped() {
        let (tx, rx) = channel();
        tx.send("Server starting...".to_string()).unwrap();
        tx.send(json!({ "jsonrpc": "2.0", "id": 1, "result": { "supportedVersions": ["2026-07-28"] } }).to_string()).unwrap();
        std::mem::forget(tx);
        let mut c = Conn::new(Box::new(Sink::default()), rx);
        assert_eq!(c.probe(SHORT).unwrap(), Probe::Modern);
    }
}
