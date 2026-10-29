//! One independently selected audio source, shared by call recording and the
//! no-file Settings test. Manual choices never fall back to another endpoint.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use super::audio::{self, Capture, Source};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatus {
    pub source: String,
    pub device: String,
    pub automatic: bool,
    pub connected: bool,
    pub level: f32,
    pub peak: f32,
    pub clipping: bool,
    pub received: bool,
    pub error: Option<String>,
    pub warnings: Vec<String>,
    pub sample_rate: u32,
    pub channels: u16,
    pub application: Option<String>,
    /// Successful endpoints used by this source, retained across recovery.
    #[serde(default)]
    pub devices: Vec<String>,
    /// Peak across the capture, including packets between UI polls.
    #[serde(default)]
    pub peak_seen: f32,
}

impl SourceStatus {
    pub fn warn(&mut self, warning: String) {
        if !self.warnings.contains(&warning) && self.warnings.len() < 16 { self.warnings.push(warning); }
    }

    pub fn fail(&mut self, error: String) {
        self.warn(error.clone());
        self.error = Some(error);
    }
}

type Sink = Box<dyn FnMut(&[f32], Duration) -> Result<(), String> + Send>;

pub struct LiveSource {
    source: Source,
    origin: Instant,
    capture: Option<Capture>,
    device_id: Option<String>,
    meter: Arc<Mutex<(SourceStatus, Option<Instant>)>>,
    sink: Arc<Mutex<Sink>>,
    last_attempt: Instant,
}

impl LiveSource {
    pub fn start(source: Source, origin: Instant, sink: impl FnMut(&[f32], Duration) -> Result<(), String> + Send + 'static) -> Self {
        let status = SourceStatus {
            source: if matches!(source, Source::Mic(_)) { "microphone" } else { "playback" }.into(),
            automatic: matches!(source, Source::Mic(None) | Source::System(None)),
            ..Default::default()
        };
        let mut live = Self { source, origin, capture: None, device_id: None,
            meter: Arc::new(Mutex::new((status, None))), sink: Arc::new(Mutex::new(Box::new(sink))), last_attempt: Instant::now() };
        live.open();
        live
    }

    fn open(&mut self) {
        self.last_attempt = Instant::now();
        let (resolved, application, warning) = super::routing::resolve(&self.source);
        {
            let mut m = self.meter.lock().unwrap();
            m.0.application = application;
            if let Some(warning) = warning { m.0.warn(warning); }
        }
        let meter = self.meter.clone();
        let sink = self.sink.clone();
        match Capture::start_timed(resolved, self.origin, move |samples, at| {
            let result = sink.lock().unwrap()(samples, at);
            let mut m = meter.lock().unwrap();
            let (rms, peak) = levels(samples);
            m.0.level = rms;
            m.0.peak = peak;
            m.0.peak_seen = m.0.peak_seen.max(peak);
            m.0.clipping = peak >= 0.995;
            m.0.received = true;
            if let Err(error) = result { m.0.fail(error); }
            m.1 = Some(Instant::now());
        }) {
            Ok(capture) => {
                let info = capture.info();
                self.device_id = Some(info.id.clone());
                let mut m = self.meter.lock().unwrap();
                m.0.connected = true;
                m.0.device = info.name.clone();
                if !m.0.devices.contains(&info.name) { m.0.devices.push(info.name.clone()); }
                m.0.sample_rate = info.sample_rate;
                m.0.channels = info.channels;
                m.0.error = None;
                self.capture = Some(capture);
            }
            Err(error) => {
                self.device_id = None;
                let mut m = self.meter.lock().unwrap();
                m.0.connected = false;
                m.0.fail(error);
            }
        }
    }

    /// Each source recovers independently. The same endpoint may reappear after
    /// Bluetooth multipoint, a USB replug, sleep, or a headset profile change.
    pub fn refresh(&mut self) {
        self.collect_health();
        let (resolved, application, warning) = super::routing::resolve(&self.source);
        let current = audio::selected_device_id(&resolved);
        {
            let mut m = self.meter.lock().unwrap();
            m.0.application = application;
            if let Some(warning) = warning { m.0.warn(warning); }
        }
        let failed = self.capture.as_ref().and_then(Capture::error).is_some();
        let changed = current != self.device_id;
        if self.capture.is_some() && changed {
            let mut m = self.meter.lock().unwrap();
            m.0.connected = false;
            m.0.fail(if current.is_none() { crate::i18n::t("Selected device (disconnected)") } else { crate::i18n::t("Audio device changed. Check the selected source.") });
        }
        if recovery_needed(self.capture.is_some(), failed, self.device_id.as_deref(), current.as_deref())
            && self.last_attempt.elapsed() >= Duration::from_secs(2) {
            self.stop();
            self.open();
        }
    }

    pub fn set_source(&mut self, source: Source) {
        if self.source == source { return; }
        self.stop();
        self.source = source;
        {
            let mut m = self.meter.lock().unwrap();
            let warnings = std::mem::take(&mut m.0.warnings);
            let devices = std::mem::take(&mut m.0.devices);
            let peak_seen = m.0.peak_seen;
            let received = m.0.received;
            let kind = m.0.source.clone();
            m.0 = SourceStatus { source: kind, automatic: matches!(self.source, Source::Mic(None) | Source::System(None)), warnings, devices, peak_seen, received, ..Default::default() };
            m.1 = None;
        }
        self.open();
    }

    fn collect_health(&self) {
        if let Some(capture) = &self.capture {
            let mut m = self.meter.lock().unwrap();
            for warning in capture.warnings() { m.0.warn(warning); }
            if let Some(error) = capture.error() { m.0.connected = false; m.0.fail(error); }
        }
    }

    pub fn status(&self) -> SourceStatus {
        self.collect_health();
        let m = self.meter.lock().unwrap();
        let mut status = m.0.clone();
        if !status.connected || m.1.is_none_or(|at| at.elapsed() > Duration::from_millis(500)) {
            status.level = 0.0;
            status.peak = 0.0;
            status.clipping = false;
        }
        status
    }

    pub fn stop(&mut self) {
        if let Some(capture) = self.capture.as_mut() { capture.stop(); }
        self.collect_health();
        self.capture.take();
    }
}

impl Drop for LiveSource { fn drop(&mut self) { self.stop(); } }

fn recovery_needed(open: bool, failed: bool, opened_id: Option<&str>, selected_id: Option<&str>) -> bool {
    !open || failed || opened_id != selected_id
}

pub(crate) fn levels(samples: &[f32]) -> (f32, f32) {
    if samples.is_empty() { return (0.0, 0.0); }
    let peak = samples.iter().fold(0.0_f32, |max, sample| max.max(sample.abs()));
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    (rms.clamp(0.0, 1.0), peak.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_recovery_covers_defaults_fixed_devices_profile_changes_and_replug() {
        for (open, failed, from, to, expected) in [
            (true, false, Some("laptop mic"), Some("laptop mic"), false),
            (true, false, Some("buds"), None, true),
            (false, false, None, Some("buds"), true),
            (true, false, Some("buds"), Some("speakers"), true),
            (true, true, Some("buds"), Some("buds"), true),
            (true, false, Some("usb"), Some("wired"), true),
            (false, false, None, None, true),
        ] { assert_eq!(recovery_needed(open, failed, from, to), expected, "{from:?} → {to:?}"); }
    }

    #[test]
    fn meters_distinguish_silence_speech_and_clipping() {
        assert_eq!(levels(&[]), (0.0, 0.0));
        assert_eq!(levels(&[0.0; 16]), (0.0, 0.0));
        assert_eq!(levels(&[-0.5, 0.5]), (0.5, 0.5));
        assert_eq!(levels(&[-1.2, 1.2]), (1.0, 1.0));
    }

    #[test]
    fn recovery_does_not_erase_recording_warnings() {
        let mut status = SourceStatus::default();
        status.fail("Headset disconnected".into());
        status.fail("Headset disconnected".into());
        status.connected = true;
        status.error = None;
        assert_eq!(status.warnings, ["Headset disconnected"]);
        for i in 0..40 { status.fail(format!("Error {i}")); }
        assert_eq!(status.warnings.len(), 16);
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "Live laptop mic and headset disconnect/reconnect check; needs explicit hardware interaction"]
    fn live_headset_recovery_keeps_laptop_microphone() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let mic_id = std::env::var("COUCOU_TEST_MIC_ID").expect("select the laptop microphone ID");
        let output_id = std::env::var("COUCOU_TEST_OUTPUT_ID").expect("select the headset output ID");
        let samples = Arc::new(AtomicUsize::new(0));
        let seen = samples.clone();
        let origin = Instant::now();
        let mut mic = LiveSource::start(Source::Mic(Some(mic_id)), origin, move |audio, _| {
            seen.fetch_add(audio.len(), Ordering::Relaxed); Ok(())
        });
        let mut output = LiveSource::start(Source::System(Some(output_id)), origin, |_, _| Ok(()));
        println!("microphone: {}", serde_json::to_string(&mic.status()).unwrap());
        println!("playback: {}", serde_json::to_string(&output.status()).unwrap());
        assert!(mic.status().connected && output.status().connected, "both devices must start connected");
        println!("READY: laptop microphone active; disconnect and reconnect the selected headset within 60 seconds");
        let mut lost = false;
        let mut recovered = false;
        let mut previous = String::new();
        while origin.elapsed() < Duration::from_secs(60) {
            mic.refresh();
            output.refresh();
            assert!(mic.status().connected, "the independent laptop microphone stopped");
            let state = output.status();
            let snapshot = serde_json::to_string(&state).unwrap();
            if snapshot != previous { println!("{:.1}s {snapshot}", origin.elapsed().as_secs_f32()); previous = snapshot; }
            lost |= !state.connected;
            if lost && state.connected { recovered = true; break; }
            std::thread::sleep(Duration::from_millis(500));
        }
        mic.stop(); output.stop();
        assert!(lost, "no physical headset disconnect was observed");
        assert!(recovered, "headset did not reconnect before the deadline");
        assert!(!output.status().warnings.is_empty(), "lost audio must stay visible after recovery");
        assert!(samples.load(Ordering::Relaxed) > super::super::SAMPLE_RATE as usize * 3, "laptop mic did not deliver sustained samples");
        println!("PASS: laptop mic continued; the fixed headset recovered; warnings retained; no recording saved");
    }
}
