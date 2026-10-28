//! Official CLI sign-ins, chat, and interactive project sessions.
//! Credentials remain with each CLI. Prompts travel over stdin, never argv.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::State;

use crate::{chat, platform};
use chat::{Chat, ChatContext, ChatReply, ModelInfo};

const MAX_OUTPUT: usize = 8 * 1024 * 1024;
const MAX_PROMPT: usize = 1024 * 1024;
const STATUS_TIMEOUT: Duration = Duration::from_secs(15);
const CHAT_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Agent { Claude, Codex }

impl Agent {
    pub fn parse(id: &str) -> Result<Self, String> {
        match id {
            "claude-code" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            _ => Err("Choose Claude Code or Codex.".into()),
        }
    }

    fn id(self) -> &'static str {
        match self { Self::Claude => "claude-code", Self::Codex => "codex" }
    }

    fn name(self) -> &'static str {
        match self { Self::Claude => "Claude Code", Self::Codex => "Codex" }
    }

    fn executable(self) -> Option<PathBuf> {
        let (setting, stem) = match self {
            Self::Claude => ("COUCOU_CLAUDE_BIN", "claude"),
            Self::Codex => ("COUCOU_CODEX_BIN", "codex"),
        };
        if let Some(path) = std::env::var_os(setting) {
            let path = PathBuf::from(path);
            return (path.is_absolute() && path.is_file()).then_some(path);
        }
        if let Some(path) = platform::find_on_path(stem) { return Some(path); }
        if self == Self::Codex {
            if let Some(path) = platform::codex_candidates().into_iter().next() { return Some(path); }
        }
        let bin = platform::home_dir().join(".local").join("bin");
        let path = bin.join(if cfg!(windows) { format!("{stem}.exe") } else { stem.into() });
        path.is_file().then_some(path)
    }

    fn command(self) -> Result<Command, String> {
        let exe = self.executable().ok_or_else(|| format!("Install {} first, then refresh connections.", self.name()))?;
        let mut cmd = Command::new(exe);
        // This provider deliberately uses the CLI account, not API-key billing.
        for key in ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "OPENAI_API_KEY"] {
            cmd.env_remove(key);
        }
        Ok(cmd)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub id: &'static str,
    pub name: &'static str,
    pub installed: bool,
    pub signed_in: bool,
    pub message: String,
}

/// Only non-secret state reaches the webview; raw CLI output is never returned.
fn signed_in(agent: Agent, success: bool, stdout: &[u8], stderr: &[u8]) -> bool {
    if !success { return false; }
    match agent {
        Agent::Claude => serde_json::from_slice::<Value>(stdout).is_ok_and(|v| {
            v.get("loggedIn").and_then(Value::as_bool) == Some(true)
                && v.get("authMethod").and_then(Value::as_str) == Some("claude.ai")
        }),
        Agent::Codex => [stdout, stderr].iter().any(|bytes| {
            String::from_utf8_lossy(bytes).lines().any(|line| line.trim() == "Logged in using ChatGPT")
        }),
    }
}

fn status(agent: Agent) -> Connection {
    status_with_cancel(agent, &AtomicBool::new(false))
}

fn status_with_cancel(agent: Agent, cancel: &AtomicBool) -> Connection {
    let installed = agent.executable().is_some();
    let result = (|| {
        let mut cmd = agent.command()?;
        match agent {
            Agent::Claude => { cmd.args(["auth", "status", "--json"]); }
            Agent::Codex => { cmd.args(["login", "status"]); }
        }
        capture(cmd, None, STATUS_TIMEOUT, cancel)
    })();
    let authenticated = result.as_ref().is_ok_and(|r| signed_in(agent, r.success, &r.stdout, &r.stderr));
    let message = if authenticated { "Connected with your existing sign-in.".into() }
        else if !installed { format!("Install {} to connect.", agent.name()) }
        else if result.is_err() { "Could not check the CLI. Refresh or open it in a terminal.".into() }
        else { "Sign in with your subscription, then refresh.".into() };
    Connection { id: agent.id(), name: agent.name(), installed, signed_in: authenticated, message }
}

#[tauri::command]
pub async fn agent_connections() -> Result<Vec<Connection>, String> {
    tauri::async_runtime::spawn_blocking(|| vec![status(Agent::Claude), status(Agent::Codex)])
        .await.map_err(|_| "Could not check agent connections.".into())
}

fn interactive_args(agent: Agent, login: bool) -> Vec<&'static str> {
    match (agent, login) {
        (Agent::Claude, true) => vec!["auth", "login"],
        (Agent::Codex, true) => vec!["login"],
        (Agent::Claude, false) => vec!["--permission-mode", "manual"],
        (Agent::Codex, false) => vec!["--ask-for-approval", "on-request", "--sandbox", "workspace-write", "--enable", "hooks"],
    }
}

/// The user explicitly clicks Sign in or Open project. The official CLI owns
/// its browser flow, account storage, tools, context, skills, MCP and approvals.
#[tauri::command]
pub async fn agent_open(agent: String, login: bool, project: Option<String>) -> Result<(), String> {
    let agent = Agent::parse(&agent)?;
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(not(windows))]
        { let _ = (agent, login, project); Err("Open the agent in your terminal on this platform.".into()) }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let cwd = if login { platform::home_dir() } else {
                let folder = project.as_deref().filter(|s| !s.trim().is_empty())
                    .ok_or("Choose a project folder first.")?;
                let path = PathBuf::from(folder.trim());
                if !path.is_absolute() || !path.is_dir() { return Err("Choose an existing, absolute project folder.".into()); }
                path
            };
            let mut cmd = agent.command()?;
            cmd.args(interactive_args(agent, login)).current_dir(cwd);
            cmd.creation_flags(0x0000_0010).spawn()
                .map(|_| ()).map_err(|_| format!("Could not open {}. Check the CLI installation.", agent.name()))
        }
    }).await.map_err(|_| "Could not open the agent.".to_string())?
}

#[derive(Default)]
pub struct CliChat {
    active: std::sync::Mutex<Option<Arc<AtomicBool>>>,
}

impl CliChat {
    pub fn cancel(&self) {
        if let Some(active) = self.active.lock().unwrap().as_ref() { active.store(true, Ordering::Release); }
    }
    fn begin(&self) -> Result<Arc<AtomicBool>, String> {
        let mut active = self.active.lock().unwrap();
        if active.is_some() { return Err("Wait for the current reply or stop it first.".into()); }
        let flag = Arc::new(AtomicBool::new(false));
        *active = Some(flag.clone());
        Ok(flag)
    }
}

#[tauri::command]
pub fn agent_chat_cancel(chat: State<CliChat>) { chat.cancel(); }

struct Finished<'a>(&'a CliChat);
impl Drop for Finished<'_> {
    fn drop(&mut self) { *self.0.active.lock().unwrap() = None; }
}

struct Captured { success: bool, stdout: Vec<u8>, stderr: Vec<u8> }

fn read_bounded(mut stream: impl Read) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    stream.by_ref().take((MAX_OUTPUT + 1) as u64).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// A finite process and finite output. A Windows job ends its descendants too;
/// cancellation and deadlines are checked while it runs, not after output closes.
fn capture(mut cmd: Command, input: Option<String>, timeout: Duration, cancel: &AtomicBool) -> Result<Captured, String> {
    platform::no_console(&mut cmd);
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|_| "Could not start the agent CLI.".to_string())?;
    let job = platform::contain(&child);
    #[cfg(windows)]
    if job.is_none() {
        let _ = child.kill(); let _ = child.wait();
        return Err("Could not supervise the agent process.".into());
    }
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let out = std::thread::spawn(move || read_bounded(stdout));
    let err = std::thread::spawn(move || read_bounded(stderr));
    let writer = std::thread::spawn(move || {
        if let Some(input) = input { stdin.write_all(input.as_bytes())?; }
        Ok::<_, std::io::Error>(())
    });
    let deadline = Instant::now() + timeout;
    let result = loop {
        if cancel.load(Ordering::Acquire) { break Err("Reply stopped.".to_string()); }
        if Instant::now() >= deadline { break Err("The agent took too long. Try again or open it in a terminal.".into()); }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status.success()),
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(_) => break Err("Could not read the agent's exit status.".into()),
        }
    };
    let _ = child.kill();
    let _ = child.wait();
    drop(job);
    let stdout = out.join().ok().and_then(Result::ok).ok_or("Could not read the agent reply.")?;
    let stderr = err.join().ok().and_then(Result::ok).ok_or("Could not read the agent status.")?;
    let written = writer.join().ok().and_then(Result::ok).is_some();
    let success = result?;
    if stdout.len() > MAX_OUTPUT || stderr.len() > MAX_OUTPUT { return Err("The agent reply exceeded the size limit.".into()); }
    if !written { return Err("Could not send the message to the agent.".into()); }
    Ok(Captured { success, stdout, stderr })
}

fn chat_args(agent: Agent) -> Vec<&'static str> {
    match agent {
        Agent::Claude => vec!["--print", "--output-format", "json", "--tools", "", "--strict-mcp-config", "--mcp-config", "{\"mcpServers\":{}}", "--disable-slash-commands", "--no-session-persistence", "--safe-mode", "--permission-mode", "manual"],
        Agent::Codex => vec!["exec", "--json", "--ephemeral", "--ignore-user-config", "--ignore-rules", "--skip-git-repo-check", "--sandbox", "read-only", "--disable", "shell_tool", "--disable", "unified_exec", "--disable", "multi_agent", "--disable", "plugins", "--disable", "hooks", "-c", "web_search=\"disabled\"", "-"],
    }
}

fn answer(agent: Agent, output: Captured) -> Result<String, String> {
    if !output.success { return Err(format!("{} could not complete the reply. Check its sign-in and usage limit in Settings → Agents.", agent.name())); }
    let text = match agent {
        Agent::Claude => {
            let value: Value = serde_json::from_slice(&output.stdout).map_err(|_| "Claude Code returned an unreadable reply.")?;
            // JSON output can contain the complete event array. Only its final
            // result is an answer; an assistant event alone is not completion.
            let result = match value.as_array() {
                Some(events) => events.last().ok_or("Claude Code returned no result.")?,
                None => &value,
            };
            if result.get("type").and_then(Value::as_str) != Some("result")
                || result.get("subtype").and_then(Value::as_str) != Some("success")
                || result.get("is_error").and_then(Value::as_bool) != Some(false) {
                return Err("Claude Code did not complete the reply.".into());
            }
            result.get("result").and_then(Value::as_str).unwrap_or_default().to_string()
        }
        Agent::Codex => {
            let mut text = String::new(); let mut completed = false;
            for line in output.stdout.split(|b| *b == b'\n') {
                let Ok(v) = serde_json::from_slice::<Value>(line) else { continue };
                match v.get("type").and_then(Value::as_str) {
                    Some("turn.completed") => completed = true,
                    Some("turn.failed" | "error") => return Err("Codex did not complete the reply. Open it in a terminal for details.".into()),
                    Some("item.completed") if v.pointer("/item/type").and_then(Value::as_str) == Some("agent_message") => {
                        text = v.pointer("/item/text").and_then(Value::as_str).unwrap_or_default().into();
                    }
                    _ => {}
                }
            }
            if !completed { return Err("Codex ended before completing its reply.".into()); }
            text
        }
    };
    if text.trim().is_empty() { return Err("The agent returned no answer.".into()); }
    Ok(text)
}

fn user_message(query: &str, context: Option<&ChatContext>) -> Result<String, String> {
    let mut text = String::new();
    if let Some(context) = context {
        match context {
            ChatContext::File { name, path } => {
                let bytes = read_file(Path::new(path))?;
                let body = String::from_utf8(bytes).map_err(|_| "CLI chat accepts text files. Use an API provider for images or PDFs.")?;
                text.push_str(&format!("\nAttached file {name}:\n{body}\n"));
            }
            ChatContext::Window { app_name, title, url } => text.push_str(&chat::window_line(app_name, title, url.as_deref())),
        }
    }
    text.push_str(query);
    Ok(text)
}

fn prompt(history: &[Value], message: &str) -> Result<String, String> {
    let mut text = chat::system_prompt(false);
    text.push_str("\nAnswer the latest user message. This chat has no tools; project work is opened separately in the coding agent.\n");
    for turn in history {
        let role = turn.get("role").and_then(Value::as_str).unwrap_or("user");
        let body = turn.get("content").and_then(Value::as_str).unwrap_or_default();
        text.push_str(&format!("\n{role}: {body}\n"));
    }
    text.push_str(&format!("\nuser: {message}\nassistant:"));
    if text.len() > MAX_PROMPT { return Err("This conversation is too large. Start a new chat.".into()); }
    Ok(text)
}

fn read_file(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes=Vec::new();
    std::fs::File::open(path).map_err(|_| "Could not read the attached file.")?
        .take(200_001).read_to_end(&mut bytes).map_err(|_| "Could not read the attached file.")?;
    if bytes.len() > 200_000 { return Err("CLI chat accepts text attachments up to 200 KB.".into()); }
    Ok(bytes)
}

pub async fn send(runtime: &CliChat, chat: &Chat, agent: Agent, query: String, context: Option<ChatContext>) -> Result<ChatReply, String> {
    let cancel = runtime.begin()?;
    let _finished = Finished(runtime);
    let turn = chat.begin(agent.id());
    let context = context.filter(|_| turn.first);
    let message = user_message(&query, context.as_ref())?;
    let input = prompt(&turn.history, &message)?;
    let text = tauri::async_runtime::spawn_blocking(move || {
        let connected = status_with_cancel(agent, &cancel).signed_in;
        if cancel.load(Ordering::Acquire) { return Err("Reply stopped.".into()); }
        if !connected { return Err(format!("Connect {} in Settings → Agents first.", agent.name())); }
        // An empty directory keeps project instructions and tools out of casual chat.
        let folder = crate::settings::local_dir().join("cli-chat");
        platform::ensure_private_dir(&folder).map_err(|_| "Could not prepare the chat folder.")?;
        let mut cmd=agent.command()?;
        cmd.args(chat_args(agent)).current_dir(folder);
        answer(agent, capture(cmd, Some(input), CHAT_TIMEOUT, &cancel)?)
    }).await.map_err(|_| "The agent chat process failed.".to_string())??;
    let plain = chat::plain_question(turn.first, context.as_ref(), &query);
    chat.commit(&turn, json!({"role":"user","content":message}), json!({"role":"assistant","content":text}), &plain, &text);
    Ok(ChatReply { text })
}

pub fn models(agent: Agent) -> Vec<ModelInfo> {
    vec![ModelInfo { id: "default".into(), label: format!("{} default", agent.name()) }]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn connection_requires_the_selected_subscription_and_successful_probe() {
        let claude=br#"{"loggedIn":true,"authMethod":"claude.ai","email":"private@example.com"}"#;
        assert!(signed_in(Agent::Claude,true,claude,b""));
        assert!(!signed_in(Agent::Claude,false,claude,b""));
        assert!(!signed_in(Agent::Claude,true,br#"{"loggedIn":true,"authMethod":"api_key"}"#,b""));
        assert!(signed_in(Agent::Codex,true,b"",b"Logged in using ChatGPT\n"));
        assert!(!signed_in(Agent::Codex,true,b"Logged in using an API key",b""));
    }
    #[test]
    fn a_partial_or_failed_codex_turn_is_never_reported_as_success() {
        let item=r#"{"type":"item.completed","item":{"type":"agent_message","text":"answer"}}"#;
        let captured=|s:String| Captured{success:true,stdout:s.into_bytes(),stderr:vec![]};
        assert!(answer(Agent::Codex,captured(item.into())).is_err());
        assert!(answer(Agent::Codex,captured(format!("{item}\n{{\"type\":\"turn.failed\"}}"))).is_err());
        assert_eq!(answer(Agent::Codex,captured(format!("{item}\n{{\"type\":\"turn.completed\"}}"))).unwrap(),"answer");
    }
    #[test]
    fn claude_requires_a_successful_final_result_in_either_json_envelope() {
        let complete = json!({"type":"result","subtype":"success","is_error":false,"result":"answer"});
        let captured = |v: Value| Captured {success:true,stdout:serde_json::to_vec(&v).unwrap(),stderr:vec![]};
        assert_eq!(answer(Agent::Claude, captured(complete.clone())).unwrap(), "answer");
        assert_eq!(answer(Agent::Claude, captured(json!([{"type":"assistant"}, complete]))).unwrap(), "answer");
        for v in [json!([]), json!([{"type":"assistant","result":"partial"}]),
            json!([{"type":"result","subtype":"error_max_turns","is_error":true,"result":"partial"}]),
            json!({"type":"result","subtype":"success","is_error":false,"result":""})] {
            assert!(answer(Agent::Claude, captured(v)).is_err());
        }
    }

    #[test]
    fn followup_messages_retain_the_attached_text_without_repeating_the_transcript() {
        let path = std::env::temp_dir().join(format!("coucou-context-{}.txt", std::process::id()));
        std::fs::write(&path, "the project is called Finch").unwrap();
        let context = ChatContext::File { name: "notes.txt".into(), path: path.to_string_lossy().into() };
        let message = user_message("Read this", Some(&context)).unwrap();
        let history = vec![json!({"role":"user","content":message}), json!({"role":"assistant","content":"Read."})];
        let input = prompt(&history, "What is the project called?").unwrap();
        assert_eq!(input.matches("the project is called Finch").count(), 1);
        assert!(input.contains("What is the project called?"));
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn interactive_sessions_keep_manual_approval_and_chat_cannot_run_shell_tools() {
        assert_eq!(interactive_args(Agent::Claude,false),["--permission-mode","manual"]);
        assert!(interactive_args(Agent::Codex,false).contains(&"on-request"));
        assert!(chat_args(Agent::Codex).windows(2).any(|p|p==["--disable","shell_tool"]));
        assert!(chat_args(Agent::Claude).windows(2).any(|p|p==["--tools",""]));
        for agent in [Agent::Claude,Agent::Codex] {
            assert!(!interactive_args(agent,false).iter().any(|s|s.contains("bypass")));
            assert!(!chat_args(agent).iter().any(|s|s.contains("bypass")));
        }
    }

    #[test]
    fn a_process_that_never_replies_is_stopped_at_the_deadline() {
        let mut cmd = Command::new("node");
        cmd.args(["-e", "setInterval(() => {}, 1000)"]);
        let start = Instant::now();
        let result = capture(cmd, None, Duration::from_millis(150), &AtomicBool::new(false));
        assert!(result.is_err_and(|e| e.contains("too long")));
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn cancellation_stops_the_process_and_allows_another_turn() {
        let runtime = CliChat::default();
        let cancel = runtime.begin().unwrap();
        assert!(runtime.begin().is_err());
        let stopped = cancel.clone();
        let trigger = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            stopped.store(true, Ordering::Release);
        });
        let mut cmd = Command::new("node");
        cmd.args(["-e", "setInterval(() => {}, 1000)"]);
        let started = Instant::now();
        let result = capture(cmd, None, Duration::from_secs(10), &cancel);
        trigger.join().unwrap();
        assert!(result.is_err_and(|e| e == "Reply stopped."));
        assert!(started.elapsed() < Duration::from_secs(5));
        drop(Finished(&runtime));
        assert!(runtime.begin().is_ok());
    }

    fn live_smoke(agent: Agent) {
        assert!(status(agent).signed_in, "subscription sign-in is required");
        let folder = std::env::temp_dir().join(format!("coucou-cli-smoke-{}-{}", agent.id(), std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let mut cmd = agent.command().unwrap();
        cmd.args(chat_args(agent)).current_dir(&folder);
        let output = capture(cmd, Some("Reply with exactly COUCOU_CLI_OK.".into()), CHAT_TIMEOUT, &AtomicBool::new(false)).unwrap();
        let reply = answer(agent, output).unwrap();
        assert_eq!(reply.trim(), "COUCOU_CLI_OK");
    }

    #[test]
    #[ignore = "explicit live check using an installed Claude subscription"]
    fn live_claude_sign_in_and_reply() { live_smoke(Agent::Claude); }

    #[test]
    #[ignore = "explicit live check using an installed ChatGPT subscription"]
    fn live_codex_sign_in_and_reply() { live_smoke(Agent::Codex); }
}
