// The plugins folder: %APPDATA%\Coucou\plugins\<id>\plugin.json.
//
// Every folder is read on each scan. A plugin runs only while the user has
// approved exactly the manifest that is there now: the approval stores the
// SHA-256 of the file's bytes, so any edit switches the plugin off until it is
// reviewed again.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::manifest::{self, Manifest};
use super::sha256;
use crate::settings::PluginSetting;

/// A manifest larger than this is not one.
const MAX_MANIFEST: u64 = 64 * 1024;

#[derive(Debug, Clone)]
pub struct Installed {
    /// The folder's name, which the manifest's id must match.
    pub id: String,
    /// SHA-256 of plugin.json, as read.
    pub hash: String,
    pub manifest: Result<Manifest, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Approval {
    Off,
    On,
    /// Approved once, but the manifest is no longer the one approved.
    Changed,
    Invalid,
}

pub fn dir() -> PathBuf {
    crate::settings::config_dir().join("plugins")
}

fn read_one(dir: &Path, id: &str) -> (String, Result<Manifest, String>) {
    let path = dir.join("plugin.json");
    let len = match std::fs::metadata(&path) {
        Ok(m) => m.len(),
        Err(_) => return (String::new(), Err("There is no plugin.json in this folder.".into())),
    };
    if len > MAX_MANIFEST {
        return (String::new(), Err("plugin.json is too large to be a manifest.".into()));
    }
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => return (String::new(), Err(format!("plugin.json could not be read: {e}"))),
    };
    let hash = sha256::hex(&bytes);
    if !manifest::valid_id(id) {
        return (hash, Err("The folder's name must be the plugin's id: lowercase letters, digits and hyphens.".into()));
    }
    let parsed = manifest::parse(&bytes).and_then(|m| {
        if m.id == id {
            Ok(m)
        } else {
            Err(format!("The manifest's id is \"{}\" but its folder is \"{id}\".", m.id))
        }
    });
    (hash, parsed)
}

/// Every folder in the plugins folder, sorted by name.
pub fn scan(dir: &Path) -> Vec<Installed> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut found: Vec<Installed> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| {
            let id = e.file_name().to_string_lossy().to_string();
            let (hash, manifest) = read_one(&e.path(), &id);
            Installed { id, hash, manifest }
        })
        .collect();
    found.sort_by(|a, b| a.id.cmp(&b.id));
    found
}

pub fn approval(installed: &Installed, setting: Option<&PluginSetting>) -> Approval {
    if installed.manifest.is_err() {
        return Approval::Invalid;
    }
    match setting {
        Some(s) if s.enabled && s.approved_hash.as_deref() == Some(installed.hash.as_str()) => Approval::On,
        Some(s) if s.enabled => Approval::Changed,
        _ => Approval::Off,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"{"schema":1,"id":"status-page","name":"Status","version":"1","kind":"http",
        "allowedHosts":["status.example.com"],"http":{"url":"https://status.example.com/api"}}"#;

    fn folder(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("coucou-plugins-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn put(root: &Path, id: &str, body: &[u8]) {
        std::fs::create_dir_all(root.join(id)).unwrap();
        std::fs::write(root.join(id).join("plugin.json"), body).unwrap();
    }

    #[test]
    fn every_folder_is_listed_with_its_manifest_or_why_it_is_not_usable() {
        let root = folder("scan");
        put(&root, "status-page", GOOD.as_bytes());
        put(&root, "broken", b"{ not json");
        put(&root, "other-name", GOOD.as_bytes()); // manifest says status-page
        std::fs::create_dir_all(root.join("empty")).unwrap();
        put(&root, "Bad_Folder", GOOD.as_bytes());
        put(&root, "huge", &vec![b' '; 70 * 1024]);
        std::fs::write(root.join("stray-file.json"), GOOD).unwrap();

        let found = scan(&root);
        let ids: Vec<&str> = found.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["Bad_Folder", "broken", "empty", "huge", "other-name", "status-page"], "folders only, sorted");
        let get = |id: &str| found.iter().find(|p| p.id == id).unwrap();
        assert!(get("status-page").manifest.is_ok());
        assert_eq!(get("status-page").hash, sha256::hex(GOOD.as_bytes()), "the hash is of the file's bytes");
        assert!(get("broken").manifest.is_err());
        assert!(get("other-name").manifest.as_ref().unwrap_err().contains("status-page"), "id must match the folder");
        assert!(get("empty").manifest.as_ref().unwrap_err().contains("plugin.json"));
        assert!(get("Bad_Folder").manifest.is_err());
        assert!(get("huge").manifest.as_ref().unwrap_err().contains("large"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_plugin_runs_only_while_the_approved_manifest_is_the_one_there() {
        let root = folder("approval");
        put(&root, "status-page", GOOD.as_bytes());
        let p = scan(&root).remove(0);
        let on = PluginSetting { enabled: true, approved_hash: Some(p.hash.clone()) };
        assert_eq!(approval(&p, None), Approval::Off, "never approved");
        assert_eq!(approval(&p, Some(&on)), Approval::On);
        assert_eq!(approval(&p, Some(&PluginSetting { enabled: false, approved_hash: Some(p.hash.clone()) })), Approval::Off);
        assert_eq!(approval(&p, Some(&PluginSetting { enabled: true, approved_hash: None })), Approval::Changed, "on without a hash is not approved");

        // Somebody edits the manifest.
        put(&root, "status-page", GOOD.replace("\"Status\"", "\"Status!\"").as_bytes());
        let edited = scan(&root).remove(0);
        assert_eq!(approval(&edited, Some(&on)), Approval::Changed);

        put(&root, "status-page", b"{}");
        let broken = scan(&root).remove(0);
        assert_eq!(approval(&broken, Some(&on)), Approval::Invalid);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_folder_is_no_plugins() {
        assert!(scan(Path::new(r"Z:\no\such\plugins")).is_empty());
    }
}
