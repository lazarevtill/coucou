// Chat with your own model server: anything that speaks the OpenAI chat
// completions API — llama.cpp's llama-server, LM Studio, Ollama, vLLM.
//
// The base URL is the user's; it is checked before anything is sent. Plain http
// is accepted only for this machine and the local network, where llama-server
// and friends usually live; anything else needs https. Redirects are not
// followed, so a local address can never send the conversation elsewhere.

use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};

use crate::claude::{ChatContext, ChatReply};

/// A base URL that passed `check_base_url`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseUrl {
    /// Without a trailing slash, e.g. `http://127.0.0.1:8080/v1`.
    pub url: String,
    /// This machine or the local network.
    pub local: bool,
}

/// Host names that only mean something on a local network.
const LOCAL_SUFFIXES: &[&str] = &[".local", ".lan", ".home.arpa", ".internal", ".localhost"];

fn is_local_host(url: &reqwest::Url) -> bool {
    use std::net::{Ipv4Addr, Ipv6Addr};
    let v4 = |ip: Ipv4Addr| ip.is_loopback() || ip.is_private() || ip.is_link_local();
    let v6 = |ip: Ipv6Addr| {
        let first = ip.segments()[0];
        ip.is_loopback() || (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
    };
    let Some(host) = url.host_str() else { return false };
    // An IPv6 literal comes back in brackets; IPv4 literals are already normalised.
    if let Some(inner) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        return inner.parse::<Ipv6Addr>().map(v6).unwrap_or(false);
    }
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return v4(ip);
    }
    let name = host.to_ascii_lowercase();
    name == "localhost" || LOCAL_SUFFIXES.iter().any(|s| name.ends_with(s))
}

/// What the settings hold, made safe to send a conversation to.
pub fn check_base_url(raw: &str) -> Result<BaseUrl, String> {
    let url = reqwest::Url::parse(raw.trim())
        .map_err(|_| "That is not a web address (e.g. http://127.0.0.1:8080/v1).".to_string())?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err("The address must start with http:// or https://.".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Put the key in the key field, not in the address.".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("The address must not have ?… or #… in it.".into());
    }
    if url.host().is_none() {
        return Err("The address has no server in it.".into());
    }
    let local = is_local_host(&url);
    if url.scheme() == "http" && !local {
        return Err("Plain http is only for this computer and your local network; use https for anything else.".into());
    }
    Ok(BaseUrl { url: url.as_str().trim_end_matches('/').to_string(), local })
}

pub fn request_body(model: &str, system: &str, history: &[Value], max_tokens: u32) -> Value {
    let mut messages = vec![json!({ "role": "system", "content": system })];
    messages.extend(history.iter().cloned());
    json!({ "model": model, "messages": messages, "max_tokens": max_tokens, "stream": false })
}

fn error_detail(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            let e = v.get("error")?;
            e.get("message").or(Some(e)).and_then(Value::as_str).map(str::to_string)
        })
        .unwrap_or_else(|| body.chars().take(200).collect())
}

/// The answer in a chat-completions response. Reasoning (`reasoning_content`)
/// is never shown as the answer.
pub fn parse_reply(status: u16, body: &str) -> Result<String, String> {
    if !(200..300).contains(&status) {
        return Err(format!("Model server {status}: {}", error_detail(body)));
    }
    let v: Value = serde_json::from_str(body).map_err(|_| "The model server sent something that is not JSON.".to_string())?;
    let message = v
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
        .and_then(|c| c.get("message"))
        .ok_or("The model server sent no answer.")?;
    let text = match message.get("content") {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string(),
        _ => String::new(),
    };
    if text.is_empty() {
        let thought = message.get("reasoning_content").and_then(Value::as_str).is_some_and(|r| !r.trim().is_empty());
        return Err(if thought {
            "The model thought it over but gave no answer — it may need more tokens, or a different model.".into()
        } else {
            "The model gave no answer.".into()
        });
    }
    Ok(text)
}

/// Model ids from `GET /models`: the OpenAI shape, or Ollama's.
pub fn parse_models(body: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<Value>(body) else { return Vec::new() };
    if let Some(data) = v.get("data").and_then(Value::as_array) {
        return data.iter().filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_string)).collect();
    }
    v.get("models")
        .and_then(Value::as_array)
        .map(|models| {
            models
                .iter()
                .filter_map(|m| m.get("name").or_else(|| m.get("model")).and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

const SYSTEM_PROMPT: &str = "You are Mochi, a personal AI assistant on the user's computer. \
Respond in the user's language. Be clear and complete. \
No markdown formatting (no **, no ##, no bullet dashes). Use plain text with line breaks.";
const MAX_TOKENS: u32 = 2048;
/// Text files are inlined up to this size, as with Claude.
const MAX_INLINE_TEXT: u64 = 200_000;

#[derive(Default)]
pub struct LlmChat {
    /// The conversation in chat-completions form, without the system message.
    messages: Mutex<Vec<Value>>,
    /// The model the server named when none was chosen, asked once per conversation.
    listed_model: Mutex<Option<String>>,
}

impl LlmChat {
    pub fn reset(&self) {
        self.messages.lock().unwrap().clear();
        *self.listed_model.lock().unwrap() = None;
    }
    fn is_empty(&self) -> bool {
        self.messages.lock().unwrap().is_empty()
    }
    fn push(&self, message: Value) {
        self.messages.lock().unwrap().push(message);
    }
    fn pop(&self) {
        self.messages.lock().unwrap().pop();
    }
    fn snapshot(&self) -> Vec<Value> {
        self.messages.lock().unwrap().clone()
    }
}

pub struct ServerConfig {
    pub base: BaseUrl,
    /// Empty: the first model the server lists.
    pub model: String,
    pub key: Option<String>,
}

/// A CPU-only model can take minutes over a long answer; a remote API should not.
fn timeout_for(base: &BaseUrl) -> Duration {
    Duration::from_secs(if base.local { 600 } else { 120 })
}

fn client(cfg: &ServerConfig) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(timeout_for(&cfg.base))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())
}

fn with_key(request: reqwest::RequestBuilder, cfg: &ServerConfig) -> reqwest::RequestBuilder {
    match &cfg.key {
        Some(key) => request.bearer_auth(key),
        None => request,
    }
}

async fn read(response: Result<reqwest::Response, reqwest::Error>, cfg: &ServerConfig) -> Result<(u16, String), String> {
    let response = response.map_err(|e| {
        if e.is_timeout() {
            format!("The model server at {} took too long to answer.", cfg.base.url)
        } else {
            format!("Can't reach the model server at {}.", cfg.base.url)
        }
    })?;
    let status = response.status().as_u16();
    let body = response.text().await.map_err(|e| e.to_string())?;
    Ok((status, body))
}

pub async fn list_models(cfg: &ServerConfig) -> Result<Vec<String>, String> {
    let http = client(cfg)?;
    let sent = with_key(http.get(format!("{}/models", cfg.base.url)), cfg).send().await;
    let (status, body) = read(sent, cfg).await?;
    if !(200..300).contains(&status) {
        return Err(format!("Model server {status}: {}", error_detail(&body)));
    }
    Ok(parse_models(&body))
}

/// What a dropped file contributes. Only text: this source is not assumed to
/// read PDFs or images.
fn file_text(name: &str, path: &str) -> Result<String, String> {
    let ext = std::path::Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if ["pdf", "jpg", "jpeg", "png", "gif", "webp"].contains(&ext.as_str()) {
        return Err("Your model server can only read text files here, not PDFs or images. Switch the chat to Claude in Settings for those.".into());
    }
    let small = std::fs::metadata(path).map(|m| m.len() <= MAX_INLINE_TEXT).unwrap_or(false);
    match std::fs::read_to_string(path).ok().filter(|_| small) {
        Some(text) => Ok(format!("File: {name}\nFile contents:\n{text}")),
        None => Ok(format!("File: {name}")),
    }
}

/// One chat turn with the model server.
pub async fn send(chat: &LlmChat, cfg: &ServerConfig, query: String, context: Option<ChatContext>) -> Result<ChatReply, String> {
    let mut content = String::new();
    // Like the Claude chat: the file or window rides along with the first message.
    if chat.is_empty() {
        match &context {
            Some(ChatContext::File { name, path }) => content.push_str(&file_text(name, path)?),
            Some(ChatContext::Window { app_name, title, url }) => {
                content.push_str(&format!("Context — App: {app_name}, Window: {title}"));
                if let Some(url) = url {
                    content.push_str(&format!(", URL: {url}"));
                }
            }
            None => {}
        }
        if !content.is_empty() {
            content.push_str("\n\n");
        }
    }
    content.push_str(&query);

    let cached = chat.listed_model.lock().unwrap().clone();
    let model = match (cfg.model.is_empty(), cached) {
        (false, _) => cfg.model.clone(),
        (true, Some(listed)) => listed,
        (true, None) => {
            let listed = list_models(cfg).await?.into_iter().next().ok_or("The model server lists no model.")?;
            *chat.listed_model.lock().unwrap() = Some(listed.clone());
            listed
        }
    };

    chat.push(json!({ "role": "user", "content": content }));
    let body = request_body(&model, SYSTEM_PROMPT, &chat.snapshot(), MAX_TOKENS);
    let http = client(cfg)?;
    let sent = with_key(http.post(format!("{}/chat/completions", cfg.base.url)).json(&body), cfg).send().await;
    let answer = match read(sent, cfg).await.and_then(|(status, body)| parse_reply(status, &body)) {
        Ok(text) => text,
        Err(err) => {
            chat.pop(); // the history stays what the model actually saw
            return Err(err);
        }
    };
    chat.push(json!({ "role": "assistant", "content": answer }));
    Ok(ChatReply { text: answer })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex as StdMutex};

    // ── Base URL ──────────────────────────────────────────────────────────────

    #[test]
    fn local_servers_may_use_plain_http() {
        for raw in [
            "http://127.0.0.1:8080/v1",
            "http://localhost:1234/v1/",
            "http://[::1]:8080/v1",
            "http://192.168.1.20:8080/v1",
            "http://10.0.0.5/v1",
            "http://172.20.0.2:11434/v1",
            "http://gpu-box.local:8080/v1",
            "http://nas.lan:8080/v1",
        ] {
            let b = check_base_url(raw).unwrap_or_else(|e| panic!("{raw}: {e}"));
            assert!(b.local, "{raw}");
            assert!(!b.url.ends_with('/'), "{raw} -> {}", b.url);
        }
    }

    #[test]
    fn remote_servers_need_https() {
        assert!(check_base_url("http://api.example.com/v1").is_err());
        assert!(check_base_url("http://8.8.8.8/v1").is_err());
        assert!(check_base_url("http://172.32.0.1/v1").is_err(), "just outside 172.16/12");
        let b = check_base_url("https://api.example.com/v1").unwrap();
        assert!(!b.local);
        assert_eq!(b.url, "https://api.example.com/v1");
    }

    #[test]
    fn tricks_in_the_url_are_refused() {
        for raw in [
            "",
            "127.0.0.1:8080",
            "ftp://127.0.0.1/v1",
            "file:///C:/x",
            "http://user:pass@127.0.0.1:8080/v1",
            "http://127.0.0.1:8080/v1?redirect=https://evil",
            "http://127.0.0.1:8080/v1#x",
            "javascript:alert(1)",
        ] {
            assert!(check_base_url(raw).is_err(), "{raw:?} must be refused");
        }
    }

    // ── Request and reply ─────────────────────────────────────────────────────

    #[test]
    fn the_request_is_a_plain_chat_completion() {
        let history = vec![json!({"role": "user", "content": "hi"})];
        let body = request_body("qwen", "be brief", &history, 512);
        assert_eq!(body["model"], "qwen");
        assert_eq!(body["max_tokens"], 512);
        assert_eq!(body["stream"], false);
        assert_eq!(body["messages"][0], json!({"role": "system", "content": "be brief"}));
        assert_eq!(body["messages"][1], json!({"role": "user", "content": "hi"}));
        assert!(body.get("tools").is_none(), "no Anthropic-only tools here");
    }

    #[test]
    fn the_answer_is_the_message_content() {
        let ok = r#"{"choices":[{"index":0,"message":{"role":"assistant","content":"  Hello there. "},"finish_reason":"stop"}]}"#;
        assert_eq!(parse_reply(200, ok).unwrap(), "Hello there.");
        // Some servers send the content as parts.
        let parts = r#"{"choices":[{"message":{"role":"assistant","content":[{"type":"text","text":"A"},{"type":"text","text":"B"}]}}]}"#;
        assert_eq!(parse_reply(200, parts).unwrap(), "A\nB");
    }

    #[test]
    fn a_reasoning_model_that_never_answered_says_so() {
        // gpt-oss and friends keep their thinking apart from the answer.
        let only_thinking = r#"{"choices":[{"message":{"role":"assistant","content":"","reasoning_content":"Let me think..."},"finish_reason":"length"}]}"#;
        let err = parse_reply(200, only_thinking).unwrap_err();
        assert!(err.contains("no answer"), "{err}");
        // And the thinking is never shown as the answer.
        let both = r#"{"choices":[{"message":{"content":"42","reasoning_content":"secret steps"}}]}"#;
        assert_eq!(parse_reply(200, both).unwrap(), "42");
    }

    #[test]
    fn server_errors_are_shown_in_the_servers_words() {
        let err = parse_reply(401, r#"{"error":{"message":"Invalid API Key","type":"authentication_error"}}"#).unwrap_err();
        assert!(err.contains("401") && err.contains("Invalid API Key"), "{err}");
        let err = parse_reply(500, "upstream went away").unwrap_err();
        assert!(err.contains("upstream went away"), "{err}");
        assert!(parse_reply(200, "not json").is_err());
        assert!(parse_reply(200, r#"{"choices":[]}"#).is_err());
    }

    #[test]
    fn models_are_read_from_either_shape() {
        // OpenAI shape, which llama-server also sends.
        assert_eq!(parse_models(r#"{"object":"list","data":[{"id":"qwen2.5-0.5b"},{"id":"other"}]}"#), vec!["qwen2.5-0.5b", "other"]);
        // Ollama's own /api/tags-like shape.
        assert_eq!(parse_models(r#"{"models":[{"name":"llama3:8b"}]}"#), vec!["llama3:8b"]);
        assert!(parse_models("nonsense").is_empty());
    }

    #[test]
    fn local_models_get_minutes_remote_ones_less() {
        let local = check_base_url("http://127.0.0.1:8080/v1").unwrap();
        let remote = check_base_url("https://api.example.com/v1").unwrap();
        assert!(timeout_for(&local) >= Duration::from_secs(300), "a CPU model can be slow");
        assert!(timeout_for(&remote) <= Duration::from_secs(180));
        assert!(timeout_for(&remote) > Duration::from_secs(30));
    }

    // ── Against a real HTTP server ────────────────────────────────────────────

    /// A one-thread HTTP server answering each request with the next canned
    /// response, recording what it was sent.
    fn mock_server(responses: Vec<(u16, &'static str)>) -> (String, Arc<StdMutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = Arc::new(StdMutex::new(Vec::new()));
        let record = seen.clone();
        std::thread::spawn(move || {
            for (status, body) in responses {
                let Ok((mut stream, _)) = listener.accept() else { return };
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                // Read headers, then as much body as Content-Length says.
                loop {
                    let n = stream.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    if let Some(end) = text.find("\r\n\r\n") {
                        let len = text
                            .lines()
                            .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                            .unwrap_or(0);
                        if buf.len() >= end + 4 + len {
                            break;
                        }
                    }
                }
                record.lock().unwrap().push(String::from_utf8_lossy(&buf).to_string());
                let reply = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        (format!("http://127.0.0.1:{port}/v1"), seen)
    }

    fn run<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
    }

    const MODELS: &str = r#"{"object":"list","data":[{"id":"tiny-model"}]}"#;
    const REPLY_1: &str = r#"{"choices":[{"message":{"role":"assistant","content":"Paris."}}]}"#;
    const REPLY_2: &str = r#"{"choices":[{"message":{"role":"assistant","content":"About 2.1 million."}}]}"#;

    #[test]
    fn a_conversation_with_a_real_server_keeps_its_history_and_sends_the_key() {
        let (base, seen) = mock_server(vec![(200, MODELS), (200, REPLY_1), (200, REPLY_2)]);
        let cfg = ServerConfig { base: check_base_url(&base).unwrap(), model: String::new(), key: Some("sk-local-test".into()) };
        let chat = LlmChat::default();
        let first = run(send(&chat, &cfg, "Capital of France?".into(), None)).unwrap();
        assert_eq!(first.text, "Paris.");
        let second = run(send(&chat, &cfg, "Population?".into(), None)).unwrap();
        assert_eq!(second.text, "About 2.1 million.");

        let seen = seen.lock().unwrap();
        assert!(seen[0].starts_with("GET /v1/models "), "no model chosen: ask the server ({})", &seen[0][..40]);
        assert!(seen[1].starts_with("POST /v1/chat/completions "));
        assert!(seen[1].to_lowercase().contains("authorization: bearer sk-local-test"));
        assert!(seen[1].contains(r#""model":"tiny-model""#));
        // The second turn carries the first question and answer.
        assert!(seen[2].contains("Capital of France?") && seen[2].contains("Paris.") && seen[2].contains("Population?"));
    }

    #[test]
    fn a_failed_turn_leaves_the_history_as_it_was() {
        let (base, seen) = mock_server(vec![(500, r#"{"error":{"message":"out of memory"}}"#), (200, REPLY_1)]);
        let cfg = ServerConfig { base: check_base_url(&base).unwrap(), model: "m".into(), key: None };
        let chat = LlmChat::default();
        let err = run(send(&chat, &cfg, "first try".into(), None)).unwrap_err();
        assert!(err.contains("out of memory"), "{err}");
        run(send(&chat, &cfg, "second try".into(), None)).unwrap();
        let seen = seen.lock().unwrap();
        assert!(!seen[1].contains("first try"), "the failed question must not be resent");
        assert!(!seen[0].to_lowercase().contains("authorization"), "no key, no header");
    }

    #[test]
    fn a_redirect_is_not_followed() {
        let (base, seen) = mock_server(vec![(302, r#"{}"#)]);
        let cfg = ServerConfig { base: check_base_url(&base).unwrap(), model: "m".into(), key: None };
        let err = run(send(&LlmChat::default(), &cfg, "hello".into(), None)).unwrap_err();
        assert!(err.contains("302"), "{err}");
        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    #[test]
    fn listing_models_asks_the_server() {
        let (base, _) = mock_server(vec![(200, MODELS)]);
        let cfg = ServerConfig { base: check_base_url(&base).unwrap(), model: String::new(), key: None };
        assert_eq!(run(list_models(&cfg)).unwrap(), vec!["tiny-model"]);
    }

    #[test]
    fn files_the_server_cannot_read_are_refused_before_sending() {
        let (base, seen) = mock_server(vec![]);
        let cfg = ServerConfig { base: check_base_url(&base).unwrap(), model: "m".into(), key: None };
        let pdf = std::env::temp_dir().join(format!("coucou-llm-{}.pdf", std::process::id()));
        std::fs::write(&pdf, b"%PDF-1.4").unwrap();
        let context = Some(ChatContext::File { name: "a.pdf".into(), path: pdf.to_string_lossy().into() });
        let err = run(send(&LlmChat::default(), &cfg, "summarise".into(), context)).unwrap_err();
        assert!(err.to_lowercase().contains("pdf"), "{err}");
        assert!(seen.lock().unwrap().is_empty(), "nothing was sent");
        let _ = std::fs::remove_file(pdf);
    }
}
