// Preferences, stored as plain JSON in settings.json under platform::config_dir().
// No secret ever lands here — API keys live in the OS keychain (see secrets.rs).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub sound_enabled: bool,
    pub sound_volume: f64,
    pub auto_close_interval: f64,
    pub absence_interval: f64,
    pub active_integrations: Vec<String>,
    /// "primary" = the main display, "cursor" = whichever display the mouse is on.
    pub screen: String,
    pub autostart: bool,
    pub hooks_installed: bool,
    /// Claude model used by the chat. Changeable in the settings window.
    /// Defaulted explicitly so a settings.json written by an older build still loads.
    #[serde(default = "default_model")]
    pub model: String,
    /// "Open in…" editor: `cursor`, `vscode` or `none`. Anything else means the default.
    #[serde(default = "default_editor")]
    pub editor: String,
    /// "Open terminal here": `windowsTerminal`, `shell` or `none`.
    #[serde(default = "default_terminal")]
    pub terminal: String,
    /// The island at the top of the screen: `on`, or `off` when the tray icon and
    /// its flyout carry everything (the island then only opens for chat and drops).
    #[serde(default = "default_island")]
    pub island: String,
    /// Windows notifications: `needsYou` (questions, failures), `all` (also
    /// finishes) or `off`.
    #[serde(default = "default_notifications")]
    pub notifications: String,
    /// Who answers the chat: `anthropic` (Claude) or `llmServer` (your own
    /// OpenAI-compatible server — llama.cpp, LM Studio, Ollama…).
    #[serde(default = "default_chat_provider")]
    pub chat_provider: String,
    /// The model server's base URL, up to and including `/v1`.
    #[serde(default = "default_llm_server_url")]
    pub llm_server_url: String,
    /// The model to ask for; empty means the first one the server lists.
    #[serde(default)]
    pub llm_server_model: String,
}

fn default_model() -> String {
    crate::claude::DEFAULT_MODEL.to_string()
}

fn default_editor() -> String {
    "cursor".into()
}

fn default_terminal() -> String {
    "windowsTerminal".into()
}

fn default_island() -> String {
    "off".into()
}

fn default_notifications() -> String {
    "needsYou".into()
}

fn default_chat_provider() -> String {
    "anthropic".into()
}

/// Where llama-server listens when started with no options.
fn default_llm_server_url() -> String {
    "http://127.0.0.1:8080/v1".into()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sound_enabled: true,
            sound_volume: 0.12,
            auto_close_interval: 15.0,
            absence_interval: 180.0,
            active_integrations: vec![
                "integration_resend".into(),
                "integration_n8n".into(),
                "integration_vercel".into(),
                "integration_github".into(),
            ],
            screen: "primary".into(),
            autostart: false,
            hooks_installed: false,
            model: default_model(),
            editor: default_editor(),
            terminal: default_terminal(),
            island: default_island(),
            notifications: default_notifications(),
            chat_provider: default_chat_provider(),
            llm_server_url: default_llm_server_url(),
            llm_server_model: String::new(),
        }
    }
}

pub use crate::platform::{config_dir, local_dir};

pub fn hook_exe_path() -> PathBuf {
    local_dir().join("bin").join(crate::platform::HOOK_EXE)
}

fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn load() -> Settings {
    match std::fs::read(settings_path()) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

pub fn save(settings: &Settings) -> std::io::Result<()> {
    let dir = config_dir();
    crate::platform::ensure_private_dir(&dir)?;
    let json = serde_json::to_vec_pretty(settings)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(settings_path(), json)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The settings file of a real install made before `editor` and `terminal`
    /// existed. A field missing from it must never cost the user the rest:
    /// `load()` falls back to defaults for the *whole* file when parsing fails.
    const BEFORE_EDITOR_AND_TERMINAL: &str = r#"{
      "soundEnabled": false,
      "soundVolume": 0.12,
      "autoCloseInterval": 15.0,
      "absenceInterval": 180.0,
      "activeIntegrations": ["integration_resend", "integration_n8n", "integration_github"],
      "screen": "primary",
      "autostart": true,
      "hooksInstalled": true,
      "model": "claude-opus-5"
    }"#;

    #[test]
    fn a_settings_file_from_before_editor_and_terminal_loads_intact() {
        let s: Settings = serde_json::from_str(BEFORE_EDITOR_AND_TERMINAL)
            .expect("an older settings.json must still parse");
        // Everything that was there is still there…
        assert!(!s.sound_enabled);
        assert!(s.autostart && s.hooks_installed);
        assert_eq!(
            s.active_integrations,
            vec!["integration_resend", "integration_n8n", "integration_github"]
        );
        assert_eq!(s.model, "claude-opus-5");
        // …and the new settings take the defaults the owner asked for.
        assert_eq!(s.editor, "cursor");
        assert_eq!(s.terminal, "windowsTerminal");
        assert_eq!(s.island, "off", "the tray flyout replaces the island unless asked for");
        assert_eq!(s.notifications, "needsYou");
    }

    #[test]
    fn a_settings_file_with_editor_and_terminal_but_no_shell_settings_loads_intact() {
        let s: Settings = serde_json::from_str(
            r#"{"soundEnabled":true,"soundVolume":0.1,"autoCloseInterval":15,"absenceInterval":180,
                "activeIntegrations":[],"screen":"cursor","autostart":false,"hooksInstalled":true,
                "model":"claude-opus-5","editor":"vscode","terminal":"shell"}"#,
        )
        .expect("the previous release's settings.json must still parse");
        assert_eq!((s.editor.as_str(), s.terminal.as_str(), s.screen.as_str()), ("vscode", "shell", "cursor"));
        assert_eq!((s.island.as_str(), s.notifications.as_str()), ("off", "needsYou"));
        assert_eq!(s.chat_provider, "anthropic", "the chat keeps talking to Claude until told otherwise");
        assert_eq!(s.llm_server_url, "http://127.0.0.1:8080/v1", "llama-server's own default");
        assert_eq!(s.llm_server_model, "");
    }

    #[test]
    fn the_new_settings_survive_a_save_and_load() {
        let mut s = Settings::default();
        s.editor = "vscode".into();
        s.terminal = "none".into();
        s.island = "on".into();
        s.notifications = "all".into();
        s.chat_provider = "llmServer".into();
        s.llm_server_url = "http://192.168.1.20:8080/v1".into();
        s.llm_server_model = "qwen2.5".into();
        let back: Settings = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert_eq!((back.editor.as_str(), back.terminal.as_str()), ("vscode", "none"));
        assert_eq!((back.island.as_str(), back.notifications.as_str()), ("on", "all"));
        assert_eq!(back.chat_provider, "llmServer");
        assert_eq!((back.llm_server_url.as_str(), back.llm_server_model.as_str()), ("http://192.168.1.20:8080/v1", "qwen2.5"));
    }

    #[test]
    fn the_defaults_are_cursor_and_windows_terminal() {
        let s = Settings::default();
        assert_eq!((s.editor.as_str(), s.terminal.as_str()), ("cursor", "windowsTerminal"));
    }
}
