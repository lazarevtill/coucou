//! User-started chat dictation. Audio stays in bounded memory and is never sent.
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State, WebviewWindow};
use crate::speech::{self, audio::{Capture, Source}};

const MAX_SECONDS: usize = 120;

#[derive(Default)]
pub struct ChatInput {
    active: Mutex<Option<Arc<AtomicU8>>>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl ChatInput {
    pub fn cancel(&self) {
        if let Some(mode) = self.active.lock().unwrap().as_ref() { mode.store(2, Ordering::Release); }
    }
    pub fn shutdown(&self) {
        self.cancel();
        if let Some(worker) = self.worker.lock().unwrap().take() { let _ = worker.join(); }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Event {
    phase: &'static str,
    level: f32,
    device: String,
    text: Option<String>,
    error: Option<String>,
}

fn emit(app: &AppHandle, phase: &'static str, level: f32, device: &str, text: Option<String>, error: Option<String>) {
    let _ = app.emit_to(crate::island::WINDOW_LABEL, "chat-dictation", Event { phase, level, device: device.into(), text, error });
}

fn from_chat(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == crate::island::WINDOW_LABEL { Ok(()) } else { Err("Open chat to use its microphone.".into()) }
}

fn append(samples: &mut Vec<f32>, incoming: &[f32]) {
    let remaining = (speech::SAMPLE_RATE as usize * MAX_SECONDS).saturating_sub(samples.len());
    samples.extend_from_slice(&incoming[..remaining.min(incoming.len())]);
}

#[tauri::command]
pub async fn chat_dictation_start(app: AppHandle, window: WebviewWindow, input: State<'_, Arc<ChatInput>>) -> Result<(), String> {
    from_chat(&window)?;
    if !speech::is_ready() { return Err(crate::i18n::t("Download the voice model first, in Coucou → Settings → Recognition.")); }
    if crate::voice::is_listening() { return Err("Pause the voice assistant before using chat dictation.".into()); }
    let mode = Arc::new(AtomicU8::new(0));
    {
        let mut active = input.active.lock().unwrap();
        if active.is_some() { return Err("Finish or cancel the current dictation first.".into()); }
        *active = Some(mode.clone());
    }
    let input = input.inner().clone();
    let owner = input.clone();
    let mic = crate::flow::get(&app).map(|f| f.store.lock().unwrap().prefs.mic.clone()).unwrap_or_default();
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let stopping = mode.clone();
    let worker_app = app.clone();
    let worker = std::thread::Builder::new().name("coucou-chat-mic".into()).spawn(move || {
        let app = worker_app;
        let samples = Arc::new(Mutex::new(Vec::new()));
        let writing = samples.clone();
        let level = Arc::new(Mutex::new(0.0));
        let meter = level.clone();
        let capture = Capture::start(Source::Mic((!mic.is_empty()).then_some(mic)), move |incoming| {
            append(&mut writing.lock().unwrap(), incoming);
            *meter.lock().unwrap() = crate::speech::source::levels(incoming).0;
        });
        let mut capture = match capture {
            Ok(capture) => { let _ = ready_tx.send(Ok(())); capture }
            Err(error) => {
                *input.active.lock().unwrap() = None;
                let _ = ready_tx.send(Err(error));
                return;
            }
        };
        let device = capture.info().name.clone();
        let started = Instant::now();
        let mut failed = None;
        while mode.load(Ordering::Acquire) == 0 && started.elapsed().as_secs() < MAX_SECONDS as u64 {
            if let Some(error) = capture.error() { failed = Some(error); break; }
            emit(&app, "recording", *level.lock().unwrap(), &device, None, None);
            std::thread::sleep(Duration::from_millis(100));
        }
        capture.stop();
        if mode.load(Ordering::Acquire) == 2 {
            emit(&app, "idle", 0.0, &device, None, None);
        } else if let Some(error) = failed {
            emit(&app, "idle", 0.0, &device, None, Some(error));
        } else {
            emit(&app, "transcribing", 0.0, &device, None, None);
            let samples = std::mem::take(&mut *samples.lock().unwrap());
            let text = speech::transcribe(&samples).map(|t| t.text).unwrap_or_default();
            if mode.load(Ordering::Acquire) != 2 {
                emit(&app, "idle", 0.0, &device, Some(text), None);
            }
        }
        *input.active.lock().unwrap() = None;
    });
    *owner.worker.lock().unwrap() = Some(worker.map_err(|_| {
        *owner.active.lock().unwrap() = None;
        "Could not start chat dictation."
    })?);
    tauri::async_runtime::spawn_blocking(move || ready_rx.recv_timeout(Duration::from_secs(5))).await
        .map_err(|_| "Could not start chat dictation.".to_string())?
        .unwrap_or_else(|_| { stopping.store(2, Ordering::Release); Err("The microphone did not start in time.".into()) })
}

#[tauri::command]
pub fn chat_dictation_stop(window: WebviewWindow, input: State<Arc<ChatInput>>, cancel: bool) -> Result<(), String> {
    from_chat(&window)?;
    if let Some(mode) = input.active.lock().unwrap().as_ref() { mode.store(if cancel { 2 } else { 1 }, Ordering::Release); }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowChoice { id: String, app_name: String, title: String }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowContext { app_name: String, title: String, url: Option<String> }

#[cfg(windows)]
struct WindowOffer { id: String, handle: isize, pid: u32, at: Instant }
#[cfg(windows)]
static WINDOWS: Mutex<Vec<WindowOffer>> = Mutex::new(Vec::new());
#[cfg(windows)]
static FRONT: Mutex<Option<WindowOffer>> = Mutex::new(None);

#[cfg(windows)]
pub fn offer_front_window() {
    *FRONT.lock().unwrap() = crate::platform::front_context_window().map(|(handle, pid)| WindowOffer {
        id: crate::flow::text::new_id(), handle, pid, at: Instant::now(),
    });
}

#[cfg(windows)]
fn take_front_window() -> Result<WindowOffer, String> {
    FRONT.lock().unwrap().take().filter(|offer| offer.at.elapsed() < Duration::from_secs(10))
        .ok_or_else(|| "Use Attach front window while the window you want is in front.".into())
}

#[tauri::command]
pub async fn chat_front_window_context(window: WebviewWindow) -> Result<WindowContext, String> {
    from_chat(&window)?;
    #[cfg(windows)]
    {
        let offer = take_front_window()?;
        tauri::async_runtime::spawn_blocking(move || {
            let (app_name, title, url) = crate::platform::selected_window_context(offer.handle, offer.pid)?;
            Ok(WindowContext { app_name, title, url })
        }).await.map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    Err("Window context is available on Windows.".into())
}

#[tauri::command]
pub fn chat_windows(window: WebviewWindow) -> Result<Vec<WindowChoice>, String> {
    from_chat(&window)?;
    #[cfg(windows)]
    {
        let mut offers = WINDOWS.lock().unwrap();
        offers.clear();
        Ok(crate::platform::context_windows().into_iter().map(|(handle, pid, app_name, title)| {
            let id = crate::flow::text::new_id();
            offers.push(WindowOffer { id: id.clone(), handle, pid, at: Instant::now() });
            WindowChoice { id, app_name, title }
        }).collect())
    }
    #[cfg(not(windows))]
    { Err("Window context is available on Windows.".into()) }
}

#[tauri::command]
pub async fn chat_window_context(window: WebviewWindow, id: String) -> Result<WindowContext, String> {
    from_chat(&window)?;
    #[cfg(windows)]
    {
        let offer = {
            let mut offers = WINDOWS.lock().unwrap();
            offers.retain(|offer| offer.at.elapsed() < Duration::from_secs(120));
            let index = offers.iter().position(|offer| offer.id == id).ok_or("Choose the window again.")?;
            offers.remove(index)
        };
        tauri::async_runtime::spawn_blocking(move || {
            let (app_name, title, url) = crate::platform::selected_window_context(offer.handle, offer.pid)?;
            Ok(WindowContext { app_name, title, url })
        }).await.map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    { let _ = id; Err("Window context is available on Windows.".into()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn a_front_window_offer_is_single_use_and_expires() {
        let offer = |at| WindowOffer { id: "chosen".into(), handle: 123, pid: 456, at };
        *FRONT.lock().unwrap() = Some(offer(Instant::now()));
        assert_eq!(take_front_window().unwrap().handle, 123);
        assert!(take_front_window().is_err());
        *FRONT.lock().unwrap() = Some(offer(Instant::now() - Duration::from_secs(11)));
        assert!(take_front_window().is_err());
        assert!(FRONT.lock().unwrap().is_none());
    }
    #[test]
    fn dictation_never_keeps_more_than_two_minutes_of_audio() {
        let mut samples = vec![0.1; speech::SAMPLE_RATE as usize * MAX_SECONDS - 2];
        append(&mut samples, &[0.2, 0.3, 0.4, 0.5]);
        assert_eq!(samples.len(), speech::SAMPLE_RATE as usize * MAX_SECONDS);
        assert_eq!(&samples[samples.len() - 2..], &[0.2, 0.3]);
        append(&mut samples, &[0.6]);
        assert_eq!(*samples.last().unwrap(), 0.3);
    }
}
