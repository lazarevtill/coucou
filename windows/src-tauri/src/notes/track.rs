//! One recording source. Device recovery and meters are shared with the
//! Settings test; the audio callback only queues samples for the disk worker.

use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use crate::speech::{audio::Source, source::{LiveSource, SourceStatus}};
use super::Channel;

/// Samples for the disk worker; `None` tells it to finish.
type Queue = mpsc::SyncSender<Option<(Duration, Vec<f32>)>>;

pub struct Track {
    live: Option<LiveSource>,
    pub(super) channel: Arc<Mutex<Channel>>,
    queue: Queue,
    writer: Option<std::thread::JoinHandle<()>>,
    stopped: SourceStatus,
}

/// The worker ends on `None`, not when every sender is gone: a capture thread
/// left blocked in a driver after a timed-out reopen still holds one.
fn spawn_writer(channel: Arc<Mutex<Channel>>) -> Result<(Queue, std::thread::JoinHandle<()>), String> {
    let (tx, rx) = mpsc::sync_channel::<Option<(Duration, Vec<f32>)>>(256);
    let writer = std::thread::Builder::new().name("coucou-call-writer".into()).spawn(move || {
        while let Ok(Some((at, samples))) = rx.recv() { channel.lock().unwrap().push_at(&samples, at); }
        channel.lock().unwrap().close();
    }).map_err(|e| e.to_string())?;
    Ok((tx, writer))
}

impl Track {
    pub fn start(source: Source, origin: Instant, path: &Path) -> Result<Self, String> {
        let channel = Arc::new(Mutex::new(Channel::new(path).map_err(|e| e.to_string())?));
        let (queue, writer) = spawn_writer(channel.clone())?;
        let tx = queue.clone();
        let live = LiveSource::start(source, origin, move |samples, at| {
            tx.try_send(Some((at, samples.to_vec()))).map_err(|_| "Audio data was lost because recording could not keep up.".into())
        });
        Ok(Self { live: Some(live), channel, queue, writer: Some(writer), stopped: SourceStatus::default() })
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
        if let Some(writer) = self.writer.take() {
            // Queued after everything the stopped capture delivered; blocks only while the queue drains.
            let _ = self.queue.send(None);
            let _ = writer.join();
        }
    }
}

impl Drop for Track { fn drop(&mut self) { self.stop(); } }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_returns_while_an_orphaned_capture_thread_still_holds_a_sender() {
        let path = std::env::temp_dir().join(format!("coucou-track-{}.wav", crate::flow::text::new_id()));
        let channel = Arc::new(Mutex::new(Channel::new(&path).unwrap()));
        let (queue, writer) = spawn_writer(channel.clone()).unwrap();
        // What a capture thread stuck in a driver call keeps after its reopen timed out.
        let orphan = queue.clone();
        orphan.send(Some((Duration::ZERO, vec![0.5; 160]))).unwrap();
        let mut track = Track { live: None, channel: channel.clone(), queue, writer: Some(writer), stopped: SourceStatus::default() };
        let (done, stopped) = mpsc::channel();
        std::thread::spawn(move || { track.stop(); let _ = done.send(()); });
        assert!(stopped.recv_timeout(Duration::from_secs(5)).is_ok(), "Track::stop waited for the orphaned sender");
        assert!(channel.lock().unwrap().writer.is_none(), "the recording was closed");
        assert_eq!(crate::speech::audio::read_wav(&path).unwrap().len(), 160);
        drop(orphan);
        std::fs::remove_file(path).unwrap();
    }
}
