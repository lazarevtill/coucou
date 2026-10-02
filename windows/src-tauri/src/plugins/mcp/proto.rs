// MCP messages, without any transport: building requests, reading replies, and
// telling a current server from an older one. Shapes follow the 2026-07-28
// specification (modelcontextprotocol.io/specification/2026-07-28).

use serde_json::{json, Value};

/// The revision spoken first.
pub const MODERN: &str = "2026-07-28";
/// The initialize-based revision older servers speak.
pub const LEGACY: &str = "2025-11-25";

/// Error codes a current server answers with (and an older one never does).
pub const HEADER_MISMATCH: i64 = -32020;
pub const MISSING_CAPABILITY: i64 = -32021;
pub const UNSUPPORTED_VERSION: i64 = -32022;
pub const METHOD_NOT_FOUND: i64 = -32601;

#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Result(Value),
    Error { code: i64, message: String, data: Option<Value> },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    /// What the server says — a hint only: annotations are untrusted.
    pub read_only_hint: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CallResult {
    pub text: String,
    pub structured: Option<Value>,
    pub is_error: bool,
}

pub fn client_info() -> Value {
    json!({ "name": "Coucou", "version": env!("CARGO_PKG_VERSION") })
}

/// A request in the current revision: the protocol version, who we are and what
/// we can do travel in `_meta` on every request.
pub fn modern_request(id: u64, method: &str, params: Value) -> Value {
    let mut params = match params {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    params.insert(
        "_meta".into(),
        json!({
            "io.modelcontextprotocol/protocolVersion": MODERN,
            "io.modelcontextprotocol/clientInfo": client_info(),
            "io.modelcontextprotocol/clientCapabilities": {},
        }),
    );
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

/// A request to a server that went through `initialize`: no per-request `_meta`.
pub fn legacy_request(id: u64, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

pub fn initialize(id: u64) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "initialize",
        "params": { "protocolVersion": LEGACY, "capabilities": {}, "clientInfo": client_info() },
    })
}

pub fn initialized() -> Value {
    json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })
}

fn same_id(value: Option<&Value>, id: u64) -> bool {
    match value {
        Some(Value::Number(n)) => n.as_u64() == Some(id),
        Some(Value::String(s)) => s == &id.to_string(),
        _ => false,
    }
}

/// The reply to request `id` in a message, if that is what it is.
pub fn reply_to(message: &Value, id: u64) -> Option<Reply> {
    if message.get("method").is_some() || !same_id(message.get("id"), id) {
        return None;
    }
    if let Some(result) = message.get("result") {
        return Some(Reply::Result(result.clone()));
    }
    let error = message.get("error")?;
    Some(Reply::Error {
        code: error.get("code").and_then(Value::as_i64).unwrap_or(0),
        message: error.get("message").and_then(Value::as_str).unwrap_or("").to_string(),
        data: error.get("data").cloned(),
    })
}

/// An error only a current server sends: the server speaks the current revision.
pub fn is_modern_error(reply: &Reply) -> bool {
    matches!(reply, Reply::Error { code, .. } if [HEADER_MISMATCH, MISSING_CAPABILITY, UNSUPPORTED_VERSION].contains(code))
}

/// A request an older server sends the client: ping gets an empty result,
/// anything else (sampling, roots, elicitation) that it is not supported.
pub fn answer_server_request(message: &Value) -> Option<Value> {
    let method = message.get("method")?.as_str()?;
    let id = message.get("id")?.clone();
    Some(if method == "ping" {
        json!({ "jsonrpc": "2.0", "id": id, "result": {} })
    } else {
        json!({ "jsonrpc": "2.0", "id": id, "error": { "code": METHOD_NOT_FOUND, "message": format!("Coucou does not offer {method}") } })
    })
}

/// A finished result, or why it is not one.
pub fn complete(reply: Reply) -> Result<Value, String> {
    match reply {
        Reply::Error { code, message, .. } => Err(format!("The server answered with an error ({code}): {message}")),
        Reply::Result(result) => match result.get("resultType").and_then(Value::as_str) {
            None | Some("complete") => Ok(result),
            Some("input_required") => Err("The server asked for input; a plugin's poll cannot give any.".into()),
            Some(other) => Err(format!("The server answered with an unknown result type \"{other}\".")),
        },
    }
}

pub fn tools_page(result: &Value) -> (Vec<ToolInfo>, Option<String>) {
    let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let tools = result
        .get("tools")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|t| {
                    Some(ToolInfo {
                        name: text(t, "name")?,
                        title: text(t, "title"),
                        description: text(t, "description"),
                        read_only_hint: t.get("annotations").and_then(|a| a.get("readOnlyHint")).and_then(Value::as_bool),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    (tools, text(result, "nextCursor").filter(|c| !c.is_empty()))
}

pub fn call_result(result: &Value) -> CallResult {
    let text = result
        .get("content")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|c| c.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|c| c.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    CallResult {
        text,
        structured: result.get("structuredContent").cloned(),
        is_error: result.get("isError").and_then(Value::as_bool).unwrap_or(false),
    }
}

/// The JSON-RPC messages in a `text/event-stream` body.
pub fn sse_messages(body: &str) -> Vec<Value> {
    let body = body.replace("\r\n", "\n");
    body.split("\n\n")
        .filter_map(|event| {
            let data: Vec<&str> = event
                .lines()
                .filter(|l| !l.starts_with(':'))
                .filter_map(|l| l.strip_prefix("data:"))
                .map(|d| d.strip_prefix(' ').unwrap_or(d))
                .collect();
            if data.is_empty() {
                return None;
            }
            serde_json::from_str(&data.join("\n")).ok()
        })
        .collect()
}

/// A value for `Mcp-Name`: as-is when it is plain ASCII, else the base64 sentinel.
pub fn header_value(value: &str) -> String {
    let plain = !value.is_empty()
        && value.chars().all(|c| c == ' ' || c == '\t' || ('\x21'..='\x7e').contains(&c))
        && !value.starts_with([' ', '\t'])
        && !value.ends_with([' ', '\t'])
        && !(value.starts_with("=?base64?") && value.ends_with("?="));
    if plain {
        value.to_string()
    } else {
        format!("=?base64?{}?=", crate::claude::base64_for(value.as_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_modern_request_carries_its_version_and_capabilities() {
        let r = modern_request(7, "tools/call", json!({ "name": "status", "arguments": {} }));
        assert_eq!(r["jsonrpc"], "2.0");
        assert_eq!(r["id"], 7);
        assert_eq!(r["method"], "tools/call");
        assert_eq!(r["params"]["name"], "status");
        let meta = &r["params"]["_meta"];
        assert_eq!(meta["io.modelcontextprotocol/protocolVersion"], MODERN);
        assert_eq!(meta["io.modelcontextprotocol/clientCapabilities"], json!({}));
        assert_eq!(meta["io.modelcontextprotocol/clientInfo"]["name"], "Coucou");
    }

    #[test]
    fn legacy_messages_follow_the_initialize_handshake() {
        let init = initialize(1);
        assert_eq!(init["method"], "initialize");
        assert_eq!(init["params"]["protocolVersion"], LEGACY);
        assert_eq!(init["params"]["capabilities"], json!({}));
        assert_eq!(init["params"]["clientInfo"]["name"], "Coucou");
        let note = initialized();
        assert_eq!(note["method"], "notifications/initialized");
        assert!(note.get("id").is_none(), "a notification has no id");
        let r = legacy_request(2, "tools/list", json!({}));
        assert!(r["params"].get("_meta").is_none());
    }

    #[test]
    fn replies_are_matched_by_id_and_errors_read() {
        let ok = json!({ "jsonrpc": "2.0", "id": 3, "result": { "tools": [] } });
        assert_eq!(reply_to(&ok, 3), Some(Reply::Result(json!({ "tools": [] }))));
        assert_eq!(reply_to(&ok, 4), None);
        let err = json!({ "jsonrpc": "2.0", "id": "3", "error": { "code": -32602, "message": "bad" } });
        assert!(matches!(reply_to(&err, 3), Some(Reply::Error { code: -32602, .. })), "string ids match too");
        assert_eq!(reply_to(&json!({ "jsonrpc": "2.0", "method": "notifications/progress" }), 3), None);
    }

    #[test]
    fn only_the_current_revisions_own_errors_mark_a_modern_server() {
        let e = |code| Reply::Error { code, message: String::new(), data: None };
        assert!(is_modern_error(&e(UNSUPPORTED_VERSION)));
        assert!(is_modern_error(&e(HEADER_MISMATCH)));
        assert!(is_modern_error(&e(MISSING_CAPABILITY)));
        // What an older server says to a request it did not expect before initialize.
        assert!(!is_modern_error(&e(METHOD_NOT_FOUND)));
        assert!(!is_modern_error(&e(-32602)));
        assert!(!is_modern_error(&Reply::Result(json!({}))));
    }

    #[test]
    fn requests_from_an_older_server_are_answered_not_ignored() {
        let ping = json!({ "jsonrpc": "2.0", "id": 9, "method": "ping" });
        assert_eq!(answer_server_request(&ping), Some(json!({ "jsonrpc": "2.0", "id": 9, "result": {} })));
        let sampling = json!({ "jsonrpc": "2.0", "id": "s1", "method": "sampling/createMessage", "params": {} });
        let answer = answer_server_request(&sampling).unwrap();
        assert_eq!(answer["id"], "s1");
        assert_eq!(answer["error"]["code"], METHOD_NOT_FOUND);
        assert_eq!(answer_server_request(&json!({ "jsonrpc": "2.0", "method": "notifications/message" })), None, "notifications need no answer");
        assert_eq!(answer_server_request(&json!({ "jsonrpc": "2.0", "id": 1, "result": {} })), None, "a reply is not a request");
    }

    #[test]
    fn a_result_is_complete_unless_it_asks_for_input() {
        assert_eq!(complete(Reply::Result(json!({ "resultType": "complete", "x": 1 }))).unwrap()["x"], 1);
        assert_eq!(complete(Reply::Result(json!({ "x": 2 }))).unwrap()["x"], 2, "no resultType from an older server means complete");
        let err = complete(Reply::Result(json!({ "resultType": "input_required", "inputRequests": {} }))).unwrap_err();
        assert!(err.contains("input"), "{err}");
        assert!(complete(Reply::Result(json!({ "resultType": "something_new" }))).is_err());
        let err = complete(Reply::Error { code: -32602, message: "Unknown tool: x".into(), data: None }).unwrap_err();
        assert!(err.contains("Unknown tool"), "{err}");
    }

    #[test]
    fn tools_are_listed_with_their_hints_and_the_next_page() {
        let result = json!({
            "resultType": "complete",
            "tools": [
                { "name": "get_weather", "title": "Weather", "description": "Current weather", "inputSchema": { "type": "object" },
                  "annotations": { "readOnlyHint": true } },
                { "name": "delete_all", "inputSchema": { "type": "object" } },
                { "title": "no name, skipped" }
            ],
            "nextCursor": "page-2"
        });
        let (tools, next) = tools_page(&result);
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0], ToolInfo { name: "get_weather".into(), title: Some("Weather".into()), description: Some("Current weather".into()), read_only_hint: Some(true) });
        assert_eq!(tools[1].read_only_hint, None);
        assert_eq!(next.as_deref(), Some("page-2"));
    }

    #[test]
    fn a_tool_result_is_its_text_and_its_structured_content() {
        let r = call_result(&json!({
            "content": [{ "type": "text", "text": "3 open" }, { "type": "image", "data": "…" }, { "type": "text", "text": "tickets" }],
            "structuredContent": { "open": 3 },
            "isError": false
        }));
        assert_eq!(r.text, "3 open\ntickets");
        assert_eq!(r.structured, Some(json!({ "open": 3 })));
        assert!(!r.is_error);
        assert!(call_result(&json!({ "content": [{ "type": "text", "text": "API down" }], "isError": true })).is_error);
    }

    #[test]
    fn an_event_stream_yields_its_messages() {
        let body = ": keep-alive\n\
event: message\n\
data: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\",\"params\":{}}\n\
\n\
data: {\"jsonrpc\":\"2.0\",\n\
data:  \"id\":5,\"result\":{\"ok\":true}}\n\
\n";
        let messages = sse_messages(body);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1]["id"], 5, "an event split over several data lines is joined");
        assert_eq!(sse_messages("data: not json\n\n"), Vec::<Value>::new());
        // CRLF line endings are allowed in an event stream.
        assert_eq!(sse_messages("data: {\"id\":1}\r\n\r\n").len(), 1);
    }

    #[test]
    fn header_values_are_plain_ascii_or_the_base64_sentinel() {
        assert_eq!(header_value("get_weather"), "get_weather");
        assert_eq!(header_value("Hello, 世界"), "=?base64?SGVsbG8sIOS4lueVjA==?=");
        assert_eq!(header_value(" padded "), "=?base64?IHBhZGRlZCA=?=");
        assert_eq!(header_value("line1\nline2"), "=?base64?bGluZTEKbGluZTI=?=");
        assert_eq!(header_value("=?base64?literal?="), "=?base64?PT9iYXNlNjQ/bGl0ZXJhbD89?=");
    }
}
