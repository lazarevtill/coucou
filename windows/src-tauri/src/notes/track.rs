//! One recording source. The audio callback only queues samples; disk I/O
//! happens on a worker. Reopening an endpoint keeps the original call clock.

use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use serde::Serialize;
use crate::speech::audio::{Capture, Source};
use super::Channel;

#[derive(Clone, Default, Serialize)]
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
}

pub struct Track {
    source: Source,
    origin: Instant,
    capture: Option<Capture>,
    device_id: Option<String>,
    pub(super) channel: Arc<Mutex<Channel>>,
    tx: Option<mpsc::SyncSender<(Duration, Vec<f32>)>>,
    writer: Option<std::thread::JoinHandle<()>>,
    status: Arc<Mutex<SourceStatus>>,
    last_audio: Arc<Mutex<Option<Instant>>>,
    last_attempt: Instant,
}

impl Track {
    pub fn start(source: Source, origin: Instant, path: &Path) -> Result<Self, String> {
        let channel = Arc::new(Mutex::new(Channel::new(path).map_err(|e| e.to_string())?));
        let (tx, rx) = mpsc::sync_channel::<(Duration, Vec<f32>)>(256);
        let writing = channel.clone();
        let writer = std::thread::Builder::new().name("coucou-call-writer".into()).spawn(move || {
            while let Ok((at, samples)) = rx.recv() { writing.lock().unwrap().push_at(&samples, at); }
            writing.lock().unwrap().close();
        }).map_err(|e| e.to_string())?;
        let (kind, manual) = match &source { Source::Mic(value) => ("microphone", value), Source::System(value) => ("playback", value) };
        let status = SourceStatus { source: kind.into(), device: manual.clone().unwrap_or_default(), automatic: manual.is_none(), ..Default::default() };
        let mut track = Self { source, origin, capture: None, device_id: None, channel, tx: Some(tx), writer: Some(writer),
            status: Arc::new(Mutex::new(status)), last_audio: Arc::new(Mutex::new(None)), last_attempt: Instant::now() };
        track.open();
        Ok(track)
    }

    fn open(&mut self) {
        self.last_attempt = Instant::now();
        self.device_id = crate::speech::audio::selected_device_id(&self.source);
        let tx = self.tx.as_ref().unwrap().clone();
        let status = self.status.clone();
        let last_audio = self.last_audio.clone();
        match Capture::start_timed(self.source.clone(), self.origin, move |samples, at| {
            let (level, peak) = levels(samples);
            {
                let mut s = status.lock().unwrap();
                s.level = level;
                s.peak = peak;
                s.clipping = peak >= 0.995;
                s.received = true;
            }
            *last_audio.lock().unwrap() = Some(Instant::now());
            if tx.try_send((at, samples.to_vec())).is_err() {
                record_error(&mut status.lock().unwrap(), "Audio data was lost because recording could not keep up.".into());
            }
        }) {
            Ok(capture) => {
                let info = capture.info();
                let mut s = self.status.lock().unwrap();
                s.connected = true;
                s.device = info.name.clone();
                s.sample_rate = info.sample_rate;
                s.channels = info.channels;
                s.error = None;
                self.capture = Some(capture);
            }
            Err(err) => {
                let mut s = self.status.lock().unwrap();
                s.connected = false;
                record_error(&mut s, err);
            }
        }
    }

    /// Follow automatic defaults, retry missing manual devices, and recover
    /// Bluetooth profile changes without replacing a manual choice.
    pub fn refresh(&mut self) {
        let current = crate::speech::audio::selected_device_id(&self.source);
        let failed = self.capture.as_ref().and_then(Capture::error);
        if let Some(error) = failed.as_ref() {
            let mut status = self.status.lock().unwrap();
            status.connected = false;
            record_error(&mut status, error.clone());
        }
        let retry = self.capture.is_none() || failed.is_some() || current != self.device_id;
        if retry && self.last_attempt.elapsed() >= Duration::from_secs(2) {
            if let Some(mut capture) = self.capture.take() { capture.stop(); }
            self.open();
        }
    }

    pub fn set_source(&mut self, source: Source) {
        if self.source == source { return; }
        self.source = source;
        {
            let mut s = self.status.lock().unwrap();
            s.automatic = matches!(self.source, Source::Mic(None) | Source::System(None));
            s.connected = false;
            s.received = false;
            s.level = 0.0;
            s.peak = 0.0;
            s.sample_rate = 0;
            s.channels = 0;
            s.device = match &self.source { Source::Mic(v) | Source::System(v) => v.clone().unwrap_or_default() };
        }
        self.last_attempt = Instant::now().checked_sub(Duration::from_secs(3)).unwrap_or_else(Instant::now);
        if let Some(mut capture) = self.capture.take() { capture.stop(); }
        self.open();
    }

    pub fn status(&self) -> SourceStatus {
        let mut s = self.status.lock().unwrap().clone();
        if self.last_audio.lock().unwrap().is_none_or(|at| at.elapsed() > Duration::from_millis(500)) {
            s.level = 0.0;
            s.peak = 0.0;
            s.clipping = false;
        }
        if let Some(error) = &self.channel.lock().unwrap().error { record_error(&mut s, error.clone()); }
        s
    }

    pub fn stop(&mut self) {
        if let Some(mut capture) = self.capture.take() { capture.stop(); }
        self.tx.take();
        if let Some(writer) = self.writer.take() { let _ = writer.join(); }
    }
}

impl Drop for Track { fn drop(&mut self) { self.stop(); } }

fn record_error(status: &mut SourceStatus, error: String) {
    // Bounded, persistent evidence of lost audio, even if the device recovers.
    if !status.warnings.contains(&error) && status.warnings.len() < 16 {
        status.warnings.push(error.clone());
    }
    status.error = Some(error);
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
    fn meters_distinguish_silence_speech_and_clipping() {
        assert_eq!(levels(&[]), (0.0, 0.0));
        assert_eq!(levels(&[0.0; 16]), (0.0, 0.0));
        assert_eq!(levels(&[-0.5, 0.5]), (0.5, 0.5));
        assert_eq!(levels(&[-1.2, 1.2]), (1.0, 1.0));
    }

    #[test]
    fn recovery_does_not_erase_recording_warnings() {
        let mut status = SourceStatus::default();
        record_error(&mut status, "Headset disconnected".into());
        record_error(&mut status, "Headset disconnected".into());
        status.connected = true;
        status.error = None;
        assert_eq!(status.warnings, ["Headset disconnected"]);
        for i in 0..40 { record_error(&mut status, format!("Error {i}")); }
        assert_eq!(status.warnings.len(), 16);
    }
}
