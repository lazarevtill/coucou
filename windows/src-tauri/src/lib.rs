// Coucou for Windows — app wiring and the commands the island calls.

mod claude;
mod files;
mod flyout;
mod hooks;
mod host;
mod ide_lock;
mod integrations;
mod island;
mod launch;
mod llm_server;
mod log;
mod open;
mod pipe;
mod plugins;
mod platform;
mod secrets;
mod sessions;
mod settings;
mod shell;
mod tray;
#[cfg(test)]
mod test_http;

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::{ManagerExt, MacosLauncher};

use claude::{Chat, ChatContext, ChatReply};
use files::DroppedFile;
use hooks::{HookPreview, HookStatus};
use island::{PollGate, ScreenInfo};
use pipe::Pending;
use sessions::Sessions;
use settings::Settings;

pub struct Shared {
    pub settings: Mutex<Settings>,
    pub gate: Arc<PollGate>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootInfo {
    settings: Settings,
    screen: ScreenInfo,
    version: String,
    hook_path: String,
    /// False where the OS has no global cursor (Wayland): the page then reports
    /// the cursor from its own mouse events.
    cursor_poll: bool,
}

#[tauri::command]
fn boot(app: AppHandle, shared: State<Shared>) -> BootInfo {
    let mut settings = shared.settings.lock().unwrap().clone();
    // The real state of ~/.claude/settings.json wins over whatever we stored.
    settings.hooks_installed = hooks::status().installed;
    let screen = island::screen_info(&app, &settings.screen);
    BootInfo {
        settings,
        screen,
        version: env!("CARGO_PKG_VERSION").to_string(),
        hook_path: settings::hook_exe_path().to_string_lossy().to_string(),
        cursor_poll: platform::CURSOR_POLL,
    }
}

#[tauri::command]
fn save_settings(app: AppHandle, shared: State<Shared>, settings: Settings) {
    let (screen_changed, autostart_changed) = {
        let mut current = shared.settings.lock().unwrap();
        let screen_changed = current.screen != settings.screen;
        let autostart_changed = current.autostart != settings.autostart;
        *current = settings.clone();
        (screen_changed, autostart_changed)
    };
    if let Err(err) = settings::save(&settings) {
        eprintln!("[coucou] could not save settings: {err}");
    }
    if autostart_changed {
        let manager = app.autolaunch();
        let result = if settings.autostart { manager.enable() } else { manager.disable() };
        if let Err(err) = result {
            eprintln!("[coucou] autostart: {err}");
        }
    }
    if screen_changed {
        let collapsed = shared.gate.collapsed.load(Ordering::Relaxed);
        island::apply_geometry(&app, &settings.screen, collapsed);
    }
    // Keep the other window in step (island ⇄ settings window).
    let _ = app.emit("settings-changed", settings);
}

/// Hidden island → shrink the window to the invisible wake strip and park the
/// cursor poll; anything else → full panel and 60 Hz polling.
///
/// `interactive: false` (island switched off) makes the collapsed strip let every
/// click through: nothing wakes the island from the top of the screen then.
#[tauri::command]
fn set_collapsed(app: AppHandle, shared: State<Shared>, collapsed: bool, interactive: Option<bool>) {
    let pref = shared.settings.lock().unwrap().screen.clone();
    shared.gate.collapsed.store(collapsed, Ordering::Relaxed);
    island::apply_geometry(&app, &pref, collapsed);
    // The wake strip must take the mouse, and a resize invalidates the flag.
    island::refresh_click_through(&app, &shared.gate);
    if collapsed && interactive == Some(false) {
        island::set_ignore_cursor(&app, true);
    }
    shared.gate.set_active(!collapsed);
}

/// The front end pushes the island shape; Rust decides click-through from it.
#[tauri::command]
fn set_island_rect(app: AppHandle, shared: State<Shared>, x: f64, y: f64, width: f64, height: f64) {
    shared.gate.set_rect(island::IslandRect { x, y, w: width, h: height });
    // Without the cursor poll the input region is the click-through: it follows the island.
    if !platform::CURSOR_POLL {
        island::refresh_click_through(&app, &shared.gate);
    }
}

#[tauri::command]
fn focus_window(app: AppHandle, focused: bool) {
    let Some(win) = island::window(&app) else { return };
    platform::set_activating(&win, focused);
    if focused {
        let _ = win.set_focus();
    }
}

#[tauri::command]
fn reposition(app: AppHandle, shared: State<Shared>) {
    let pref = shared.settings.lock().unwrap().screen.clone();
    let collapsed = shared.gate.collapsed.load(Ordering::Relaxed);
    island::apply_geometry(&app, &pref, collapsed);
}

#[tauri::command]
fn open_url(url: String) {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return;
    }
    platform::open_url(&url);
}

/// The island's two buttons: "Open in <editor>" and "Open terminal here".
/// `target` is `editor` or `terminal`; the choices come from the settings, and
/// the system file manager is the fallback. See open.rs for the rules. The folder
/// arrives in a hook payload, so only an absolute, existing directory goes any
/// further (see launch::validate_folder).
#[tauri::command]
fn open_project(shared: State<Shared>, target: String, path: Option<String>) -> open::OpenResult {
    let Some(target) = open::Target::parse(&target) else {
        return open::OpenResult { via: "none".into(), fell_back: false, error: Some("unknown target".into()) };
    };
    let (editor, terminal) = {
        let s = shared.settings.lock().unwrap();
        (launch::Editor::from_setting(&s.editor), launch::Terminal::from_setting(&s.terminal))
    };
    let result = open::open_project(&open::RealHost, editor, terminal, target, path.as_deref());
    log::line(format!("open {target:?} -> {} (fallback {})", result.via, result.fell_back));
    result
}

/// What the settings window shows next to the editor and terminal choices.
#[tauri::command]
fn launch_info() -> launch::LaunchInfo {
    launch::describe(&launch::SystemEnv)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FocusResult {
    /// `focused`, `focusedUnsure`, `flashed`, `noWindow` or `unknownSession`.
    outcome: &'static str,
}

/// Brings the window a session lives in to the front; says how it went.
fn jump(sessions: &Sessions, session_id: &str) -> &'static str {
    let Some(record) = sessions.get(session_id) else {
        return "unknownSession";
    };
    let target = sessions::focus_target(&record);
    let (outcome, method) = platform::focus_window(target.window, &target.candidates, &target.hints);
    // The host and how it went — no folder names, no ids.
    log::line(format!("jump {} -> {outcome:?} ({method:?})", record.host.label));
    match outcome {
        platform::FocusOutcome::Focused => "focused",
        platform::FocusOutcome::FocusedUnsure => "focusedUnsure",
        platform::FocusOutcome::Flashed => "flashed",
        platform::FocusOutcome::NoWindow => "noWindow",
    }
}

/// Click on a session: bring the window it lives in to the front.
#[tauri::command]
fn focus_session(sessions: State<Sessions>, session_id: String) -> FocusResult {
    FocusResult { outcome: jump(&sessions, &session_id) }
}

// ── Tray, flyout and notifications ────────────────────────────────────────────

/// The last snapshot the island published, for a flyout that opens after it.
#[derive(Default)]
pub struct ShellSnapshot(Mutex<Option<serde_json::Value>>);

/// The island publishes what the tray icon and the flyout show. It stays the one
/// owner of the sessions; everything else draws this.
#[tauri::command]
fn publish_shell(app: AppHandle, last: State<ShellSnapshot>, snapshot: serde_json::Value) {
    let text = |key: &str| snapshot.get(key).and_then(serde_json::Value::as_str).unwrap_or("");
    tray::show_state(&app, shell::TrayState::parse(text("trayState")), text("tooltip"));
    *last.0.lock().unwrap() = Some(snapshot.clone());
    let _ = app.emit_to(flyout::LABEL, "shell-snapshot", snapshot);
}

#[tauri::command]
fn shell_snapshot(last: State<ShellSnapshot>) -> Option<serde_json::Value> {
    last.0.lock().unwrap().clone()
}

/// A click in the flyout, handed to the island, which owns what it acts on.
#[tauri::command]
fn shell_action(app: AppHandle, action: serde_json::Value) -> Result<(), String> {
    const KINDS: &[&str] = &["decide", "seen", "pause", "open"];
    let kind = action.get("kind").and_then(serde_json::Value::as_str).unwrap_or("");
    if !KINDS.contains(&kind) {
        return Err(format!("unknown action {kind:?}"));
    }
    app.emit_to(island::WINDOW_LABEL, "shell-action", action).map_err(|e| e.to_string())
}

#[tauri::command]
fn flyout_show(app: AppHandle) {
    flyout::show(&app);
}

#[tauri::command]
fn flyout_hide(app: AppHandle) {
    flyout::hide(&app);
}

#[tauri::command]
fn flyout_visible(app: AppHandle) -> bool {
    flyout::is_visible(&app)
}

/// Starts toast notifications the first time one is needed: that is when the
/// app identity gets registered, not at every launch.
fn ensure_toasts(app: &AppHandle) {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        let icon = settings::local_dir().join("coucou.png");
        if !icon.exists() {
            let _ = platform::ensure_private_dir(&settings::local_dir());
            let _ = std::fs::write(&icon, include_bytes!("../icons/128x128.png"));
        }
        let handle = app.clone();
        platform::toasts_start(
            app.config().identifier.clone(),
            "Coucou".into(),
            icon,
            Box::new(move |activation| {
                let app = handle.clone();
                let _ = handle.run_on_main_thread(move || match activation {
                    shell::Activation::Jump(id) => {
                        let outcome = jump(&app.state::<Sessions>(), &id);
                        let _ = app.emit_to(island::WINDOW_LABEL, "shell-action", serde_json::json!({ "kind": "seen", "id": id }));
                        // Nowhere to go: show the session in the flyout instead.
                        if outcome != "focused" && outcome != "focusedUnsure" {
                            flyout::show(&app);
                        }
                    }
                    shell::Activation::Open => flyout::show(&app),
                });
            }),
        );
    });
}

#[tauri::command]
fn show_toast(app: AppHandle, spec: shell::ToastSpec) -> Result<(), String> {
    ensure_toasts(&app);
    platform::toast_show(spec)
}

#[tauri::command]
fn clear_toast(tag: String) {
    platform::toast_clear(tag);
}

/// Every 20 s, forget sessions whose Claude Code process has exited (a crash or a
/// closed terminal never says SessionEnd) and tell the island so the row goes.
fn spawn_session_pruner(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(20));
        loop {
            ticker.tick().await;
            let sessions = app.state::<Sessions>();
            if sessions.len() == 0 {
                continue;
            }
            let gone = sessions.prune(std::time::Instant::now(), &|entry| {
                platform::process_alive(entry.pid, &entry.exe)
            });
            for id in gone {
                let _ = app.emit_to(island::WINDOW_LABEL, "session-ended", id);
            }
        }
    });
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// Tray → Pause. Paused means paused: the pollers stop talking to the network,
/// not just the island stopping showing things.
#[tauri::command]
fn set_paused(paused: bool) {
    integrations::set_paused(paused);
}

// ── Claude Code hooks ─────────────────────────────────────────────────────────

#[tauri::command]
fn hooks_status() -> HookStatus {
    hooks::status()
}

/// Returns the diff the user has to look at before anything is written.
#[tauri::command]
fn hooks_preview(install: bool) -> Result<HookPreview, String> {
    hooks::preview(install)
}

/// Only ever called from an explicit click in the settings window.
#[tauri::command]
fn hooks_apply(
    app: AppHandle,
    shared: State<Shared>,
    install: bool,
    fingerprint: String,
) -> Result<String, String> {
    // The fingerprint comes from the preview the user actually looked at, so a
    // settings.json that changed in between is refused rather than overwritten.
    let backup = hooks::write(install, &fingerprint)?;
    let updated = {
        let mut current = shared.settings.lock().unwrap();
        current.hooks_installed = install;
        let _ = settings::save(&current);
        current.clone()
    };
    let _ = app.emit("settings-changed", updated);
    Ok(backup)
}

#[tauri::command]
fn approval_decision(app: AppHandle, request_id: String, decision: String) {
    pipe::answer(&app, &request_id, &decision);
}

/// The island has the card on screen, so the long wait for a human may begin.
/// Until this arrives the relay only waits a few hundred milliseconds, which is
/// what stops a paused or unresponsive island from freezing Claude Code.
#[tauri::command]
fn approval_ack(app: AppHandle, request_id: String) {
    pipe::acknowledge(&app, &request_id);
}

/// Nobody can act on this request — the island is paused, or another card is
/// already up. Claude Code falls back to asking in the terminal immediately.
#[tauri::command]
fn approval_decline(app: AppHandle, request_id: String) {
    pipe::decline(&app, &request_id);
}

// ── Chat, files and secrets ───────────────────────────────────────────────────

/// One chat turn, with whichever source the settings name. The API keys and
/// any file bytes stay on the Rust side.
#[tauri::command]
async fn chat_send(
    shared: State<'_, Shared>,
    chat: State<'_, Chat>,
    local: State<'_, llm_server::LlmChat>,
    query: String,
    context: Option<ChatContext>,
) -> Result<ChatReply, String> {
    let (provider, model, url, server_model) = {
        let s = shared.settings.lock().unwrap();
        (s.chat_provider.clone(), s.model.clone(), s.llm_server_url.clone(), s.llm_server_model.clone())
    };
    if provider == "llmServer" {
        let cfg = llm_server::ServerConfig {
            base: llm_server::check_base_url(&url)?,
            model: server_model,
            key: secrets::get("llm-server-api-key"),
        };
        return llm_server::send(&local, &cfg, query, context).await;
    }
    claude::send(&chat, &model, query, context).await
}

/// A new conversation, with either source.
#[tauri::command]
fn chat_reset(chat: State<Chat>, local: State<llm_server::LlmChat>) {
    chat.reset();
    local.reset();
}

// ── Plugins ───────────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginSecret {
    key: String,
    label: String,
    present: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginTool {
    name: String,
    access: plugins::manifest::Access,
}

/// Everything the settings window shows about a plugin before it is trusted.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginInfo {
    id: String,
    approval: plugins::registry::Approval,
    /// What an approval is pinned to.
    hash: String,
    error: Option<String>,
    name: Option<String>,
    version: Option<String>,
    /// `http`, `mcp-stdio` or `mcp-http`.
    kind: Option<String>,
    color: Option<String>,
    hosts: Vec<String>,
    allow_private_network: bool,
    secrets: Vec<PluginSecret>,
    /// The address it reads, or the exact command line it starts.
    runs: Option<String>,
    poll_secs: Option<u64>,
    poll_tool: Option<String>,
    tools: Vec<PluginTool>,
    links: Vec<plugins::manifest::Link>,
}

fn quote_arg(a: &str) -> String {
    if a.is_empty() || a.contains([' ', '\t', '"']) {
        format!("\"{}\"", a.replace('"', "\\\""))
    } else {
        a.to_string()
    }
}

fn plugin_info(p: &plugins::registry::Installed, setting: Option<&settings::PluginSetting>) -> PluginInfo {
    use plugins::manifest::{McpTransport, Source};
    let approval = plugins::registry::approval(p, setting);
    let mut info = PluginInfo {
        id: p.id.clone(),
        approval,
        hash: p.hash.clone(),
        error: p.manifest.as_ref().err().cloned(),
        name: None,
        version: None,
        kind: None,
        color: None,
        hosts: Vec::new(),
        allow_private_network: false,
        secrets: Vec::new(),
        runs: None,
        poll_secs: None,
        poll_tool: None,
        tools: Vec::new(),
        links: Vec::new(),
    };
    let Ok(m) = &p.manifest else { return info };
    info.name = Some(m.name.clone());
    info.version = Some(m.version.clone());
    info.color = Some(m.color.clone());
    info.hosts = m.allowed_hosts.clone();
    info.allow_private_network = m.allow_private_network;
    info.poll_secs = Some(m.poll_secs);
    info.links = m.links.clone();
    info.secrets = m
        .secrets
        .iter()
        .map(|s| PluginSecret {
            key: s.key.clone(),
            label: s.label.clone(),
            present: secrets::present(&secrets::plugin_key(&m.id, &s.key)),
        })
        .collect();
    match &m.source {
        Source::Http { url, .. } => {
            info.kind = Some("http".into());
            info.runs = Some(url.clone());
        }
        Source::Mcp(mcp) => {
            match &mcp.transport {
                McpTransport::Stdio { command, args, .. } => {
                    info.kind = Some("mcp-stdio".into());
                    let line: Vec<String> = std::iter::once(command.as_str()).chain(args.iter().map(String::as_str)).map(quote_arg).collect();
                    info.runs = Some(line.join(" "));
                }
                McpTransport::Http { url, .. } => {
                    info.kind = Some("mcp-http".into());
                    info.runs = Some(url.clone());
                }
            }
            info.poll_tool = mcp.poll.as_ref().map(|p| p.tool.clone());
            info.tools = mcp.tools.iter().map(|(name, access)| PluginTool { name: name.clone(), access: *access }).collect();
        }
    }
    info
}

/// The plugins in the folder, as they are now.
#[tauri::command]
fn plugins_list(shared: State<Shared>) -> Vec<PluginInfo> {
    let settings = shared.settings.lock().unwrap().clone();
    plugins::registry::scan(&plugins::registry::dir())
        .iter()
        .map(|p| plugin_info(p, settings.plugins.get(&p.id)))
        .collect()
}

fn save_and_announce(app: &AppHandle, shared: &Shared, change: impl FnOnce(&mut Settings)) -> Result<(), String> {
    let updated = {
        let mut s = shared.settings.lock().unwrap();
        change(&mut s);
        settings::save(&s).map_err(|e| e.to_string())?;
        s.clone()
    };
    let _ = app.emit("settings-changed", updated);
    plugins::runner::refresh(app);
    Ok(())
}

/// Trusts a plugin — exactly the manifest the user reviewed, identified by
/// its hash. A manifest that changed in between is refused.
#[tauri::command]
fn plugin_enable(app: AppHandle, shared: State<Shared>, id: String, hash: String) -> Result<(), String> {
    let installed = plugins::registry::scan(&plugins::registry::dir());
    let p = installed.iter().find(|p| p.id == id).ok_or("That plugin is not in the folder any more.")?;
    if let Err(e) = &p.manifest {
        return Err(e.clone());
    }
    if p.hash != hash {
        return Err("The plugin changed while you were reviewing it. Look at it again.".into());
    }
    log::line(format!("plugin {id} approved"));
    save_and_announce(&app, &shared, |s| {
        s.plugins.insert(id.clone(), settings::PluginSetting { enabled: true, approved_hash: Some(hash.clone()) });
    })
}

#[tauri::command]
fn plugin_disable(app: AppHandle, shared: State<Shared>, id: String) -> Result<(), String> {
    save_and_announce(&app, &shared, |s| {
        if let Some(p) = s.plugins.get_mut(&id) {
            p.enabled = false;
        }
    })
}

/// The tools an approved MCP plugin's server offers. Never before approval:
/// listing them means starting the server.
#[tauri::command]
async fn plugin_tools(shared: State<'_, Shared>, id: String) -> Result<Vec<plugins::mcp::proto::ToolInfo>, String> {
    let setting = shared.settings.lock().unwrap().plugins.get(&id).cloned();
    let installed = plugins::registry::scan(&plugins::registry::dir());
    let p = installed.iter().find(|p| p.id == id).ok_or("That plugin is not in the folder any more.")?;
    if plugins::registry::approval(p, setting.as_ref()) != plugins::registry::Approval::On {
        return Err("Switch the plugin on first: listing its tools starts its server.".into());
    }
    let m = p.manifest.clone()?;
    let key_id = m.id.clone();
    let secret = move |key: &str| secrets::get(&secrets::plugin_key(&key_id, key));
    plugins::mcp::list_tools(&m, &secret).await
}

#[tauri::command]
fn plugins_open_folder() {
    let dir = plugins::registry::dir();
    let _ = platform::ensure_private_dir(&dir);
    platform::reveal_folder(&dir.to_string_lossy());
}

/// The models a server offers — also how the settings window tests an address
/// before it is saved.
#[tauri::command]
async fn llm_server_models(url: String) -> Result<Vec<String>, String> {
    let cfg = llm_server::ServerConfig {
        base: llm_server::check_base_url(&url)?,
        model: String::new(),
        key: secrets::get("llm-server-api-key"),
    };
    llm_server::list_models(&cfg).await
}

/// Copies a dropped file into the inbox and reports its name back.
#[tauri::command]
fn ingest_file(path: String) -> Result<DroppedFile, String> {
    files::ingest(&path)
}

/// The island may only ask whether a key exists — never read it.
#[tauri::command]
fn secret_present(key: String) -> bool {
    secrets::present(&key)
}

#[tauri::command]
fn secret_set(key: String, value: String) -> Result<(), String> {
    secrets::set(&key, &value)
}

#[tauri::command]
fn secret_clear(key: String) -> Result<(), String> {
    secrets::clear(&key)
}

/// Opens the configured n8n instance — the URL lives in the Credential Manager.
#[tauri::command]
fn open_n8n() {
    if let Some(url) = secrets::get("n8n-url") {
        open_url(url);
    }
}

/// Refresh buttons in the integration cards.
#[tauri::command]
async fn refresh_integration(app: AppHandle, id: String) {
    integrations::poll_once(app, &id).await;
}

/// Lets the island write to the same log as the Rust side.
#[tauri::command]
fn log_line(message: String) {
    log::line(format!("ui  {message}"));
}

// ── Settings window ───────────────────────────────────────────────────────────

/// WebView2 allows exactly one browser environment per app, and its options are
/// fixed by whichever webview is created first. Every window must therefore ask
/// for the *same* arguments as the island (see `additionalBrowserArgs` in
/// tauri.conf.json) — a mismatch makes the second window come up blank, with no
/// error anywhere.
const BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required";

/// In a dev build the pages are served by Vite, so the second window needs the
/// absolute dev URL; a bundled build resolves it inside the app bundle.
fn settings_page_url(app: &AppHandle) -> WebviewUrl {
    #[cfg(dev)]
    if let Some(mut base) = app.config().build.dev_url.clone() {
        base.set_path("/settings.html");
        return WebviewUrl::External(base);
    }
    let _ = app;
    WebviewUrl::App("settings.html".into())
}

/// The settings window is created hidden at launch and only ever shown and
/// hidden afterwards. A WebView2 window created later — on the main thread or
/// not — silently comes up blank in this app, so the window that works is the
/// one that exists before the island's webview does. Not focused when created:
/// a hidden window must not take the keyboard from whatever you are typing in.
fn create_settings_window(app: &AppHandle) {
    let url = settings_page_url(app);
    match WebviewWindowBuilder::new(app, "settings", url)
        .additional_browser_args(BROWSER_ARGS)
        .focused(false)
        .title("Settings — Coucou")
        .inner_size(560.0, 680.0)
        .min_inner_size(460.0, 480.0)
        .resizable(true)
        .visible(false)
        .center()
        .build()
    {
        Ok(win) => {
            // Closing it must only hide it, or it could never be reopened.
            let hidden = win.clone();
            win.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = hidden.hide();
                }
            });
        }
        Err(err) => log::line(format!("settings window failed: {err}")),
    }
}

pub fn show_settings_window(app: &AppHandle) {
    let Some(win) = app.get_webview_window("settings") else {
        log::line("settings window missing");
        return;
    };
    let _ = win.unminimize();
    let _ = win.show();
    let _ = win.set_focus();
}

#[tauri::command]
fn open_settings_window(app: AppHandle) {
    show_settings_window(&app);
}

pub fn run() {
    platform::prepare_environment();
    let loaded = settings::load();
    let gate = Arc::new(PollGate::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            let _ = app.emit_to(island::WINDOW_LABEL, "tray", "open".to_string());
        }))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .manage(Shared {
            settings: Mutex::new(loaded.clone()),
            gate: gate.clone(),
        })
        .manage(Pending::default())
        .manage(Sessions::default())
        .manage(ShellSnapshot::default())
        .manage(flyout::Flyout::default())
        .manage(plugins::runner::Supervisor::default())
        .manage(Chat::default())
        .manage(llm_server::LlmChat::default())
        .invoke_handler(tauri::generate_handler![
            boot,
            save_settings,
            set_collapsed,
            set_island_rect,
            focus_window,
            reposition,
            open_url,
            open_project,
            launch_info,
            focus_session,
            publish_shell,
            shell_snapshot,
            shell_action,
            flyout_show,
            flyout_hide,
            flyout_visible,
            show_toast,
            clear_toast,
            quit_app,
            hooks_status,
            hooks_preview,
            hooks_apply,
            approval_decision,
            approval_ack,
            approval_decline,
            log_line,
            chat_send,
            chat_reset,
            llm_server_models,
            plugins_list,
            plugin_enable,
            plugin_disable,
            plugin_tools,
            plugins_open_folder,
            ingest_file,
            secret_present,
            secret_set,
            secret_clear,
            refresh_integration,
            open_n8n,
            open_settings_window,
            set_paused,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            tray::build(&handle)?;
            // Before the island: see create_settings_window.
            create_settings_window(&handle);
            flyout::create(&handle, BROWSER_ARGS);

            if let Some(win) = island::window(&handle) {
                platform::make_non_activating(&win);
                island::apply_geometry(&handle, &loaded.screen, false);
                let _ = win.show();
            }
            gate.collapsed.store(false, Ordering::Relaxed);
            // Nothing drawn yet, so nothing takes the mouse until the page
            // reports the island's shape.
            if !platform::CURSOR_POLL {
                island::refresh_click_through(&handle, &gate);
            }
            gate.set_active(true);
            island::spawn_cursor_poll(handle.clone(), gate.clone());

            log::line(format!("--- Coucou {} started ---", env!("CARGO_PKG_VERSION")));
            hooks::ensure_hook_exe(&handle);
            pipe::start(handle.clone());
            spawn_session_pruner(handle.clone());
            integrations::start(handle.clone());
            plugins::runner::start(handle.clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Coucou");
}
