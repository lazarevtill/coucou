//! A short user-requested audio check. Only levels are emitted; no samples are
//! retained, written to disk, transcribed or sent to a provider.
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use super::{audio::Source, source::LiveSource};

static ACTIVE: AtomicBool = AtomicBool::new(false);
static CANCEL: AtomicBool = AtomicBool::new(false);
pub fn stop() { CANCEL.store(true, Ordering::Release); }

pub fn test(app: AppHandle, mic: String, speakers: String) -> Result<(), String> {
    if ACTIVE.swap(true, Ordering::AcqRel) { return Err("An audio test is already running.".into()); }
    struct Finished;
    impl Drop for Finished { fn drop(&mut self) { ACTIVE.store(false, Ordering::Release); } }
    let _finished = Finished;
    CANCEL.store(false, Ordering::Release);
    let origin = Instant::now();
    let mut sources = [Source::Mic((!mic.is_empty()).then_some(mic)), Source::System((!speakers.is_empty()).then_some(speakers))]
        .map(|source| LiveSource::start(source, origin, |_, _| Ok(())));
    let started = Instant::now();
    let mut refreshed = Instant::now();
    while started.elapsed() < Duration::from_secs(10) && !CANCEL.load(Ordering::Acquire) {
        if refreshed.elapsed() >= Duration::from_secs(1) {
            for source in &mut sources { source.refresh(); }
            refreshed = Instant::now();
        }
        let values: Vec<_> = sources.iter().map(LiveSource::status).collect();
        let _ = app.emit_to(crate::HUB_LABEL, "audio-test-levels", values);
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
