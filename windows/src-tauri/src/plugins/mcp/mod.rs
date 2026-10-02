// A small MCP client: enough to list a server's tools and call the one tool a
// plugin polls. It speaks the current, stateless revision (2026-07-28) and falls
// back to the initialize-based one (2025-11-25) for older servers, over stdio or
// streamable HTTP. See windows/docs/plugins.md.

pub mod http;
pub mod proto;
pub mod stdio;

use super::guard;
use super::http::SecretLookup;
use super::manifest::{self, Access, Manifest, McpTransport, Shown, Show, Source};
use proto::{CallResult, ToolInfo};

fn filled(pairs: &[(String, String)], secret: SecretLookup<'_>) -> Result<Vec<(String, String)>, String> {
    pairs.iter().map(|(k, v)| manifest::fill(v, secret).map(|v| (k.clone(), v))).collect()
}

enum Ask {
    Tools,
    Call(String, serde_json::Value),
}

enum Answer {
    Tools(Vec<ToolInfo>),
    Call(CallResult),
}

/// One question to the plugin's server, over whichever transport it uses.
async fn ask(m: &Manifest, secret: SecretLookup<'_>, question: Ask) -> Result<Answer, String> {
    let Source::Mcp(mcp) = &m.source else { return Err("not an MCP plugin".into()) };
    match &mcp.transport {
        McpTransport::Stdio { command, args, env } => {
            // Secrets first: a missing one stops everything before the server starts.
            let env = filled(env, secret)?;
            let (command, args) = (command.clone(), args.clone());
            tokio::task::spawn_blocking(move || {
                let mut session = stdio::Session::open(&command, &args, &env)?;
                let answer = match question {
                    Ask::Tools => session.conn().list_tools().map(Answer::Tools),
                    Ask::Call(name, arguments) => session.conn().call_tool(&name, &arguments).map(Answer::Call),
                };
                answer.map_err(|e| session.explain(e))
            })
            .await
            .map_err(|e| e.to_string())?
        }
        McpTransport::Http { url, headers } => {
            let headers = filled(headers, secret)?;
            let g = guard::client(m).await?;
            let mut session = http::HttpSession::new(&g, url, headers);
            match question {
                Ask::Tools => session.list_tools().await.map(Answer::Tools),
                Ask::Call(name, arguments) => session.call_tool(&name, &arguments).await.map(Answer::Call),
            }
        }
    }
}

/// Every tool the plugin's server offers — shown before the plugin is trusted.
pub async fn list_tools(m: &Manifest, secret: SecretLookup<'_>) -> Result<Vec<ToolInfo>, String> {
    match ask(m, secret, Ask::Tools).await? {
        Answer::Tools(tools) => Ok(tools),
        Answer::Call(_) => unreachable!(),
    }
}

/// What a tool's result shows: the manifest's mapping over its structured
/// content (or its text read as JSON), else the first line of its text.
fn shown_from(show: &Show, result: &CallResult) -> Shown {
    let value = result.structured.clone().or_else(|| serde_json::from_str(&result.text).ok());
    match value {
        Some(v) if show.headline.is_some() || show.items.is_some() => manifest::show(show, &v),
        _ => Shown {
            headline: result.text.lines().next().map(|l| l.chars().take(120).collect()),
            items: Vec::new(),
        },
    }
}

/// Calls the plugin's poll tool and picks out what to show.
pub async fn poll(m: &Manifest, secret: SecretLookup<'_>) -> Result<Shown, String> {
    let Source::Mcp(mcp) = &m.source else { return Err("not an MCP plugin".into()) };
    let poll = mcp.poll.as_ref().ok_or("This plugin has no tool to poll.")?;
    // Checked when the manifest was read; checked again right before the call.
    if mcp.tools.get(&poll.tool) != Some(&Access::Read) {
        return Err(format!("{} is not marked read-only; it is never called on a timer.", poll.tool));
    }
    match ask(m, secret, Ask::Call(poll.tool.clone(), poll.arguments.clone())).await? {
        Answer::Call(result) if result.is_error => {
            Err(format!("The tool reported an error: {}", result.text.chars().take(200).collect::<String>()))
        }
        Answer::Call(result) => Ok(shown_from(&m.show, &result)),
        Answer::Tools(_) => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn run<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap().block_on(f)
    }

    fn node() -> String {
        let found = crate::platform::find_all_on_path("node");
        found.into_iter().next().map(|p| p.to_string_lossy().to_string()).expect("these tests need Node.js on PATH")
    }

    fn plugin(mode: &str, show: serde_json::Value) -> Manifest {
        let v = json!({
            "schema": 1, "id": "tickets", "name": "Tickets", "version": "1", "kind": "mcp",
            "secrets": [{ "key": "api_key", "label": "API key" }],
            "mcp": {
                "transport": "stdio",
                "command": node(),
                "args": [concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/mcp-test-server.mjs"), mode],
                "env": { "TICKETS_KEY": "{{secret.api_key}}" },
                "poll": { "tool": "open_tickets", "arguments": { "limit": 2 } },
                "tools": { "open_tickets": "read", "close_ticket": "write" }
            },
            "show": show
        });
        manifest::parse(&serde_json::to_vec(&v).unwrap()).unwrap()
    }

    const SHOW: fn() -> serde_json::Value = || json!({ "headline": "/summary", "items": "/tickets", "title": "/title", "detail": "/state" });

    #[test]
    fn a_poll_calls_the_read_tool_with_the_secret_and_shows_its_result() {
        let secret = |k: &str| (k == "api_key").then(|| "k-1".to_string());
        let shown = run(poll(&plugin("modern", SHOW()), &secret)).unwrap();
        assert_eq!(shown.headline.as_deref(), Some("3 open tickets"));
        assert_eq!(shown.items.len(), 2, "the poll's arguments reached the tool");
        assert_eq!((shown.items[0].title.as_str(), shown.items[0].detail.as_str()), ("Login fails", "open"));
    }

    #[test]
    fn an_older_server_is_polled_the_same_way() {
        let secret = |_: &str| Some("x".to_string());
        let shown = run(poll(&plugin("legacy", SHOW()), &secret)).unwrap();
        assert_eq!(shown.headline.as_deref(), Some("3 open tickets"));
    }

    #[test]
    fn without_a_mapping_the_first_line_of_text_is_the_headline() {
        let secret = |_: &str| Some("x".to_string());
        let shown = run(poll(&plugin("modern", json!({})), &secret)).unwrap();
        assert!(shown.headline.unwrap().starts_with("{\"summary\""));
    }

    #[test]
    fn the_tools_are_listed_for_review() {
        let secret = |_: &str| Some("x".to_string());
        let tools = run(list_tools(&plugin("modern", json!({})), &secret)).unwrap();
        assert_eq!(tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), vec!["open_tickets", "close_ticket"]);
    }

    #[test]
    fn a_missing_secret_stops_before_the_server_starts() {
        let none = |_: &str| None;
        let err = run(poll(&plugin("modern", json!({})), &none)).unwrap_err();
        assert!(err.contains("api_key"), "{err}");
    }
}
