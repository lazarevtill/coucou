// How the launcher sees this Windows machine: PATH, PATHEXT and the standard
// install folders. The same walk the old `find_on_path` did, but returning every
// hit so the caller can skip the wrong ones (Cursor's `code` shim).

use std::path::{Path, PathBuf};

use crate::launch::Env;

pub struct RealEnv;

impl Env for RealEnv {
    fn find_all(&self, stem: &str) -> Vec<PathBuf> {
        let exts = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
        let Some(dirs) = std::env::var_os("PATH") else { return Vec::new() };
        let mut hits = Vec::new();
        for dir in std::env::split_paths(&dirs) {
            for ext in exts.split(';').filter(|e| !e.is_empty()) {
                let candidate = dir.join(format!("{stem}{}", ext.to_lowercase()));
                // `is_file` also sees through App Execution Aliases such as wt.exe.
                if candidate.is_file() && !hits.contains(&candidate) {
                    hits.push(candidate);
                }
            }
        }
        hits
    }

    fn file_exists(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn dir(&self, name: &str) -> Option<PathBuf> {
        std::env::var_os(name).map(PathBuf::from)
    }
}
