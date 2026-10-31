// Exercise the real executable through both current and legacy MCP transports.
// All data/config paths point at a synthetic temporary profile, never user notes.
use std::{fs, io::{BufRead, BufReader, Write}, path::PathBuf,
    process::{Child, Command, Stdio}, sync::mpsc::{self, Receiver}, time::{Duration, Instant}};
use serde_json::{json, Value};

struct Client { child: Child, replies: Receiver<Value>, root: PathBuf, access: PathBuf, modern: bool, id: u32 }
impl Client {
    fn start(modern: bool) -> Self {
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("coucou-mcp-protocol-{}-{unique}", std::process::id()));
        #[cfg(windows)]
        let (notes, access) = (root.join("local/Coucou/Notes"), root.join("config/Coucou/mcp-access.json"));
        #[cfg(not(windows))]
        let (notes, access) = (root.join("local/coucou/Notes"), root.join("config/coucou/mcp-access.json"));
        fs::create_dir_all(notes.join("fixture")).unwrap();
        fs::create_dir_all(access.parent().unwrap()).unwrap();
        fs::write(notes.join("fixture/note.json"), json!({
            "id":"fixture","title":"MCP launch review","start":1234,"status":"ready",
            "lines":[{"id":"l","speaker":"me","start":0.0,"end":2.0,"text":"The release owner is Mina. The deadline is Friday."}]
        }).to_string()).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_coucou")).arg("mcp")
            .env("APPDATA", root.join("config")).env("LOCALAPPDATA", root.join("local"))
            .env("XDG_CONFIG_HOME", root.join("config")).env("XDG_DATA_HOME", root.join("local"))
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, replies) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break; };
                // Any non-protocol stdout is a test failure, not ignored noise.
                let value = serde_json::from_str(&line).unwrap_or_else(|_| json!({"invalidStdout":line}));
                if tx.send(value).is_err() { break; }
            }
        });
        Self { child, replies, root, access, modern, id: 0 }
    }
    fn request(&mut self, method: &str, mut params: Value) -> Value {
        self.id += 1;
        if self.modern {
            params["_meta"] = json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28",
                "io.modelcontextprotocol/clientInfo":{"name":"coucou-test","version":"1"},
                "io.modelcontextprotocol/clientCapabilities":{}});
        }
        writeln!(self.child.stdin.as_mut().unwrap(), "{}", json!({"jsonrpc":"2.0","id":self.id,"method":method,"params":params})).unwrap();
        let reply = self.replies.recv_timeout(Duration::from_secs(15)).expect("MCP must respond without launching the GUI or waiting for user input");
        assert_eq!(reply["id"], self.id, "{reply}");
        reply
    }
    fn finish(&mut self) {
        self.child.stdin.take();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() { assert!(status.success()); break; }
            assert!(Instant::now() < deadline, "Server must exit when the client closes stdin");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn exercise(modern: bool) {
    let mut client = Client::start(modern);
    if modern {
        let reply = client.request("server/discover", json!({}));
        assert!(reply["error"].is_null(), "{reply}");
    } else {
        let reply = client.request("initialize", json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"coucou-test","version":"1"}}));
        assert_eq!(reply["result"]["protocolVersion"], "2025-11-25");
        writeln!(client.child.stdin.as_mut().unwrap(), "{}", json!({"jsonrpc":"2.0","method":"notifications/initialized"})).unwrap();
    }
    let reply = client.request("tools/list", json!({}));
    let tools = reply["result"]["tools"].as_array().expect("two read-only tools");
    assert_eq!(tools.len(), 2);
    for tool in tools { assert_eq!(tool["annotations"]["readOnlyHint"], true); }
    let denied = client.request("tools/call", json!({"name":"search","arguments":{"query":""}}));
    assert_eq!(denied["result"]["isError"], true);
    assert!(!denied.to_string().contains("Mina"));
    fs::write(&client.access, "{\"enabled\":true}").unwrap();
    let found = client.request("tools/call", json!({"name":"search","arguments":{"query":"release owner"}}));
    assert_ne!(found["result"]["isError"], true, "{found}");
    let found: Value = serde_json::from_str(found["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(found["results"][0]["id"], "fixture");
    let fetched = client.request("tools/call", json!({"name":"fetch","arguments":{"id":"fixture"}}));
    assert!(fetched.to_string().contains("Mina"));
    assert!(fetched.to_string().contains("Friday"));
    fs::write(&client.access, "{\"enabled\":false}").unwrap();
    let denied = client.request("tools/call", json!({"name":"fetch","arguments":{"id":"fixture"}}));
    assert_eq!(denied["result"]["isError"], true);
    assert!(!denied.to_string().contains("Mina"));
    let unknown = client.request("tools/call", json!({"name":"delete","arguments":{"id":"fixture"}}));
    assert!(unknown["error"].is_object());
    // Reading calls must not initialize the full application's files or log.
    assert!(!client.access.parent().unwrap().join("settings.json").exists());
    client.finish();
}

#[test]
fn legacy_stdio_clients_read_calls_only_while_enabled() { exercise(false); }
#[test]
fn current_stdio_clients_read_calls_only_while_enabled() { exercise(true); }
