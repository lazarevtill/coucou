// MCP over streamable HTTP, through the plugin's guarded client.
//
// The first request is sent in the current revision. A success, or one of the
// current revision's own errors, means a current server; any other 4xx means an
// older one, which gets the initialize handshake and its session id on every
// request after that. Replies come as one JSON object or as an event stream.

use serde_json::{json, Value};

use super::proto::{self, CallResult, Reply, ToolInfo};
use super::stdio::Era;
use crate::plugins::guard::{self, Guarded};

const MAX_PAGES: usize = 10;

pub struct HttpSession<'a> {
    g: &'a Guarded,
    url: String,
    headers: Vec<(String, String)>,
    era: Option<Era>,
    session_id: Option<String>,
    next_id: u64,
}

impl<'a> HttpSession<'a> {
    pub fn new(g: &'a Guarded, url: &str, headers: Vec<(String, String)>) -> Self {
        HttpSession { g, url: url.to_string(), headers, era: None, session_id: None, next_id: 1 }
    }

    fn next(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// One POST. Returns the status and the JSON-RPC messages in the answer.
    async fn post(&mut self, message: &Value, method: &str, name: Option<&str>) -> Result<(u16, Vec<Value>), String> {
        let mut headers = self.headers.clone();
        headers.push(("Content-Type".into(), "application/json".into()));
        headers.push(("Accept".into(), "application/json, text/event-stream".into()));
        match self.era {
            Some(Era::Legacy) => {
                headers.push(("MCP-Protocol-Version".into(), proto::LEGACY.into()));
                if let Some(id) = &self.session_id {
                    headers.push(("Mcp-Session-Id".into(), id.clone()));
                }
            }
            _ => {
                headers.push(("MCP-Protocol-Version".into(), proto::MODERN.into()));
                headers.push(("Mcp-Method".into(), method.into()));
                if let Some(name) = name {
                    headers.push(("Mcp-Name".into(), proto::header_value(name)));
                }
            }
        }
        let body = serde_json::to_vec(message).map_err(|e| e.to_string())?;
        let (status, response_headers, bytes) = guard::post(self.g, &self.url, &headers, body).await?;
        let header = |name: &str| response_headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
        if let Some(id) = header("mcp-session-id") {
            self.session_id = Some(id);
        }
        let text = String::from_utf8_lossy(&bytes);
        let messages = if header("content-type").is_some_and(|c| c.starts_with("text/event-stream")) {
            proto::sse_messages(&text)
        } else {
            serde_json::from_str::<Value>(&text).map(|v| vec![v]).unwrap_or_default()
        };
        Ok((status, messages))
    }

    /// Sends a request and returns its reply, answering any request the server
    /// sent alongside (older servers may ping).
    async fn exchange(&mut self, id: u64, message: Value, method: &str, name: Option<&str>) -> Result<(u16, Option<Reply>), String> {
        let (status, messages) = self.post(&message, method, name).await?;
        for m in &messages {
            if let Some(answer) = proto::answer_server_request(m) {
                let _ = self.post(&answer, "", None).await;
            }
        }
        Ok((status, messages.iter().find_map(|m| proto::reply_to(m, id))))
    }

    async fn request(&mut self, method: &str, params: Value, name: Option<&str>) -> Result<Reply, String> {
        if self.era.is_none() {
            // First contact: the current revision, and see what comes back.
            let id = self.next();
            let (status, reply) = self.exchange(id, proto::modern_request(id, method, params.clone()), method, name).await?;
            let modern = (200..300).contains(&status)
                || reply.as_ref().is_some_and(|r| {
                    proto::is_modern_error(r) || (status == 404 && matches!(r, Reply::Error { code, .. } if *code == proto::METHOD_NOT_FOUND))
                });
            if modern {
                self.era = Some(Era::Modern);
                return reply.ok_or_else(|| format!("HTTP {status} without an answer."));
            }
            if !(400..500).contains(&status) {
                return Err(format!("HTTP {status}"));
            }
            self.start_legacy().await?;
        }
        let id = self.next();
        let message = match self.era {
            Some(Era::Legacy) => proto::legacy_request(id, method, params),
            _ => proto::modern_request(id, method, params),
        };
        let (status, reply) = self.exchange(id, message, method, name).await?;
        reply.ok_or_else(|| format!("HTTP {status} without an answer."))
    }

    async fn start_legacy(&mut self) -> Result<(), String> {
        self.era = Some(Era::Legacy);
        let id = self.next();
        let (status, reply) = self.exchange(id, proto::initialize(id), "initialize", None).await?;
        proto::complete(reply.ok_or_else(|| format!("HTTP {status}: the server did not answer initialize."))?)?;
        let (status, _) = self.post(&proto::initialized(), "notifications/initialized", None).await?;
        if !(200..300).contains(&status) {
            return Err(format!("HTTP {status} to notifications/initialized."));
        }
        Ok(())
    }

    pub async fn list_tools(&mut self) -> Result<Vec<ToolInfo>, String> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let params = match &cursor {
                Some(c) => json!({ "cursor": c }),
                None => json!({}),
            };
            let result = proto::complete(self.request("tools/list", params, None).await?)?;
            let (page, next) = proto::tools_page(&result);
            tools.extend(page);
            match next {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        Ok(tools)
    }

    pub async fn call_tool(&mut self, name: &str, arguments: &Value) -> Result<CallResult, String> {
        let reply = self.request("tools/call", json!({ "name": name, "arguments": arguments }), Some(name)).await?;
        Ok(proto::call_result(&proto::complete(reply)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::{guard, manifest};
    use crate::test_http::{serve, Canned};

    fn run<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
    }

    fn guarded(origin: &str) -> Guarded {
        let v = json!({
            "schema": 1, "id": "remote", "name": "Remote", "version": "1", "kind": "mcp",
            "allowedHosts": ["127.0.0.1"], "allowPrivateNetwork": true,
            "mcp": { "transport": "http", "url": format!("{origin}/mcp"), "tools": {} }
        });
        let m = manifest::parse(&serde_json::to_vec(&v).unwrap()).unwrap();
        run(guard::client(&m)).unwrap()
    }

    fn sse(body: &str) -> Canned {
        Canned { status: 200, headers: vec![("Content-Type", "text/event-stream".into())], body: body.as_bytes().to_vec() }
    }

    fn body_of(request: &str) -> Value {
        serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap_or("")).unwrap_or(Value::Null)
    }

    #[test]
    fn a_current_server_answers_in_json_with_the_headers_the_spec_asks_for() {
        let (origin, seen) = serve(vec![Canned::json(200, r#"{"jsonrpc":"2.0","id":1,"result":{"resultType":"complete","tools":[{"name":"status","inputSchema":{}}]}}"#)]);
        let g = guarded(&origin);
        let mut s = HttpSession::new(&g, &format!("{origin}/mcp"), vec![("Authorization".into(), "Bearer t".into())]);
        let tools = run(s.list_tools()).unwrap();
        assert_eq!(tools[0].name, "status");
        let req = seen.lock().unwrap()[0].clone();
        let lower = req.to_lowercase();
        assert!(lower.starts_with("post /mcp "));
        assert!(lower.contains("mcp-protocol-version: 2026-07-28"));
        assert!(lower.contains("mcp-method: tools/list"));
        assert!(lower.contains("accept: application/json, text/event-stream"));
        assert!(lower.contains("authorization: bearer t"));
        assert_eq!(body_of(&req)["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"], "2026-07-28");
    }

    #[test]
    fn a_tool_call_answered_as_an_event_stream_is_read() {
        let (origin, seen) = serve(vec![sse(
            ": hello\n\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\",\"params\":{\"progress\":1}}\n\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"3 open\"}]}}\n\n",
        )]);
        let g = guarded(&origin);
        let mut s = HttpSession::new(&g, &format!("{origin}/mcp"), vec![]);
        let result = run(s.call_tool("open_tickets", &json!({}))).unwrap();
        assert_eq!(result.text, "3 open");
        assert!(seen.lock().unwrap()[0].to_lowercase().contains("mcp-name: open_tickets"));
    }

    #[test]
    fn an_older_server_gets_the_handshake_and_its_session_id() {
        let (origin, seen) = serve(vec![
            Canned { status: 400, headers: vec![("Content-Type", "text/plain".into())], body: b"Bad Request: No valid session ID provided".to_vec() },
            Canned {
                status: 200,
                headers: vec![("Content-Type", "application/json".into()), ("Mcp-Session-Id", "s-1".into())],
                body: br#"{"jsonrpc":"2.0","id":2,"result":{"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"old"}}}"#.to_vec(),
            },
            Canned { status: 202, headers: vec![], body: vec![] },
            Canned::json(200, r#"{"jsonrpc":"2.0","id":3,"result":{"tools":[{"name":"legacy_tool","inputSchema":{}}]}}"#),
        ]);
        let g = guarded(&origin);
        let mut s = HttpSession::new(&g, &format!("{origin}/mcp"), vec![]);
        let tools = run(s.list_tools()).unwrap();
        assert_eq!(tools[0].name, "legacy_tool");
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 4);
        assert_eq!(body_of(&seen[1])["method"], "initialize");
        assert_eq!(body_of(&seen[2])["method"], "notifications/initialized");
        assert!(seen[2].to_lowercase().contains("mcp-session-id: s-1"));
        let last = seen[3].to_lowercase();
        assert!(last.contains("mcp-session-id: s-1"));
        assert!(last.contains("mcp-protocol-version: 2025-11-25"));
        assert!(body_of(&seen[3])["params"].get("_meta").is_none());
    }

    #[test]
    fn a_current_server_refusing_the_version_is_not_taken_for_an_older_one() {
        let (origin, seen) = serve(vec![Canned::json(400, r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32022,"message":"Unsupported protocol version","data":{"supported":["2027-01-01"],"requested":"2026-07-28"}}}"#)]);
        let g = guarded(&origin);
        let mut s = HttpSession::new(&g, &format!("{origin}/mcp"), vec![]);
        let err = run(s.list_tools()).unwrap_err();
        assert!(err.contains("-32022") || err.contains("Unsupported"), "{err}");
        assert_eq!(seen.lock().unwrap().len(), 1, "no handshake attempted");
    }
}
