// Preferences, stored as plain JSON in settings.json under platform::config_dir().
// No secret ever lands here — API keys live in the OS keychain (see secrets.rs).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSetting {
    pub enabled: bool,
    /// SHA-256 of the plugin.json the user approved. Built-ins have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub sound_enabled: bool,
    pub sound_volume: f64,
    pub auto_close_interval: f64,
    pub absence_interval: f64,
    /// What is switched on: built-in integrations (`integration_*`) and plugins
    /// from the plugins folder, which also carry the hash of the manifest the
    /// user approved.
    #[serde(default)]
    pub plugins: BTreeMap<String, PluginSetting>,
    /// `activeIntegrations`, from before `plugins`: read once to migrate, never written.
    #[serde(default, rename = "activeIntegrations", skip_serializing)]
    pub legacy_active: Option<Vec<String>>,
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
            plugins: ["integration_resend", "integration_n8n", "integration_vercel", "integration_github"]
                .into_iter()
                .map(|id| (id.to_string(), PluginSetting { enabled: true, approved_hash: None }))
                .collect(),
            legacy_active: None,
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

impl Settings {
    /// A built-in integration or a plugin the user switched on.
    pub fn is_enabled(&self, id: &str) -> bool {
        self.plugins.get(id).is_some_and(|p| p.enabled)
    }
}

fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

/// Set when the file on disk is still in the old shape, until it is rewritten.
static MIGRATED: AtomicBool = AtomicBool::new(false);

/// Reads a settings file, moving `activeIntegrations` into `plugins`. The flag
/// says the file on disk still has the old shape.
pub fn from_json(bytes: &[u8]) -> Result<(Settings, bool), String> {
    let mut s: Settings = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let legacy = s.legacy_active.take();
    let migrated = match legacy {
        Some(ids) if s.plugins.is_empty() => {
            s.plugins = ids.into_iter().map(|id| (id, PluginSetting { enabled: true, approved_hash: None })).collect();
            true
        }
        _ => false,
    };
    Ok((s, migrated))
}

pub fn load() -> Settings {
    match std::fs::read(settings_path()) {
        Ok(bytes) => match from_json(&bytes) {
            Ok((s, migrated)) => {
                MIGRATED.store(migrated, Ordering::Relaxed);
                s
            }
            Err(_) => Settings::default(),
        },
        Err(_) => Settings::default(),
    }
}

/// Writes the settings. The first write after a migration first keeps the old
/// file next to it, dated, byte for byte.
pub fn save_to(path: &Path, settings: &Settings, migrated: bool) -> std::io::Result<()> {
    if migrated && path.exists() {
        let t = crate::platform::local_time();
        let name = format!(
            "settings.json.{:04}{:02}{:02}-{:02}{:02}{:02}.bak",
            t.year, t.month, t.day, t.hour, t.minute, t.second
        );
        std::fs::copy(path, path.with_file_name(name))?;
    }
    let json = serde_json::to_vec_pretty(settings).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, json)
}

pub fn save(settings: &Settings) -> std::io::Result<()> {
    crate::platform::ensure_private_dir(&config_dir())?;
    let migrated = MIGRATED.load(Ordering::Relaxed);
    save_to(&settings_path(), settings, migrated)?;
    MIGRATED.store(false, Ordering::Relaxed);
    Ok(())
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
    fn the_real_old_file_migrates_to_exactly_the_same_switched_on_integrations() {
        let (s, migrated) = from_json(BEFORE_EDITOR_AND_TERMINAL.as_bytes()).unwrap();
        assert!(migrated);
        let on: Vec<&str> = s.plugins.iter().filter(|(_, p)| p.enabled).map(|(id, _)| id.as_str()).collect();
        assert_eq!(on, vec!["integration_github", "integration_n8n", "integration_resend"]);
        assert!(s.is_enabled("integration_n8n"));
        assert!(!s.is_enabled("integration_vercel"));
        // And the old field is never written back.
        let written = serde_json::to_value(&s).unwrap();
        assert!(written.get("activeIntegrations").is_none());
        assert_eq!(written["plugins"]["integration_resend"]["enabled"], true);
    }

    #[test]
    fn a_file_already_on_plugins_is_left_as_it_is() {
        let (s, migrated) = from_json(
            br#"{"soundEnabled":true,"soundVolume":0.1,"autoCloseInterval":15,"absenceInterval":180,"screen":"primary",
                "autostart":false,"hooksInstalled":false,"model":"m",
                "plugins":{"integration_vercel":{"enabled":true},"status-page":{"enabled":true,"approvedHash":"ab12"}}}"#,
        )
        .unwrap();
        assert!(!migrated);
        assert!(s.is_enabled("integration_vercel"));
        assert_eq!(s.plugins["status-page"].approved_hash.as_deref(), Some("ab12"));
    }

    #[test]
    fn a_new_install_starts_with_the_same_integrations_as_before() {
        let s = Settings::default();
        for id in ["integration_resend", "integration_n8n", "integration_vercel", "integration_github"] {
            assert!(s.is_enabled(id), "{id}");
        }
        assert!(!s.is_enabled("integration_stripe"));
    }

    #[test]
    fn the_first_save_after_a_migration_keeps_a_dated_backup_of_the_old_file() {
        let dir = std::env::temp_dir().join(format!("coucou-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, BEFORE_EDITOR_AND_TERMINAL).unwrap();

        let (s, migrated) = from_json(&std::fs::read(&path).unwrap()).unwrap();
        save_to(&path, &s, migrated).unwrap();
        let backups: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with("settings.json.") && n.ends_with(".bak"))
            .collect();
        assert_eq!(backups.len(), 1, "{backups:?}");
        assert_eq!(std::fs::read_to_string(dir.join(&backups[0])).unwrap(), BEFORE_EDITOR_AND_TERMINAL, "the backup is the old file, byte for byte");
        let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(saved.get("plugins").is_some() && saved.get("activeIntegrations").is_none());

        // Later saves take no further backup.
        save_to(&path, &s, false).unwrap();
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_settings_file_from_before_editor_and_terminal_loads_intact() {
        let (s, _) = from_json(BEFORE_EDITOR_AND_TERMINAL.as_bytes()).expect("an older settings.json must still parse");
        // Everything that was there is still there…
        assert!(!s.sound_enabled);
        assert!(s.autostart && s.hooks_installed);
        assert!(["integration_resend", "integration_n8n", "integration_github"].iter().all(|id| s.is_enabled(id)));
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
