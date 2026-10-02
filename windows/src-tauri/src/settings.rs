// Preferences, stored as plain JSON in %APPDATA%\Coucou\settings.json.
// No secret ever lands here — API keys live in the Windows Credential Manager.

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
        }
    }
}

/// %APPDATA%\Coucou
pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Coucou")
}

/// %LOCALAPPDATA%\Coucou — where coucou-hook.exe and the log live.
pub fn local_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Coucou")
}

pub fn hook_exe_path() -> PathBuf {
    local_dir().join("bin").join("coucou-hook.exe")
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
    std::fs::create_dir_all(&dir)?;
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
    }

    #[test]
    fn the_new_settings_survive_a_save_and_load() {
        let mut s = Settings::default();
        s.editor = "vscode".into();
        s.terminal = "none".into();
        let back: Settings = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert_eq!((back.editor.as_str(), back.terminal.as_str()), ("vscode", "none"));
    }

    #[test]
    fn the_defaults_are_cursor_and_windows_terminal() {
        let s = Settings::default();
        assert_eq!((s.editor.as_str(), s.terminal.as_str()), ("cursor", "windowsTerminal"));
    }
}
