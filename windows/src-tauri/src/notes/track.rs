//! One recording source. Device recovery and meters are shared with the
//! Settings test; the audio callback only queues samples for the disk worker.

use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use crate::speech::{audio::Source, source::{LiveSource, SourceStatus}};
use super::Channel;

pub struct Track {
    live: Option<LiveSource>,
    pub(super) channel: Arc<Mutex<Channel>>,
    writer: Option<std::thread::JoinHandle<()>>,
    stopped: SourceStatus,
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
        let live = LiveSource::start(source, origin, move |samples, at| {
            tx.try_send((at, samples.to_vec())).map_err(|_| "Audio data was lost because recording could not keep up.".into())
        });
        Ok(Self { live: Some(live), channel, writer: Some(writer), stopped: SourceStatus::default() })
    }

    pub fn refresh(&mut self) { if let Some(live) = self.live.as_mut() { live.refresh(); } }
    pub fn set_source(&mut self, source: Source) { if let Some(live) = self.live.as_mut() { live.set_source(source); } }

    pub fn status(&self) -> SourceStatus {
        let mut status = self.live.as_ref().map_or_else(|| self.stopped.clone(), LiveSource::status);
        if let Some(error) = &self.channel.lock().unwrap().error { status.fail(error.clone()); }
        status
    }

    pub fn stop(&mut self) {
        if let Some(mut live) = self.live.take() {
            live.stop();
            self.stopped = live.status();
        }
        // Dropping LiveSource also drops its callback's queue sender.
        if let Some(writer) = self.writer.take() { let _ = writer.join(); }
    }
}

impl Drop for Track { fn drop(&mut self) { self.stop(); } }
