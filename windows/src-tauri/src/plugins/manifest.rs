// plugin.json: what a plugin is, where it may connect, and what it shows.
//
// Parsing is strict: unknown fields are refused (a typo must not silently turn
// a safety setting off), every host the plugin talks to is listed up front,
// secrets go into headers or the environment and never into a URL, and an MCP
// server runs from an absolute path to an executable — never through a shell.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub color: String,
    pub poll_secs: u64,
    pub secrets: Vec<SecretDecl>,
    pub allowed_hosts: Vec<String>,
    pub allow_private_network: bool,
    pub source: Source,
    pub show: Show,
    pub links: Vec<Link>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretDecl {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    Http { url: String, headers: Vec<(String, String)> },
    Mcp(McpSource),
}

#[derive(Debug, Clone, PartialEq)]
pub struct McpSource {
    pub transport: McpTransport,
    pub poll: Option<McpPoll>,
    pub tools: BTreeMap<String, Access>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum McpTransport {
    Stdio { command: String, args: Vec<String>, env: Vec<(String, String)> },
    Http { url: String, headers: Vec<(String, String)> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct McpPoll {
    pub tool: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    Read,
    Write,
}

/// Where in the response the headline and the list are. JSON Pointers (RFC 6901).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Show {
    #[serde(default)]
    pub headline: Option<String>,
    #[serde(default)]
    pub items: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
}

/// What a poll shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Shown {
    pub headline: Option<String>,
    pub items: Vec<ShownItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ShownItem {
    pub title: String,
    pub detail: String,
}

// ── The file as written ───────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Raw {
    schema: u32,
    id: String,
    name: String,
    version: String,
    #[serde(default)]
    color: Option<String>,
    kind: String,
    #[serde(default)]
    poll_secs: Option<u64>,
    #[serde(default)]
    secrets: Vec<SecretDecl>,
    #[serde(default)]
    allowed_hosts: Vec<String>,
    #[serde(default)]
    allow_private_network: bool,
    #[serde(default)]
    http: Option<RawHttp>,
    #[serde(default)]
    mcp: Option<RawMcp>,
    #[serde(default)]
    show: Show,
    #[serde(default)]
    links: Vec<Link>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawHttp {
    url: String,
    #[serde(default)]
    headers: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMcp {
    transport: String,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(default)]
    poll: Option<RawPoll>,
    #[serde(default)]
    tools: BTreeMap<String, Access>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPoll {
    tool: String,
    #[serde(default)]
    arguments: Option<Value>,
}

// ── Rules ─────────────────────────────────────────────────────────────────────

const DEFAULT_COLOR: &str = "#8C8C8C";
const DEFAULT_POLL: u64 = 300;
const MIN_POLL: u64 = 30;
const MAX_POLL: u64 = 86_400;
/// Executables that are a shell, or run a script given to them.
const SHELLS: &[&str] = &["cmd", "powershell", "pwsh", "bash", "sh", "wsl", "wscript", "cscript", "mshta", "conhost"];

fn chars_ok(s: &str, min: usize, max: usize, ok: impl Fn(char) -> bool) -> bool {
    let n = s.chars().count();
    n >= min && n <= max && s.chars().all(ok)
}

fn plain_text(s: &str, max: usize) -> bool {
    chars_ok(s, 1, max, |c| !c.is_control())
}

/// A plugin id: lowercase letters, digits and hyphens. No `_`, so it can never
/// be a built-in's `integration_*` id, and no `:`, so secrets stay in their
/// namespace.
pub fn valid_id(id: &str) -> bool {
    chars_ok(id, 1, 40, |c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') && !id.starts_with('-')
}

fn valid_secret_key(key: &str) -> bool {
    chars_ok(key, 1, 32, |c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn valid_host(host: &str) -> bool {
    if host.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    host.len() <= 253
        && !host.starts_with('.')
        && !host.ends_with('.')
        && host.split('.').all(|label| chars_ok(label, 1, 63, |c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'))
}

/// The host of a URL as it is written in `allowedHosts` (no IPv6 brackets).
pub fn host_of(url: &reqwest::Url) -> Option<String> {
    let host = url.host_str()?.to_ascii_lowercase();
    Some(host.trim_start_matches('[').trim_end_matches(']').to_string())
}

fn check_url(raw: &str, hosts: &[String], allow_private: bool, what: &str) -> Result<(), String> {
    if raw.contains("{{") {
        return Err(format!("{what}: no templates in an address — secrets go in headers."));
    }
    let url = reqwest::Url::parse(raw).map_err(|_| format!("{what}: not a web address."))?;
    match url.scheme() {
        "https" => {}
        "http" if allow_private => {}
        "http" => return Err(format!("{what}: plain http needs \"allowPrivateNetwork\", and then only reaches the local network.")),
        _ => return Err(format!("{what}: only https (or http on the local network).")),
    }
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err(format!("{what}: no credentials or #fragment in an address."));
    }
    let host = host_of(&url).ok_or_else(|| format!("{what}: no host."))?;
    if !hosts.iter().any(|h| *h == host) {
        return Err(format!("{what}: {host} is not in allowedHosts."));
    }
    Ok(())
}

/// Every `{{…}}` must be `{{secret.<declared key>}}`.
fn check_templates(value: &str, declared: &[SecretDecl], what: &str) -> Result<(), String> {
    let mut rest = value;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let end = after.find("}}").ok_or_else(|| format!("{what}: an unclosed {{{{."))?;
        let inner = &after[..end];
        let key = inner.strip_prefix("secret.").ok_or_else(|| format!("{what}: only {{{{secret.<key>}}}} may be filled in."))?;
        if !declared.iter().any(|s| s.key == key) {
            return Err(format!("{what}: secret \"{key}\" is not declared in \"secrets\"."));
        }
        rest = &after[end + 2..];
    }
    Ok(())
}

fn headers(raw: BTreeMap<String, String>, secrets: &[SecretDecl], what: &str) -> Result<Vec<(String, String)>, String> {
    if raw.len() > 16 {
        return Err(format!("{what}: at most 16 headers."));
    }
    raw.into_iter()
        .map(|(name, value)| {
            if !chars_ok(&name, 1, 64, |c| c.is_ascii_alphanumeric() || c == '-') {
                return Err(format!("{what}: header name \"{name}\"."));
            }
            if value.chars().count() > 1000 || value.chars().any(|c| c.is_control()) {
                return Err(format!("{what}: header \"{name}\" value."));
            }
            check_templates(&value, secrets, &format!("{what} header {name}"))?;
            Ok((name, value))
        })
        .collect()
}

fn check_command(command: &str) -> Result<(), String> {
    let path = std::path::Path::new(command);
    if !path.is_absolute() {
        return Err("mcp.command: an absolute path to the server's executable — nothing is looked up on PATH.".into());
    }
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    if SHELLS.contains(&stem.as_str()) {
        return Err("mcp.command: a shell is not a server. Point at the server's own executable.".into());
    }
    if cfg!(windows) {
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
        if ext != "exe" {
            return Err("mcp.command: an .exe — batch files and scripts run through a shell.".into());
        }
    }
    Ok(())
}

fn check_pointer(p: &Option<String>, what: &str) -> Result<(), String> {
    match p {
        Some(p) if !(p.is_empty() || p.starts_with('/')) || p.len() > 200 => {
            Err(format!("show.{what}: a JSON Pointer such as /status/description."))
        }
        _ => Ok(()),
    }
}

pub fn parse(bytes: &[u8]) -> Result<Manifest, String> {
    let raw: Raw = serde_json::from_slice(bytes).map_err(|e| format!("plugin.json: {e}"))?;
    if raw.schema != 1 {
        return Err(format!("plugin.json: schema {} is not known (1 is).", raw.schema));
    }
    if !valid_id(&raw.id) {
        return Err("id: lowercase letters, digits and hyphens, at most 40.".into());
    }
    if !plain_text(&raw.name, 40) || !plain_text(&raw.version, 20) {
        return Err("name and version: plain text, at most 40 and 20 characters.".into());
    }
    let color = match raw.color {
        None => DEFAULT_COLOR.to_string(),
        Some(c) if c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit()) => c,
        Some(_) => return Err("color: #RRGGBB.".into()),
    };
    let poll_secs = raw.poll_secs.unwrap_or(DEFAULT_POLL).clamp(MIN_POLL, MAX_POLL);

    if raw.secrets.len() > 8 {
        return Err("secrets: at most 8.".into());
    }
    for (i, s) in raw.secrets.iter().enumerate() {
        if !valid_secret_key(&s.key) || !plain_text(&s.label, 40) {
            return Err(format!("secrets[{i}]: key in a-z, 0-9 and _, a label of at most 40."));
        }
        if raw.secrets[..i].iter().any(|o| o.key == s.key) {
            return Err(format!("secrets: \"{}\" twice.", s.key));
        }
    }

    if raw.allowed_hosts.len() > 16 {
        return Err("allowedHosts: at most 16.".into());
    }
    let hosts: Vec<String> = raw.allowed_hosts.iter().map(|h| h.trim().to_ascii_lowercase()).collect();
    if let Some(bad) = hosts.iter().find(|h| !valid_host(h)) {
        return Err(format!("allowedHosts: \"{bad}\" — a host name or address, no wildcards, schemes or ports."));
    }

    let source = match (raw.kind.as_str(), raw.http, raw.mcp) {
        ("http", Some(http), None) => {
            check_url(&http.url, &hosts, raw.allow_private_network, "http.url")?;
            Source::Http { url: http.url, headers: headers(http.headers, &raw.secrets, "http")? }
        }
        ("mcp", None, Some(mcp)) => {
            let transport = match mcp.transport.as_str() {
                "stdio" => {
                    let command = mcp.command.ok_or("mcp.command: required for stdio.")?;
                    check_command(&command)?;
                    if mcp.args.len() > 32 || mcp.args.iter().any(|a| a.chars().count() > 1000 || a.contains("{{")) {
                        return Err("mcp.args: at most 32, no templates (a command line is visible to every program).".into());
                    }
                    if mcp.env.len() > 16 {
                        return Err("mcp.env: at most 16.".into());
                    }
                    let mut env = Vec::new();
                    for (name, value) in mcp.env {
                        let first_ok = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
                        if !first_ok || !chars_ok(&name, 1, 64, |c| c.is_ascii_alphanumeric() || c == '_') {
                            return Err(format!("mcp.env: \"{name}\" is not a variable name."));
                        }
                        check_templates(&value, &raw.secrets, &format!("mcp.env {name}"))?;
                        env.push((name, value));
                    }
                    McpTransport::Stdio { command, args: mcp.args, env }
                }
                "http" => {
                    let url = mcp.url.ok_or("mcp.url: required for http.")?;
                    check_url(&url, &hosts, raw.allow_private_network, "mcp.url")?;
                    McpTransport::Http { url, headers: headers(mcp.headers, &raw.secrets, "mcp")? }
                }
                other => return Err(format!("mcp.transport: \"{other}\" — stdio or http.")),
            };
            if mcp.tools.len() > 64 {
                return Err("mcp.tools: at most 64.".into());
            }
            if let Some(bad) = mcp.tools.keys().find(|t| !chars_ok(t, 1, 128, |c| c.is_ascii_alphanumeric() || "_.-".contains(c))) {
                return Err(format!("mcp.tools: \"{bad}\" is not a tool name."));
            }
            let poll = match mcp.poll {
                None => None,
                Some(p) => {
                    // Polled on a timer, with nobody watching: only what the user approved as read-only.
                    if mcp.tools.get(&p.tool) != Some(&Access::Read) {
                        return Err(format!("mcp.poll: \"{}\" must be listed in mcp.tools as \"read\".", p.tool));
                    }
                    let arguments = p.arguments.unwrap_or_else(|| Value::Object(Default::default()));
                    if !arguments.is_object() {
                        return Err("mcp.poll.arguments: an object.".into());
                    }
                    Some(McpPoll { tool: p.tool, arguments })
                }
            };
            Source::Mcp(McpSource { transport, poll, tools: mcp.tools })
        }
        ("http", ..) => return Err("kind http: needs \"http\", and no \"mcp\".".into()),
        ("mcp", ..) => return Err("kind mcp: needs \"mcp\", and no \"http\".".into()),
        (other, ..) => return Err(format!("kind: \"{other}\" — http or mcp.")),
    };

    check_pointer(&raw.show.headline, "headline")?;
    check_pointer(&raw.show.items, "items")?;
    check_pointer(&raw.show.title, "title")?;
    check_pointer(&raw.show.detail, "detail")?;

    if raw.links.len() > 4 {
        return Err("links: at most 4.".into());
    }
    for link in &raw.links {
        let ok = reqwest::Url::parse(&link.url)
            .map(|u| u.scheme() == "https" && u.username().is_empty() && u.password().is_none())
            .unwrap_or(false);
        if !ok || !plain_text(&link.label, 40) {
            return Err("links: https addresses with a short label.".into());
        }
    }

    Ok(Manifest {
        id: raw.id,
        name: raw.name,
        version: raw.version,
        color,
        poll_secs,
        secrets: raw.secrets,
        allowed_hosts: hosts,
        allow_private_network: raw.allow_private_network,
        source,
        show: raw.show,
        links: raw.links,
    })
}

/// Fills `{{secret.<key>}}` in a header or environment value.
pub fn fill(template: &str, secret: &dyn Fn(&str) -> Option<String>) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{secret.") {
        out.push_str(&rest[..start]);
        let after = &rest[start + "{{secret.".len()..];
        let end = after.find("}}").ok_or("an unclosed template")?;
        let key = &after[..end];
        let value = secret(key).ok_or_else(|| format!("The plugin's secret \"{key}\" is missing — set it in Settings → Plugins."))?;
        out.push_str(&value);
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

const MAX_ITEMS: usize = 20;
const MAX_TEXT: usize = 120;

fn text(v: &Value) -> Option<String> {
    let s = match v {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => return None,
    };
    Some(if s.chars().count() > MAX_TEXT { s.chars().take(MAX_TEXT).collect::<String>() + "…" } else { s })
}

/// Picks the headline and the rows out of a response.
pub fn show(spec: &Show, value: &Value) -> Shown {
    let at = |v: &Value, p: &Option<String>| -> Option<String> { p.as_deref().and_then(|p| v.pointer(p)).and_then(text) };
    let items = spec
        .items
        .as_deref()
        .and_then(|p| value.pointer(p))
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|item| {
                    let title = if spec.title.is_some() { at(item, &spec.title) } else { text(item) }?;
                    Some(ShownItem { title, detail: at(item, &spec.detail).unwrap_or_default() })
                })
                .take(MAX_ITEMS)
                .collect()
        })
        .unwrap_or_default();
    Shown { headline: at(value, &spec.headline), items }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const HTTP: &str = r##"{
      "schema": 1,
      "id": "github-status",
      "name": "GitHub status",
      "version": "1.0.0",
      "color": "#24292F",
      "kind": "http",
      "pollSecs": 5,
      "secrets": [{ "key": "token", "label": "API token" }],
      "allowedHosts": ["www.githubstatus.com"],
      "http": {
        "url": "https://www.githubstatus.com/api/v2/summary.json",
        "headers": { "Authorization": "Bearer {{secret.token}}" }
      },
      "show": { "headline": "/status/description", "items": "/components", "title": "/name", "detail": "/status" },
      "links": [{ "label": "Open status page", "url": "https://www.githubstatus.com" }]
    }"##;

    const MCP: &str = r##"{
      "schema": 1,
      "id": "tickets",
      "name": "Tickets",
      "version": "0.3.0",
      "kind": "mcp",
      "secrets": [{ "key": "api_key", "label": "API key" }],
      "allowedHosts": [],
      "mcp": {
        "transport": "stdio",
        "command": "NODE_PATH",
        "args": ["server.js"],
        "env": { "TICKETS_KEY": "{{secret.api_key}}" },
        "poll": { "tool": "open_tickets", "arguments": { "limit": 5 } },
        "tools": { "open_tickets": "read", "close_ticket": "write" }
      },
      "show": { "headline": "/summary" }
    }"##;

    /// An absolute path to an executable on the platform the tests run on,
    /// written as it appears inside a JSON string.
    #[cfg(windows)]
    const NODE: &str = r"C:\\Program Files\\nodejs\\node.exe";
    #[cfg(not(windows))]
    const NODE: &str = "/usr/bin/node";

    fn with(base: &str, edit: impl FnOnce(&mut Value)) -> Vec<u8> {
        let mut v: Value = serde_json::from_str(&base.replace("NODE_PATH", NODE)).unwrap();
        edit(&mut v);
        serde_json::to_vec(&v).unwrap()
    }

    #[test]
    fn an_http_plugin_reads_in_full() {
        let m = parse(HTTP.as_bytes()).unwrap();
        assert_eq!((m.id.as_str(), m.name.as_str(), m.version.as_str()), ("github-status", "GitHub status", "1.0.0"));
        assert_eq!(m.color, "#24292F");
        assert_eq!(m.poll_secs, 30, "polls are clamped to at least 30 seconds");
        assert_eq!(m.allowed_hosts, vec!["www.githubstatus.com"]);
        assert!(!m.allow_private_network);
        assert_eq!(
            m.source,
            Source::Http {
                url: "https://www.githubstatus.com/api/v2/summary.json".into(),
                headers: vec![("Authorization".into(), "Bearer {{secret.token}}".into())],
            }
        );
        assert_eq!(m.show.items.as_deref(), Some("/components"));
        assert_eq!(m.links[0].url, "https://www.githubstatus.com");
    }

    #[test]
    fn an_mcp_plugin_reads_in_full() {
        let m = parse(&with(MCP, |_| {})).unwrap();
        assert_eq!(m.poll_secs, 300, "five minutes unless asked otherwise");
        assert_eq!(m.color, "#8C8C8C", "a default colour");
        let Source::Mcp(mcp) = &m.source else { panic!("mcp") };
        assert_eq!(mcp.tools.get("open_tickets"), Some(&Access::Read));
        assert_eq!(mcp.tools.get("close_ticket"), Some(&Access::Write));
        assert_eq!(mcp.poll.as_ref().unwrap().arguments, json!({ "limit": 5 }));
        let McpTransport::Stdio { command, args, env } = &mcp.transport else { panic!("stdio") };
        assert!(command.ends_with("node.exe") || command.ends_with("/node"));
        assert_eq!(args.len(), 1);
        assert_eq!(env[0], ("TICKETS_KEY".to_string(), "{{secret.api_key}}".to_string()));
    }

    #[test]
    fn an_mcp_plugin_over_http_reads_too() {
        let bytes = with(MCP, |v| {
            v["allowedHosts"] = json!(["mcp.example.com"]);
            v["mcp"] = json!({
                "transport": "http",
                "url": "https://mcp.example.com/mcp",
                "headers": { "Authorization": "Bearer {{secret.api_key}}" },
                "tools": {}
            });
        });
        let m = parse(&bytes).unwrap();
        let Source::Mcp(mcp) = &m.source else { panic!() };
        assert!(matches!(mcp.transport, McpTransport::Http { .. }));
        assert!(mcp.poll.is_none());
    }

    fn refused(bytes: Vec<u8>, why: &str) {
        let err = parse(&bytes).err().unwrap_or_else(|| panic!("must be refused: {why}"));
        assert!(!err.is_empty(), "{why}");
    }

    #[test]
    fn anything_unsafe_or_unclear_is_refused_with_a_reason() {
        refused(b"not json".to_vec(), "not JSON");
        refused(with(HTTP, |v| v["schema"] = json!(2)), "unknown schema");
        refused(with(HTTP, |v| v["id"] = json!("Bad Id")), "id charset");
        refused(with(HTTP, |v| v["id"] = json!("integration_stripe")), "a built-in's id");
        refused(with(HTTP, |v| v["allowPrivateNetwrok"] = json!(true)), "a typo in a safety setting");
        refused(with(HTTP, |v| v["kind"] = json!("shell")), "unknown kind");
        refused(with(HTTP, |v| v["http"]["url"] = json!("https://evil.example.com/x")), "host not allowlisted");
        refused(with(HTTP, |v| v["http"]["url"] = json!("http://www.githubstatus.com/x")), "plain http to the internet");
        refused(with(HTTP, |v| v["http"]["url"] = json!("https://www.githubstatus.com/x?t={{secret.token}}")), "a secret in a URL");
        refused(with(HTTP, |v| v["http"]["url"] = json!("https://user:pw@www.githubstatus.com/x")), "credentials in a URL");
        refused(with(HTTP, |v| v["http"]["headers"]["X-Key"] = json!("{{secret.other}}")), "an undeclared secret");
        refused(with(HTTP, |v| v["http"]["headers"]["X-Key"] = json!("{{env.PATH}}")), "anything but secrets in a template");
        refused(with(HTTP, |v| v["allowedHosts"] = json!(["*.githubstatus.com"])), "wildcards");
        refused(with(HTTP, |v| v["links"] = json!([{ "label": "x", "url": "javascript:alert(1)" }])), "a link that is not https");
        refused(with(HTTP, |v| v["secrets"] = json!([{ "key": "a:b", "label": "x" }])), "a secret key that could escape its namespace");
        refused(with(HTTP, |v| v["show"]["headline"] = json!("status.description")), "not a JSON pointer");
        refused(with(MCP, |v| v["mcp"]["command"] = json!("node.exe")), "a command found through PATH");
        #[cfg(windows)]
        refused(with(MCP, |v| v["mcp"]["command"] = json!("C:\\tools\\run.cmd")), "a batch file is run by a shell");
        #[cfg(windows)]
        refused(with(MCP, |v| v["mcp"]["command"] = json!("C:\\Windows\\System32\\cmd.exe")), "a shell");
        #[cfg(not(windows))]
        refused(with(MCP, |v| v["mcp"]["command"] = json!("/bin/sh")), "a shell");
        refused(with(MCP, |v| v["mcp"]["tools"]["open_tickets"] = json!("write")), "polling a write tool");
        refused(with(MCP, |v| v["mcp"]["poll"]["tool"] = json!("unlisted")), "polling a tool nobody approved");
        refused(with(MCP, |v| v["mcp"]["env"]["bad name"] = json!("x")), "an environment name with a space");
    }

    #[test]
    fn plain_http_is_allowed_only_when_the_plugin_says_it_talks_to_the_local_network() {
        let bytes = with(HTTP, |v| {
            v["allowedHosts"] = json!(["192.168.1.10"]);
            v["allowPrivateNetwork"] = json!(true);
            v["http"]["url"] = json!("http://192.168.1.10:8123/api/states");
        });
        assert!(parse(&bytes).is_ok());
    }

    #[test]
    fn templates_take_only_the_plugins_own_secrets() {
        let secret = |k: &str| (k == "token").then(|| "s3cr3t".to_string());
        assert_eq!(fill("Bearer {{secret.token}}", &secret).unwrap(), "Bearer s3cr3t");
        assert_eq!(fill("no template", &secret).unwrap(), "no template");
        let err = fill("Bearer {{secret.missing}}", &secret).unwrap_err();
        assert!(err.contains("missing"), "{err}");
    }

    #[test]
    fn what_is_shown_is_picked_out_of_the_response_and_capped() {
        let spec = Show {
            headline: Some("/status/description".into()),
            items: Some("/components".into()),
            title: Some("/name".into()),
            detail: Some("/status".into()),
        };
        let mut components: Vec<Value> = (0..30).map(|i| json!({ "name": format!("c{i}"), "status": "operational" })).collect();
        components[1] = json!({ "name": "API", "status": 503 });
        components[2] = json!({ "name": "x".repeat(500), "status": "degraded" });
        let value = json!({ "status": { "description": "All Systems Operational" }, "components": components });
        let shown = show(&spec, &value);
        assert_eq!(shown.headline.as_deref(), Some("All Systems Operational"));
        assert_eq!(shown.items.len(), 20, "at most twenty rows");
        assert_eq!(shown.items[1], ShownItem { title: "API".into(), detail: "503".into() }, "numbers read as text");
        assert!(shown.items[2].title.chars().count() <= 121, "long text is cut");
        assert_eq!(show(&spec, &json!({})), Shown::default(), "nothing there, nothing shown");
    }
}
