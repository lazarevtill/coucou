// Running plugins: the built-in integrations and the plugins from the folder,
// behind one trait and one pause switch.
//
// Built-ins keep exactly the delays and intervals they always had (the macOS
// ones) and report through their own `integration` events. Plugins from the
// folder report a `plugin-update`, wait longer after each failure, and are
// started and stopped by `refresh` as they are approved, edited or removed.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use super::manifest::{Manifest, ShownItem, Source};
use super::registry::{self, Approval};
use super::{http, mcp};
use crate::island::WINDOW_LABEL;

pub type PollFuture<'a> = Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>>;

/// Anything Coucou polls on a timer.
pub trait Plugin: Send + Sync {
    fn id(&self) -> &str;
    fn first_delay(&self) -> Duration;
    fn every(&self) -> Duration;
    /// One poll; the plugin reports what it found itself.
    fn poll(&self, app: AppHandle) -> PollFuture<'_>;
}

fn paused() -> bool {
    crate::integrations::PAUSED.load(std::sync::atomic::Ordering::Relaxed)
}

fn enabled(app: &AppHandle, id: &str) -> bool {
    app.try_state::<crate::Shared>().map(|s| s.settings.lock().unwrap().is_enabled(id)).unwrap_or(false)
}

/// A built-in: its first poll after its delay, then on a fixed cadence. Ticks
/// while paused or switched off are skipped: no network at all.
pub fn run_builtin(app: AppHandle, plugin: Arc<dyn Plugin>) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(plugin.first_delay()).await;
        let mut ticker = tokio::time::interval(plugin.every());
        loop {
            ticker.tick().await;
            if paused() || !enabled(&app, plugin.id()) {
                continue;
            }
            let _ = plugin.poll(app.clone()).await;
        }
    });
}

/// After this many failures in a row a plugin from the folder waits an hour.
const MAX_BACKOFF: Duration = Duration::from_secs(3600);

/// The wait before a folder plugin's next poll: its interval, doubled for each
/// failure in a row, never more than an hour.
pub fn next_wait(every: Duration, failures: u32) -> Duration {
    let factor = 1u32.checked_shl(failures.min(16)).unwrap_or(u32::MAX);
    every.saturating_mul(factor).min(MAX_BACKOFF.max(every))
}

fn run_folder(app: AppHandle, plugin: Arc<FolderPlugin>) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(plugin.first_delay()).await;
        let mut failures = 0u32;
        loop {
            if paused() || !enabled(&app, plugin.id()) {
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
            failures = match plugin.poll(app.clone()).await {
                Ok(()) => 0,
                Err(_) => failures.saturating_add(1),
            };
            tokio::time::sleep(next_wait(plugin.every(), failures)).await;
        }
    })
}

/// What the island shows for a plugin from the folder.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginStatus {
    pub id: String,
    pub name: String,
    pub color: String,
    /// `ok`, `error`, `waiting`, or the approval state (`off`, `changed`, `invalid`).
    pub status: String,
    pub headline: Option<String>,
    pub items: Vec<ShownItem>,
    pub error: Option<String>,
    pub link: Option<String>,
}

fn emit(app: &AppHandle, status: PluginStatus) {
    let _ = app.emit_to(WINDOW_LABEL, "plugin-update", status);
}

pub struct FolderPlugin {
    pub m: Manifest,
}

impl FolderPlugin {
    fn status(&self, status: &str) -> PluginStatus {
        PluginStatus {
            id: self.m.id.clone(),
            name: self.m.name.clone(),
            color: self.m.color.clone(),
            status: status.into(),
            headline: None,
            items: Vec::new(),
            error: None,
            link: self.m.links.first().map(|l| l.url.clone()),
        }
    }
}

impl Plugin for FolderPlugin {
    fn id(&self) -> &str {
        &self.m.id
    }
    fn first_delay(&self) -> Duration {
        Duration::from_secs(3)
    }
    fn every(&self) -> Duration {
        Duration::from_secs(self.m.poll_secs)
    }
    fn poll(&self, app: AppHandle) -> PollFuture<'_> {
        Box::pin(async move {
            let id = self.m.id.clone();
            let secret = move |key: &str| crate::secrets::get(&crate::secrets::plugin_key(&id, key));
            let result = match &self.m.source {
                Source::Http { .. } => http::poll(&self.m, &secret).await,
                Source::Mcp(m) if m.poll.is_none() => {
                    // Nothing to call on a timer: this plugin only offers tools.
                    let mut s = self.status("ok");
                    s.headline = Some("Offers tools; nothing to show on a timer.".into());
                    emit(&app, s);
                    return Ok(());
                }
                Source::Mcp(_) => mcp::poll(&self.m, &secret).await,
            };
            match result {
                Ok(shown) => {
                    let mut s = self.status("ok");
                    s.headline = shown.headline;
                    s.items = shown.items;
                    emit(&app, s);
                    Ok(())
                }
                Err(err) => {
                    // The host and what went wrong, never the plugin's data.
                    crate::log::line(format!("plugin {} failed", self.m.id));
                    let mut s = self.status("error");
                    s.error = Some(err.clone());
                    emit(&app, s);
                    Err(err)
                }
            }
        })
    }
}

/// The folder plugins that are running, with the manifest hash they run with.
#[derive(Default)]
pub struct Supervisor(Mutex<HashMap<String, (String, tauri::async_runtime::JoinHandle<()>)>>);

/// Starts what is approved, stops what is not (any more), and tells the island
/// the state of every plugin in the folder.
pub fn refresh(app: &AppHandle) {
    let installed = registry::scan(&registry::dir());
    let settings = match app.try_state::<crate::Shared>() {
        Some(s) => s.settings.lock().unwrap().clone(),
        None => return,
    };
    let supervisor = app.state::<Supervisor>();
    let mut running = supervisor.0.lock().unwrap();
    let mut keep = HashSet::new();
    for p in &installed {
        match (registry::approval(p, settings.plugins.get(&p.id)), &p.manifest) {
            (Approval::On, Ok(m)) => {
                keep.insert(p.id.clone());
                let current = running.get(&p.id).is_some_and(|(hash, _)| *hash == p.hash);
                if !current {
                    if let Some((_, old)) = running.remove(&p.id) {
                        old.abort();
                    }
                    let plugin = Arc::new(FolderPlugin { m: m.clone() });
                    emit(app, plugin.status("waiting"));
                    running.insert(p.id.clone(), (p.hash.clone(), run_folder(app.clone(), plugin)));
                }
            }
            (state, manifest) => {
                let (name, color, link) = match manifest {
                    Ok(m) => (m.name.clone(), m.color.clone(), m.links.first().map(|l| l.url.clone())),
                    Err(_) => (p.id.clone(), "#8C8C8C".to_string(), None),
                };
                let (status, error) = match (state, manifest) {
                    (Approval::Invalid, Err(e)) => ("invalid", Some(e.clone())),
                    (Approval::Changed, _) => ("changed", Some("Changed since you approved it — review it in Settings → Plugins.".into())),
                    _ => ("off", None),
                };
                emit(app, PluginStatus { id: p.id.clone(), name, color, status: status.into(), headline: None, items: Vec::new(), error, link });
            }
        }
    }
    running.retain(|id, (_, handle)| {
        let k = keep.contains(id);
        if !k {
            handle.abort();
        }
        k
    });
}

/// Looks at the folder now and every minute: a manifest edited by hand stops its
/// plugin within a minute.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            refresh(&app);
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failing_plugin_waits_longer_each_time_but_never_more_than_an_hour() {
        let every = Duration::from_secs(60);
        assert_eq!(next_wait(every, 0), Duration::from_secs(60));
        assert_eq!(next_wait(every, 1), Duration::from_secs(120));
        assert_eq!(next_wait(every, 3), Duration::from_secs(480));
        assert_eq!(next_wait(every, 10), Duration::from_secs(3600));
        assert_eq!(next_wait(every, u32::MAX), Duration::from_secs(3600));
        // A plugin that polls daily is not polled more often after a failure.
        assert_eq!(next_wait(Duration::from_secs(86_400), 2), Duration::from_secs(86_400));
    }
}
