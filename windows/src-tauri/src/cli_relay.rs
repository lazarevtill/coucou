// The chat on the user's own subscription (Settings → Chat → Your AI
// subscriptions, as on the Mac: CLIRelay.swift). With the switch on, the chat
// is answered by the provider's official terminal tool, signed in with the
// user's own account: Claude Code (cli_agents.rs), Antigravity CLI or Gemini
// CLI, Codex CLI, Cursor CLI and GitHub Copilot CLI. Coucou reads no
// credential and makes no network call of its own.
//
// Claude Code and Codex use cli_agents, with tools disabled and shared visual
// attachments. Other relays use their official plan/ask modes in a private
// folder and explicit tool restrictions where the CLI exposes them. Those
// modes are not an OS sandbox and may inherit the CLI's user configuration.
// A dropped text file is read locally with a hard size bound; no web tools are
// requested for that turn. Every subprocess is bounded and cancellable.

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};

use crate::chat::{self, Chat, ChatContext, ChatReply, ModelInfo};
use crate::i18n::tf;
use crate::settings::Settings;
use crate::{cli_agents, platform, settings};

/// How long a chat turn may take: it may search the web.
const CHAT_TIMEOUT: Duration = Duration::from_secs(180);
/// A dropped text file's contents go in the prompt up to this size.
const MAX_INLINE_TEXT: u64 = 200_000;

/// The Gemini CLI policy that keeps a run off the web while it reads a file.
const NO_WEB_POLICY: &str = "[[rule]]\ntoolName = [\"google_web_search\", \"web_fetch\"]\ndecision = \"deny\"\npriority = 900\ndenyMessage = \"No web while Coucou reads a file.\"\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cli {
    Claude,
    Antigravity,
    Gemini,
    Codex,
    Cursor,
    Copilot,
}

impl Cli {
    pub fn name(self) -> &'static str {
        match self {
            Cli::Claude => "Claude Code",
            Cli::Antigravity => "Antigravity CLI",
            Cli::Gemini => "Gemini CLI",
            Cli::Codex => "Codex CLI",
            Cli::Cursor => "Cursor CLI",
            Cli::Copilot => "Copilot CLI",
        }
    }

    /// Its executable names, the official one first.
    fn binaries(self) -> &'static [&'static str] {
        match self {
            Cli::Claude => &["claude"],
            Cli::Antigravity => &["agy"],
            Cli::Gemini => &["gemini"],
            Cli::Codex => &["codex"],
            Cli::Cursor => &["cursor-agent", "agent"],
            Cli::Copilot => &["copilot"],
        }
    }

    /// What to type once in a terminal to sign in.
    pub fn sign_in(self) -> &'static str {
        match self {
            Cli::Claude => "claude",
            Cli::Antigravity => "agy",
            Cli::Gemini => "gemini",
            Cli::Codex => "codex login",
            Cli::Cursor => "cursor-agent login",
            Cli::Copilot => "copilot",
        }
    }

    pub fn install_url(self) -> &'static str {
        match self {
            Cli::Claude => "https://code.claude.com/docs/en/setup",
            Cli::Antigravity => "https://antigravity.google/docs/cli",
            Cli::Gemini => "https://geminicli.com",
            Cli::Codex => "https://developers.openai.com/codex/cli",
            Cli::Cursor => "https://cursor.com/cli",
            Cli::Copilot => "https://docs.github.com/copilot/how-tos/copilot-cli",
        }
    }

    /// The tool, when it is installed.
    pub fn exe(self) -> Option<PathBuf> {
        self.binaries().iter().find_map(|name| platform::cli_candidates(name).into_iter().next())
    }
}

/// The providers listed under "Your AI subscriptions", in order.
pub const PROVIDERS: [&str; 5] = ["anthropic", "google", "openai", "cursor", "copilot"];

/// The tools that answer for `provider`, the preferred one first. Empty: API only.
pub fn clis_for(provider: &str) -> &'static [Cli] {
    match provider {
        "anthropic" => &[Cli::Claude],
        "google" => &[Cli::Antigravity, Cli::Gemini],
        "openai" => &[Cli::Codex],
        "cursor" => &[Cli::Cursor],
        "copilot" => &[Cli::Copilot],
        _ => &[],
    }
}

/// Cursor and Copilot have no API here: they answer only through the subscription.
pub fn subscription_only(provider: &str) -> bool {
    matches!(provider, "cursor" | "copilot")
}

/// The installed tool that answers for `provider`, if any.
pub fn installed(provider: &str) -> Option<(Cli, PathBuf)> {
    clis_for(provider).iter().find_map(|cli| cli.exe().map(|path| (*cli, path)))
}

/// Whether the chat with `provider` goes through the user's subscription.
pub fn uses_subscription(settings: &Settings, provider: &str) -> bool {
    if clis_for(provider).is_empty() {
        return false;
    }
    if let Some(on) = settings.chat_subscriptions.get(provider) {
        return *on;
    }
    // An upgrade must not switch an existing API provider to CLI billing.
    // Explicit Claude Code/Codex providers remain independently selectable.
    false
}

/// One row of Settings → Chat → Your AI subscriptions.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub provider: &'static str,
    /// The tool that answers (or would, once installed).
    pub tool: &'static str,
    pub installed: bool,
    pub on: bool,
    pub sign_in: &'static str,
    pub install_url: &'static str,
    pub subscription_only: bool,
}

pub fn status(settings: &Settings) -> Vec<Status> {
    PROVIDERS
        .iter()
        .map(|provider| {
            let found = installed(provider).map(|(cli, _)| cli);
            let cli = found.unwrap_or(clis_for(provider)[0]);
            Status {
                provider,
                tool: cli.name(),
                installed: found.is_some(),
                on: uses_subscription(settings, provider),
                sign_in: cli.sign_in(),
                install_url: cli.install_url(),
                subscription_only: subscription_only(provider),
            }
        })
        .collect()
}

/// The models offered in the picker: Claude Code's, else the tool's own choice.
pub fn models(provider: &str) -> Vec<ModelInfo> {
    if provider == "anthropic" {
        return cli_agents::models(cli_agents::Agent::Claude);
    }
    let tool = installed(provider).map(|(cli, _)| cli).unwrap_or(clis_for(provider)[0]);
    vec![ModelInfo { id: String::new(), label: tf("The model chosen in {tool}", &[("tool", tool.name())]) }]
}

// ── One run ───────────────────────────────────────────────────────────────────

pub struct Request<'a> {
    /// None: the tool's own default model.
    pub model: Option<&'a str>,
    pub system: &'a str,
    pub prompt: &'a str,
    pub web: bool,
    pub no_web_policy: Option<&'a Path>,
}

/// The prompt as the tool receives it: the system prompt on top.
fn full_prompt(r: &Request<'_>) -> String {
    format!("<instructions>\n{}\n</instructions>\n\n{}", r.system, r.prompt)
}

/// The command line of a generic relay; Claude Code/Codex sends use cli_agents.
/// `prompt_arg`: the prompt goes in the arguments; else on stdin (npm's `.cmd`
/// shims on Windows can't take a multi-line argument).
fn args(cli: Cli, r: &Request<'_>, prompt_arg: bool) -> Vec<OsString> {
    let model = r.model.filter(|m| !m.is_empty());
    let prompt = full_prompt(r);
    let mut out: Vec<String> = Vec::new();
    match cli {
        Cli::Claude => unreachable!("Claude Code runs through cli_agents.rs"),
        Cli::Gemini => {
            // Plan mode: read-only tools and Google search, nothing that writes or runs.
            if prompt_arg {
                out.extend(["-p".into(), prompt]);
            }
            out.extend(
                ["--output-format", "json", "--approval-mode", "plan", "--skip-trust", "--extensions", "none",
                 "--allowed-mcp-server-names", "coucou-none"]
                    .map(String::from),
            );
            if let (false, Some(policy)) = (r.web, r.no_web_policy) {
                out.extend(["--policy".into(), policy.to_string_lossy().into_owned()]);
            }
        }
        Cli::Antigravity => {
            out.extend(["-p".into(), prompt]);
            out.extend(["--output-format", "json", "--mode", "plan", "--disable-slash-commands"].map(String::from));
        }
        Cli::Codex => {
            // Read-only sandbox and no shell at all; it searches only for the chat.
            if r.web {
                out.push("--search".into());
            }
            out.extend(
                ["exec", "--json", "--skip-git-repo-check", "--ephemeral", "--ignore-user-config", "--ignore-rules",
                 "--sandbox", "read-only", "--color", "never"]
                    .map(String::from),
            );
            for feature in ["shell_tool", "unified_exec", "apps", "plugins", "hooks"] {
                out.extend(["--disable".into(), feature.into()]);
            }
        }
        Cli::Cursor => {
            // Print mode, ask mode: answers and reads, never edits or runs (no --force).
            out.push("-p".into());
            if prompt_arg {
                out.push(prompt);
            }
            out.extend(["--output-format", "json", "--mode", "ask", "--trust"].map(String::from));
        }
        Cli::Copilot => {
            if prompt_arg {
                out.extend(["-p".into(), prompt]);
            }
            out.extend(
                ["--silent", "--no-custom-instructions", "--disable-builtin-mcps", "--no-ask-user", "--no-auto-update",
                 "--no-color"]
                    .map(String::from),
            );
            if r.web {
                out.extend(["--available-tools", "web_fetch"].map(String::from));
            } else {
                out.extend(["--available-tools", "view"].map(String::from));
            }
        }
    }
    if let Some(model) = model {
        out.extend(["--model".into(), model.to_string()]);
    }
    if cli == Cli::Codex {
        out.push("-".into());
    }
    out.into_iter().map(OsString::from).collect()
}

/// Whether the prompt goes in on stdin: Codex always; Gemini, Cursor and
/// Copilot when they are npm's `.cmd` shims; Antigravity never.
fn prompt_on_stdin(cli: Cli, exe: &Path) -> bool {
    let shim = exe
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"));
    match cli {
        Cli::Codex | Cli::Claude => true,
        Cli::Antigravity => false,
        Cli::Gemini | Cli::Cursor | Cli::Copilot => shim,
    }
}

/// Runs the tool once and returns the text of its answer.
async fn ask(runtime: &cli_agents::CliChat, cli: Cli, exe: &Path, r: &Request<'_>) -> Result<String, String> {
    let workdir = settings::local_dir().join("relay");
    platform::ensure_private_dir(&workdir).map_err(|e| e.to_string())?;
    let stdin_prompt = prompt_on_stdin(cli, exe);
    let prompt = full_prompt(r);
    if prompt.len() > 1024 * 1024 { return Err("This conversation is too large. Start a new chat.".into()); }
    let mut cmd = Command::new(exe);
    cmd.args(args(cli, r, !stdin_prompt)).current_dir(&workdir)
        .env_remove("CLAUDECODE").env("NO_COLOR", "1");
    for key in ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "OPENAI_API_KEY", "GEMINI_API_KEY", "GOOGLE_API_KEY", "CURSOR_API_KEY"] {
        cmd.env_remove(key);
    }
    if let Some(dir) = exe.parent() {
        let mut dirs = vec![dir.to_path_buf()];
        if let Some(path) = std::env::var_os("PATH") { dirs.extend(std::env::split_paths(&path)); }
        if let Ok(joined) = std::env::join_paths(dirs) { cmd.env("PATH", joined); }
    }
    let output = cli_agents::run_command(runtime, cmd, stdin_prompt.then_some(prompt), CHAT_TIMEOUT).await?;
    parse(cli, &output.stdout, &output.stderr, output.success)
}

// ── Output formats ────────────────────────────────────────────────────────────

/// The answer's text, or why there is none.
fn parse(cli: Cli, stdout: &[u8], stderr: &[u8], success: bool) -> Result<String, String> {
    let detail = last_line(stderr);
    let result = match cli {
        Cli::Claude | Cli::Cursor => parse_result_object(cli, stdout),
        Cli::Gemini | Cli::Antigravity => parse_response_object(cli, stdout, stderr),
        Cli::Codex => parse_codex_events(cli, stdout),
        Cli::Copilot => {
            let text = String::from_utf8_lossy(stdout).trim().to_string();
            if success && !text.is_empty() { Ok(text) } else { Err(Failure::NoAnswer) }
        }
    };
    let result = match result {
        Ok(_) if !success => Err(Failure::Said(if detail.is_empty() {
            tf("{tool} could not answer.", &[("tool", cli.name())])
        } else { detail.clone() })),
        other => other,
    };
    match result {
        Ok(text) => Ok(text),
        Err(failure) => {
            let said = match &failure {
                Failure::Said(why) => why.clone(),
                Failure::NoAnswer => detail.clone(),
            };
            if needs_sign_in(&said) {
                return Err(tf(
                    "{tool} is not signed in. Sign in once in a terminal: {command}",
                    &[("tool", cli.name()), ("command", cli.sign_in())],
                ));
            }
            match failure {
                Failure::Said(why) => Err(why),
                Failure::NoAnswer if detail.is_empty() => Err(tf("{tool} gave no answer.", &[("tool", cli.name())])),
                Failure::NoAnswer => Err(format!("{} ({detail})", tf("{tool} gave no answer.", &[("tool", cli.name())]))),
            }
        }
    }
}

enum Failure {
    NoAnswer,
    Said(String),
}

/// Claude Code and Cursor: `{"result": …, "is_error": …}`.
fn parse_result_object(cli: Cli, stdout: &[u8]) -> Result<String, Failure> {
    let json = last_json_object(stdout).ok_or(Failure::NoAnswer)?;
    let text = json.get("result").and_then(Value::as_str).unwrap_or("").trim().to_string();
    if json.get("is_error").and_then(Value::as_bool).unwrap_or(false) {
        return Err(Failure::Said(if text.is_empty() {
            tf("{tool} could not answer.", &[("tool", cli.name())])
        } else {
            text
        }));
    }
    if text.is_empty() { Err(Failure::NoAnswer) } else { Ok(text) }
}

/// Gemini CLI and Antigravity CLI: `{"response": …, "error": {…}}`, the error on stderr too.
fn parse_response_object(_cli: Cli, stdout: &[u8], stderr: &[u8]) -> Result<String, Failure> {
    let out = last_json_object(stdout);
    let err = last_json_object(stderr);
    for json in [out.as_ref(), err.as_ref()].into_iter().flatten() {
        let error = &json["error"];
        let message = error["message"].as_str().or_else(|| error.as_str()).unwrap_or("");
        if !message.is_empty() {
            return Err(Failure::Said(message.to_string()));
        }
    }
    if let Some(text) = out.as_ref().and_then(|j| j.get("response")).and_then(Value::as_str) {
        if !text.trim().is_empty() {
            return Ok(text.trim().to_string());
        }
    }
    Err(Failure::NoAnswer)
}

/// `codex exec --json`: one event per line; the answer is the last finished agent message.
fn parse_codex_events(_cli: Cli, stdout: &[u8]) -> Result<String, Failure> {
    let mut answer = None;
    let mut failure = None;
    for line in String::from_utf8_lossy(stdout).lines() {
        let Ok(event) = serde_json::from_str::<Value>(line) else { continue };
        match event["type"].as_str() {
            Some("item.completed") if event["item"]["type"] == "agent_message" => {
                if let Some(text) = event["item"]["text"].as_str().filter(|t| !t.is_empty()) {
                    answer = Some(text.trim().to_string());
                }
            }
            Some("turn.failed") => failure = event["error"]["message"].as_str().map(String::from),
            Some("error") => failure = event["message"].as_str().map(String::from),
            _ => {}
        }
    }
    match (answer, failure) {
        (Some(text), _) => Ok(text),
        (None, Some(why)) => Err(Failure::Said(why)),
        (None, None) => Err(Failure::NoAnswer),
    }
}

/// The last JSON object in a tool's output (some print a line or two before it).
fn last_json_object(bytes: &[u8]) -> Option<Value> {
    let text = String::from_utf8_lossy(bytes);
    if let Ok(v @ Value::Object(_)) = serde_json::from_str::<Value>(text.trim()) {
        return Some(v);
    }
    let start = text.rfind("\n{")?;
    match serde_json::from_str::<Value>(text[start + 1..].trim()) {
        Ok(v @ Value::Object(_)) => Some(v),
        _ => None,
    }
}

/// The last line a tool wrote on stderr, without terminal colours.
fn last_line(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut clean = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            clean.push(c);
        }
    }
    clean.lines().map(str::trim).filter(|l| !l.is_empty()).last().unwrap_or("").to_string()
}

fn needs_sign_in(text: &str) -> bool {
    let t = text.to_lowercase();
    ["log in", "login", "logged in", "authenticat", "unauthorized", "401", "sign in", "signed in", "auth method",
     "invalid api key"]
        .iter()
        .any(|needle| t.contains(needle))
}

// ── The chat ──────────────────────────────────────────────────────────────────

/// One chat turn with `provider` answered through the user's subscription.
pub async fn send(
    runtime: &cli_agents::CliChat,
    chat: &Chat,
    provider: &str,
    _model: &str,
    query: String,
    context: Option<ChatContext>,
) -> Result<ChatReply, String> {
    if let Some(agent) = match provider {
        "anthropic" => Some(cli_agents::Agent::Claude),
        "openai" => Some(cli_agents::Agent::Codex),
        _ => None,
    } {
        return cli_agents::send(runtime, chat, agent, query, context).await;
    }
    let Some((cli, exe)) = installed(provider) else {
        let tool = clis_for(provider).first().map(|c| c.name()).unwrap_or(provider);
        return Err(tf(
            "{tool} is not installed. Install it from Settings → Chat, sign in once in a terminal, then ask again.",
            &[("tool", tool)],
        ));
    };
    let turn = chat.begin(&format!("subscription:{provider}"));
    let reads_file = turn.first && matches!(context, Some(ChatContext::File { .. }));
    let system = chat::system_prompt(!reads_file);
    let prompt = chat_prompt(&turn.history, turn.first, context.as_ref(), &query);
    let policy = if cli == Cli::Gemini && reads_file {
        let dir = settings::local_dir().join("relay");
        platform::ensure_private_dir(&dir).map_err(|e| e.to_string())?;
        let path = dir.join("gemini-no-web.toml");
        std::fs::write(&path, NO_WEB_POLICY).map_err(|e| e.to_string())?;
        Some(path)
    } else { None };
    let request = Request { model: None, system: &system, prompt: &prompt, web: !reads_file, no_web_policy: policy.as_deref() };
    let text = ask(runtime, cli, &exe, &request).await?;
    // The record keeps the file's name, never its contents: later turns search the web.
    let plain = chat::plain_question(turn.first, context.as_ref(), &query);
    chat.commit(&turn, json!({ "role": "user", "content": plain }), json!({ "role": "assistant", "content": text }), &plain, &text);
    Ok(ChatReply { text })
}

/// One chat turn as a single prompt for the tools other than Claude Code: the
/// earlier plain turns, then a text file's contents or the window, then the question.
fn chat_prompt(history: &[Value], first: bool, context: Option<&ChatContext>, query: &str) -> String {
    let mut out = String::new();
    if !history.is_empty() {
        out.push_str("The conversation so far:\n<conversation>\n");
        for turn in history {
            let role = turn.get("role").and_then(Value::as_str).unwrap_or("user");
            let text = turn.get("content").and_then(Value::as_str).unwrap_or("");
            out.push_str(&format!("<{role}>\n{text}\n</{role}>\n"));
        }
        out.push_str("</conversation>\n\nThe user's new message:\n");
    }
    match context.filter(|_| first) {
        Some(ChatContext::File { name, path }) => match inline_text(Path::new(path)) {
            Some(body) => out.push_str(&format!("File: {name}\nFile contents:\n{body}\n\n")),
            None => out.push_str(&format!("File: {name} (its contents can't be read here)\n\n")),
        },
        Some(ChatContext::Window { app_name, title, url }) => {
            out.push_str(&chat::window_line(app_name, title, url.as_deref()));
            out.push_str("\n\n");
        }
        None => {}
    }
    out.push_str(query);
    out
}

/// A text file's contents, up to MAX_INLINE_TEXT; nothing for anything else.
fn inline_text(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAX_INLINE_TEXT {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_INLINE_TEXT + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_INLINE_TEXT { return None; }
    let text = String::from_utf8(bytes).ok()?;
    (!text.contains('\0')).then_some(text)
}

/// Settings → Chat: the switch of `provider` moved.
pub fn set(settings: &mut Settings, provider: &str, on: bool) {
    if clis_for(provider).is_empty() {
        return;
    }
    settings.chat_subscriptions.insert(provider.to_string(), on);
    // The chat can't stay on a provider that no longer answers.
    if !on && subscription_only(provider) && settings.chat_provider == provider {
        settings.chat_provider = chat::ANTHROPIC.into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[OsString]) -> Vec<String> {
        args.iter().map(|a| a.to_string_lossy().into_owned()).collect()
    }

    fn after(args: &[String], flag: &str) -> Option<String> {
        args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1).cloned())
    }

    fn request(web: bool) -> Request<'static> {
        Request { model: None, system: "SYS", prompt: "Hello?", web, no_web_policy: None }
    }

    #[test]
    fn each_tool_only_answers() {
        let gemini = strings(&args(Cli::Gemini, &request(true), true));
        assert!(after(&gemini, "-p").unwrap().starts_with("<instructions>\nSYS\n</instructions>"));
        assert_eq!(after(&gemini, "--approval-mode").as_deref(), Some("plan"));
        assert_eq!(after(&gemini, "--extensions").as_deref(), Some("none"));
        let policy = PathBuf::from("/tmp/no-web.toml");
        let file = Request { no_web_policy: Some(&policy), ..request(false) };
        assert_eq!(after(&strings(&args(Cli::Gemini, &file, true)), "--policy").as_deref(), Some("/tmp/no-web.toml"));
        // Through npm's .cmd shim the prompt goes on stdin.
        assert!(!strings(&args(Cli::Gemini, &request(true), false)).contains(&"-p".to_string()));

        let agy = strings(&args(Cli::Antigravity, &request(true), true));
        assert_eq!(after(&agy, "--mode").as_deref(), Some("plan"));

        let codex = strings(&args(Cli::Codex, &request(true), false));
        assert_eq!(&codex[..2], ["--search", "exec"]);
        assert_eq!(codex.last().map(String::as_str), Some("-"));
        assert_eq!(after(&codex, "--sandbox").as_deref(), Some("read-only"));
        assert!(codex.contains(&"shell_tool".to_string()) && codex.contains(&"--ephemeral".to_string()));
        assert!(!strings(&args(Cli::Codex, &request(false), false)).contains(&"--search".to_string()));

        let cursor = strings(&args(Cli::Cursor, &request(true), true));
        assert_eq!(after(&cursor, "--mode").as_deref(), Some("ask"));
        assert!(!cursor.contains(&"--force".to_string()) && !cursor.contains(&"--yolo".to_string()));

        let copilot = strings(&args(Cli::Copilot, &request(true), true));
        assert_eq!(after(&copilot, "--available-tools").as_deref(), Some("web_fetch"));
        assert!(!copilot.contains(&"--allow-all-tools".to_string()));
        let copilot_file = strings(&args(Cli::Copilot, &request(false), true));
        assert_eq!(after(&copilot_file, "--available-tools").as_deref(), Some("view"));
        assert!(!copilot_file.contains(&"--allow-all-urls".to_string()));

        let model = Request { model: Some("m1"), ..request(true) };
        assert_eq!(after(&strings(&args(Cli::Cursor, &model, true)), "--model").as_deref(), Some("m1"));
        let empty = Request { model: Some(""), ..request(true) };
        assert!(!strings(&args(Cli::Cursor, &empty, true)).contains(&"--model".to_string()));
    }

    #[test]
    fn the_prompt_goes_on_stdin_through_a_cmd_shim() {
        assert!(prompt_on_stdin(Cli::Codex, Path::new("codex")));
        assert!(!prompt_on_stdin(Cli::Antigravity, Path::new("agy.exe")));
        assert!(prompt_on_stdin(Cli::Gemini, Path::new(r"C:\npm\gemini.cmd")));
        assert!(!prompt_on_stdin(Cli::Gemini, Path::new("/usr/local/bin/gemini")));
        assert!(prompt_on_stdin(Cli::Copilot, Path::new("copilot.CMD")));
    }

    #[test]
    fn answers_come_from_each_format() {
        assert_eq!(parse(Cli::Cursor, b"warming up\n{\"result\":\"Yo\",\"is_error\":false}", b"", true).unwrap(), "Yo");
        assert_eq!(parse(Cli::Gemini, br#"{"response":"Bonjour","stats":{}}"#, b"", true).unwrap(), "Bonjour");
        assert_eq!(parse(Cli::Antigravity, br#"{"status":"SUCCESS","response":"Hey"}"#, b"", true).unwrap(), "Hey");
        let events = b"{\"type\":\"thread.started\"}\n{\"type\":\"item.completed\",\"item\":{\"type\":\"reasoning\",\"text\":\"x\"}}\n{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"Final\"}}\n";
        assert_eq!(parse(Cli::Codex, events, b"", true).unwrap(), "Final");
        assert_eq!(parse(Cli::Copilot, b"  The answer\n", b"", true).unwrap(), "The answer");
    }

    #[test]
    fn failures_say_why_and_how_to_sign_in() {
        let quota = br#"{"error":{"type":"Error","message":"Quota exceeded"}}"#;
        assert_eq!(parse(Cli::Gemini, b"", quota, false).unwrap_err(), "Quota exceeded");
        let auth = br#"{"error":{"message":"Please set an Auth method in your settings.json"}}"#;
        assert!(parse(Cli::Gemini, b"", auth, false).unwrap_err().ends_with("gemini"));
        let codex = br#"{"type":"error","message":"unexpected status 401 Unauthorized"}"#;
        assert!(parse(Cli::Codex, codex, b"", false).unwrap_err().ends_with("codex login"));
        let crash = b"\x1b[31mboom: model not found\x1b[0m\n";
        assert_eq!(parse(Cli::Copilot, b"", crash, false).unwrap_err(), "Copilot CLI gave no answer. (boom: model not found)");
        assert_eq!(parse(Cli::Cursor, br#"{"is_error":true,"result":"Usage limit"}"#, b"", false).unwrap_err(), "Usage limit");
    }

    #[test]
    fn partial_output_never_hides_a_process_or_provider_failure() {
        assert!(parse(Cli::Cursor, br#"{"result":"partial","is_error":false}"#, b"process failed", false).is_err());
        assert_eq!(parse(Cli::Gemini, br#"{"response":"partial","error":{"message":"Quota exceeded"}}"#, b"", true).unwrap_err(), "Quota exceeded");
        assert_eq!(parse(Cli::Antigravity, br#"{"response":"partial"}"#, br#"{"error":{"message":"Turn failed"}}"#, true).unwrap_err(), "Turn failed");
    }

    #[test]
    fn an_upgrade_does_not_change_api_billing_to_a_subscription() {
        let mut settings = Settings::default();
        for provider in PROVIDERS { assert!(!uses_subscription(&settings, provider)); }
        set(&mut settings, "anthropic", true);
        assert!(uses_subscription(&settings, "anthropic"));
        assert!(!uses_subscription(&settings, "openai"));
        set(&mut settings, "openai", true);
        assert!(uses_subscription(&settings, "openai"));
        set(&mut settings, "anthropic", false);
        assert!(!uses_subscription(&settings, "anthropic"));
        assert!(uses_subscription(&settings, "openai"));
    }

    #[test]
    fn a_file_goes_in_the_prompt_but_not_in_the_history() {
        let dir = std::env::temp_dir().join(format!("coucou-cli-relay-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("notes.txt");
        std::fs::write(&path, "secret plan").unwrap();
        let file = ChatContext::File { name: "notes.txt".into(), path: path.to_string_lossy().into_owned() };
        let p = chat_prompt(&[], true, Some(&file), "Sum it up");
        assert!(p.starts_with("File: notes.txt\nFile contents:\nsecret plan\n\n") && p.ends_with("Sum it up"));
        assert!(!chat::plain_question(true, Some(&file), "Sum it up").contains("secret plan"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_switch_defaults_and_choices() {
        let mut s = Settings::default();
        // Ollama has no subscription; a choice sticks; turning Cursor off leaves its chat.
        assert!(!uses_subscription(&s, "ollama"));
        set(&mut s, "google", true);
        assert!(uses_subscription(&s, "google"));
        set(&mut s, "google", false);
        assert!(!uses_subscription(&s, "google"));
        s.chat_provider = "cursor".into();
        set(&mut s, "cursor", false);
        assert_eq!(s.chat_provider, "anthropic");
        assert_eq!(status(&s).len(), PROVIDERS.len());
    }
}
