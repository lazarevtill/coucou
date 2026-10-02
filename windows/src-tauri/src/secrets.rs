// API keys live in the Windows Credential Manager or, on Linux, the Secret
// Service (GNOME Keyring, KWallet) — never on disk and never in the front end — the island can only ask whether a key is present.

use keyring::Entry;

const SERVICE: &str = "fr.louisraille.coucou";

/// Every key Coucou may store. Anything outside this list is refused.
pub const KNOWN_KEYS: &[&str] = &[
    "anthropic-api-key",
    // Optional: llama-server --api-key, or a hosted OpenAI-compatible server.
    "llm-server-api-key",
    "n8n-url",
    "n8n-api-key",
    "vercel-token",
    "github-token",
    "stripe-api-key",
    "resend-api-key",
    "notion-api-key",
    "calcom-api-key",
];

/// A key Coucou may store: one of its own, or a plugin's.
pub fn allowed(key: &str) -> bool {
    if KNOWN_KEYS.contains(&key) {
        return true;
    }
    let Some(rest) = key.strip_prefix("plugin:") else { return false };
    let mut parts = rest.split(':');
    let (Some(id), Some(name), None) = (parts.next(), parts.next(), parts.next()) else { return false };
    let key_ok = !name.is_empty() && name.len() <= 32 && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    crate::plugins::manifest::valid_id(id) && key_ok
}

/// Where a plugin's secret lives: `plugin:<id>:<key>`.
pub fn plugin_key(id: &str, key: &str) -> String {
    format!("plugin:{id}:{key}")
}

fn entry(key: &str) -> Option<Entry> {
    if !allowed(key) {
        return None;
    }
    Entry::new(SERVICE, key).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coucous_own_keys_and_well_formed_plugin_keys_are_allowed() {
        assert!(allowed("anthropic-api-key"));
        assert!(allowed("llm-server-api-key"));
        assert!(allowed(&plugin_key("status-page", "token")));
        assert!(allowed("plugin:tickets:api_key"));
    }

    #[test]
    fn a_plugin_cannot_reach_outside_its_own_namespace() {
        for key in [
            "plugin:x:anthropic-api-key", // a hyphen is not allowed in a plugin's key
            "plugin:a:b:c",
            "plugin::token",
            "plugin:status-page:",
            "plugin:Status:token",
            "plugin:integration_stripe:token",
            "something-else",
            "",
        ] {
            assert!(!allowed(key), "{key:?}");
        }
    }
}

pub fn get(key: &str) -> Option<String> {
    entry(key)?.get_password().ok().filter(|v| !v.is_empty())
}

pub fn set(key: &str, value: &str) -> Result<(), String> {
    let entry = entry(key).ok_or_else(|| format!("unknown key {key}"))?;
    if value.is_empty() {
        let _ = entry.delete_credential();
        return Ok(());
    }
    entry.set_password(value).map_err(|e| e.to_string())
}

pub fn clear(key: &str) -> Result<(), String> {
    let entry = entry(key).ok_or_else(|| format!("unknown key {key}"))?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

pub fn present(key: &str) -> bool {
    get(key).is_some()
}
