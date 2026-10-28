//! A short user-requested audio check. Only levels are emitted; no samples are
//! retained, written to disk, transcribed or sent to a provider.
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use crate::notes::track::SourceStatus;
use super::audio::{Capture, Source};

static ACTIVE: AtomicBool = AtomicBool::new(false);
static CANCEL: AtomicBool = AtomicBool::new(false);

pub fn stop() { CANCEL.store(true, Ordering::Release); }

pub fn test(app: AppHandle, mic: String, speakers: String) -> Result<(), String> {
    if ACTIVE.swap(true, Ordering::AcqRel) { return Err("An audio test is already running.".into()); }
    struct Finished;
    impl Drop for Finished { fn drop(&mut self) { ACTIVE.store(false, Ordering::Release); } }
    let _finished = Finished;
    CANCEL.store(false, Ordering::Release);
    let sources = [Source::Mic((!mic.is_empty()).then_some(mic)), Source::System((!speakers.is_empty()).then_some(speakers))];
    let mut captures = Vec::new();
    let mut meters = Vec::new();
    for source in sources {
        let status = SourceStatus { source: if matches!(source, Source::Mic(_)) {"microphone"} else {"playback"}.into(),
            automatic: matches!(source, Source::Mic(None) | Source::System(None)), ..Default::default() };
        let meter = Arc::new(Mutex::new((status, Instant::now())));
        let sink = meter.clone();
        let result = Capture::start(source, move |samples| {
            let mut m = sink.lock().unwrap();
            let (rms, peak) = crate::notes::track::levels(samples);
            m.0.level = rms;
            m.0.peak = peak;
            m.0.clipping = peak >= 0.995;
            m.0.received = true;
            m.1 = Instant::now();
        });
        match result {
            Ok(capture) => {
                let info = capture.info();
                let mut m = meter.lock().unwrap();
                m.0.connected = true;
                m.0.device = info.name.clone();
                m.0.sample_rate = info.sample_rate;
                m.0.channels = info.channels;
                captures.push(Some(capture));
            }
            Err(error) => { meter.lock().unwrap().0.error = Some(error); captures.push(None); },
        }
        meters.push(meter);
    }
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(10) && !CANCEL.load(Ordering::Acquire) {
        let values: Vec<_> = meters.iter().enumerate().map(|(index, meter)| {
            let m = meter.lock().unwrap();
            let mut status = m.0.clone();
            if m.1.elapsed() > Duration::from_millis(500) { status.level = 0.0; status.peak = 0.0; status.clipping = false; }
            if let Some(error) = captures[index].as_ref().and_then(Capture::error) { status.error = Some(error); status.connected = false; }
            status
        }).collect();
        let _ = app.emit_to(crate::HUB_LABEL, "audio-test-levels", values);
        std::thread::sleep(Duration::from_millis(100));
    }
    drop(captures);
    Ok(())
}
