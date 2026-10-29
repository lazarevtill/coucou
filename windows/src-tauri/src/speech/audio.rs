// Sound in: the microphone (or what the computer plays, for the Notetaker)
// turned into 16 kHz mono for the models, the nine bands the pill draws, and
// the WAV files a call is kept in until its transcript is written.
//
// cpal opens the device. On Windows, opening the speakers as an input is
// WASAPI's loopback: what the computer plays. On Linux, PulseAudio (or
// PipeWire's PulseAudio server) lists a "monitor" source for every output.

use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Serialize;

use super::SAMPLE_RATE;
use crate::flow::logic::bands;

// ── Resampling ────────────────────────────────────────────────────────────────

/// Any rate → 16 kHz, with a windowed-sinc filter (32 taps): clean enough for
/// speech, and a few hundred thousand multiplications a second.
pub struct Resampler {
    /// Input samples per output sample.
    step: f64,
    /// Position of the next output sample, in input samples from `buf[0]`.
    pos: f64,
    buf: Vec<f32>,
    coefficients: Vec<[f64; 32]>,
}

const HALF_TAPS: isize = 16;

impl Resampler {
    pub fn new(input_rate: u32) -> Self {
        let step = input_rate as f64 / SAMPLE_RATE as f64;
        let cutoff = (1.0 / step).min(1.0) * 0.92;
        let coefficients = (0..1024).map(|phase| {
            let frac = phase as f64 / 1024.0;
            let mut row = [0.0; 32];
            for (index, coefficient) in row.iter_mut().enumerate() {
                let x = index as f64 - (HALF_TAPS - 1) as f64 - frac;
                let sinc = if x.abs() < 1e-9 { 1.0 } else { (std::f64::consts::PI * x * cutoff).sin() / (std::f64::consts::PI * x * cutoff) };
                let n = (x + HALF_TAPS as f64) / (2.0 * HALF_TAPS as f64);
                let window = 0.42 - 0.5 * (2.0 * std::f64::consts::PI * n).cos() + 0.08 * (4.0 * std::f64::consts::PI * n).cos();
                *coefficient = sinc * window;
            }
            let norm: f64 = row.iter().sum();
            for coefficient in &mut row { *coefficient /= norm; }
            row
        }).collect();
        Self { step, pos: HALF_TAPS as f64, buf: vec![0.0; HALF_TAPS as usize], coefficients }
    }

    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if (self.step - 1.0).abs() < 1e-9 {
            out.extend_from_slice(input);
            return;
        }
        self.buf.extend_from_slice(input);
        while self.pos + (HALF_TAPS as f64) < self.buf.len() as f64 {
            let centre = self.pos.floor() as isize;
            let frac = self.pos - centre as f64;
            // Coefficients are prepared once per device, not recalculated
            // with sin/cos for every sample on the real-time audio thread.
            let phase = (frac * 1024.0).floor().clamp(0.0, 1023.0) as usize;
            let start = (centre - HALF_TAPS + 1) as usize;
            let value: f64 = self.buf[start..start + 32].iter().zip(&self.coefficients[phase])
                .map(|(&sample, &coefficient)| sample as f64 * coefficient).sum();
            out.push(value as f32);
            self.pos += self.step;
        }
        // Keep what the next samples still need.
        let keep_from = (self.pos.floor() as isize - HALF_TAPS).max(0) as usize;
        if keep_from > 0 {
            self.buf.drain(..keep_from);
            self.pos -= keep_from as f64;
        }
    }
}

// ── The nine bands ────────────────────────────────────────────────────────────

const FFT: usize = 512;

/// The pill's nine dots, from 16 kHz audio: a Hann-windowed FFT every 512
/// samples, each band's power in dB, smoothed a little.
pub struct BandMeter {
    pending: Vec<f32>,
    ranges: Vec<(usize, usize)>,
    pub levels: [f32; bands::COUNT],
}

impl Default for BandMeter {
    fn default() -> Self {
        Self { pending: Vec::with_capacity(FFT), ranges: bands::bin_ranges(FFT, SAMPLE_RATE as f64, bands::COUNT), levels: [0.0; bands::COUNT] }
    }
}

impl BandMeter {
    /// Feeds 16 kHz samples; true when the levels changed.
    pub fn feed(&mut self, samples: &[f32]) -> bool {
        let mut changed = false;
        for &s in samples {
            self.pending.push(s);
            if self.pending.len() == FFT {
                self.measure();
                self.pending.clear();
                changed = true;
            }
        }
        changed
    }

    fn measure(&mut self) {
        let mut re = vec![0.0f64; FFT];
        let mut im = vec![0.0f64; FFT];
        for (i, &s) in self.pending.iter().enumerate() {
            let w = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (FFT - 1) as f64).cos();
            re[i] = s as f64 * w;
        }
        fft(&mut re, &mut im);
        for (b, &(from, to)) in self.ranges.iter().enumerate() {
            let mut power = 0.0;
            for k in from..to {
                power += re[k] * re[k] + im[k] * im[k];
            }
            power /= ((to - from).max(1) * FFT) as f64;
            let db = 10.0 * (power + 1e-12).log10();
            let target = bands::level(db) as f32;
            // Rise fast, fall slower: the dots follow the voice without flickering.
            let k = if target > self.levels[b] { 0.6 } else { 0.25 };
            self.levels[b] += (target - self.levels[b]) * k;
        }
    }
}

/// In-place radix-2 FFT (the length is a power of two).
pub fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = -2.0 * std::f64::consts::PI / len as f64;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (s, c) = (angle * k as f64).sin_cos();
                let a = start + k;
                let b = a + len / 2;
                let tr = re[b] * c - im[b] * s;
                let ti = re[b] * s + im[b] * c;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        len <<= 1;
    }
}

// ── Capture ───────────────────────────────────────────────────────────────────

/// Where the sound comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// A microphone by name, or the default one.
    Mic(Option<String>),
    /// What the computer plays (loopback / monitor).
    System(Option<String>),
}

/// A microphone the user can pick in Settings.
#[derive(Debug, Clone, Serialize)]
pub struct InputDevice {
    pub id: String,
    pub name: String,
    pub default: bool,
}

fn device_name(d: &cpal::Device) -> String {
    d.description().map(|desc| desc.name().to_string()).unwrap_or_default()
}

fn device_id(d: &cpal::Device) -> String {
    d.id().map(|id| id.to_string()).unwrap_or_default()
}

/// The microphones, the default first. Monitors of outputs are left out.
pub fn input_devices() -> Vec<InputDevice> {
    let host = cpal::default_host();
    let default = communication_device(&host, true).or_else(|| host.default_input_device()).map(|d| device_id(&d));
    let mut out = Vec::new();
    if let Ok(devices) = host.input_devices() {
        for d in devices {
            let name = device_name(&d);
            if name.is_empty() || is_monitor(&d) {
                continue;
            }
            let id = device_id(&d);
            out.push(InputDevice { default: Some(&id) == default.as_ref(), id, name });
        }
    }
    distinct_devices(out)
}

pub fn output_devices() -> Vec<InputDevice> {
    let host = cpal::default_host();
    let default = communication_device(&host, false).or_else(|| host.default_output_device()).map(|d| device_id(&d));
    let devices: Vec<_> = host.output_devices().into_iter().flatten().map(|d| {
        let name = device_name(&d);
        let id = device_id(&d);
        InputDevice { default: Some(&id) == default.as_ref(), id, name }
    }).filter(|d| !d.name.is_empty()).collect();
    distinct_devices(devices)
}

/// Identically named USB/Bluetooth endpoints are separate choices. Use a stable
/// short identity only where the friendly names cannot distinguish them.
fn distinct_devices(mut devices: Vec<InputDevice>) -> Vec<InputDevice> {
    devices.sort_by(|a, b| (!a.default, &a.name, &a.id).cmp(&(!b.default, &b.name, &b.id)));
    let mut seen = std::collections::HashSet::new();
    devices.retain(|d| seen.insert(if d.id.is_empty() { format!("name:{}", d.name) } else { format!("id:{}", d.id) }));
    let mut counts = std::collections::HashMap::new();
    for device in &devices { *counts.entry(device.name.clone()).or_insert(0) += 1; }
    for device in &mut devices {
        if counts[&device.name] > 1 {
            let tail: String = device.id.chars().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect();
            device.name = format!("{} · {tail}", device.name);
        }
    }
    devices
}

pub fn selected_device_id(source: &Source) -> Option<String> {
    open_device(source).map(|d| device_id(&d))
}

fn is_monitor(d: &cpal::Device) -> bool {
    let id = device_id(d).to_lowercase();
    let name = device_name(d).to_lowercase();
    id.ends_with(".monitor") || name.starts_with("monitor of")
}

fn open_device(source: &Source) -> Option<cpal::Device> {
    let host = cpal::default_host();
    match source {
        Source::Mic(Some(name)) => host
            .input_devices()
            .ok()
            .and_then(|mut it| it.find(|d| device_id(d) == *name || device_name(d) == *name)),
        Source::Mic(None) => communication_device(&host, true).or_else(|| host.default_input_device()),
        Source::System(Some(name)) => host.output_devices().ok()?.find(|d| device_id(d) == *name || device_name(d) == *name)
            .and_then(|d| loopback_device(&host, d)),
        Source::System(None) => system_device(&host),
    }
}

#[cfg(windows)]
fn system_device(host: &cpal::Host) -> Option<cpal::Device> {
    communication_device(host, false).or_else(|| host.default_output_device())
}

#[cfg(windows)]
fn loopback_device(_: &cpal::Host, output: cpal::Device) -> Option<cpal::Device> { Some(output) }

#[cfg(not(windows))]
fn system_device(host: &cpal::Host) -> Option<cpal::Device> {
    loopback_device(host, host.default_output_device()?)
}

#[cfg(not(windows))]
fn loopback_device(host: &cpal::Host, sink: cpal::Device) -> Option<cpal::Device> {
    let sink_id = device_id(&sink);
    let wanted = format!("{}.monitor", sink_id.rsplit(':').next().unwrap_or(&sink_id));
    let mut monitors: Vec<cpal::Device> = host.input_devices().ok()?.filter(is_monitor).collect();
    let pos = monitors.iter().position(|d| device_id(d).ends_with(&wanted));
    match pos {
        Some(i) => Some(monitors.swap_remove(i)),
        None => monitors.into_iter().next(),
    }
}

#[cfg(not(windows))]
fn communication_device(_: &cpal::Host, _: bool) -> Option<cpal::Device> { None }

#[cfg(windows)]
fn communication_device(host: &cpal::Host, input: bool) -> Option<cpal::Device> {
    use windows::Win32::{Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator, eCapture, eRender, eCommunications},
        System::Com::{CoInitializeEx, CoUninitialize, CoCreateInstance, CoTaskMemFree, COINIT_MULTITHREADED, CLSCTX_ALL}};
    struct Com(bool);
    impl Drop for Com { fn drop(&mut self) { if self.0 { unsafe { CoUninitialize(); } } } }
    unsafe {
        let _com = Com(CoInitializeEx(None, COINIT_MULTITHREADED).is_ok());
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let endpoint = enumerator.GetDefaultAudioEndpoint(if input { eCapture } else { eRender }, eCommunications).ok()?;
        let raw = endpoint.GetId().ok()?;
        let id = raw.to_string();
        CoTaskMemFree(Some(raw.0.cast()));
        host.device_by_id(&cpal::DeviceId::new(cpal::HostId::Wasapi, id.ok()?))
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureInfo {
    pub id: String,
    pub name: String,
    pub sample_rate: u32,
    pub channels: u16,
}

/// An open input. Dropping it (or `stop`) closes the device.
pub struct Capture {
    stop: Option<mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
    health: Arc<Mutex<CaptureHealth>>,
    info: CaptureInfo,
}

#[derive(Default)]
struct CaptureHealth { error: Option<String>, warnings: Vec<String> }

fn capture_error(health: &mut CaptureHealth, error: cpal::Error) {
    if error.kind() == cpal::ErrorKind::Xrun {
        // WASAPI continues delivering packets after DATA_DISCONTINUITY. Reopening
        // the stream would create another avoidable gap, especially on Bluetooth.
        let warning = crate::i18n::t("Audio was interrupted. Recording continues.");
        if !health.warnings.contains(&warning) { health.warnings.push(warning); }
    } else {
        health.error = Some(error.to_string());
    }
}

impl Capture {
    /// Opens `source` and calls `sink` with 16 kHz mono samples from the audio
    /// thread. The stream lives on its own thread (cpal streams can't move).
    pub fn start(source: Source, mut sink: impl FnMut(&[f32]) + Send + 'static) -> Result<Capture, String> {
        Self::start_timed(source, Instant::now(), move |samples, _| sink(samples))
    }

    pub fn start_timed(source: Source, origin: Instant, sink: impl FnMut(&[f32], Duration) + Send + 'static) -> Result<Capture, String> {
        let health = Arc::new(Mutex::new(CaptureHealth::default()));
        let stream_error = health.clone();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<CaptureInfo, String>>();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let thread = std::thread::Builder::new()
            .name("coucou-audio".into())
            .spawn(move || {
                let (stream, info) = match build_stream(&source, origin, stream_error, sink) {
                    Ok(s) => s,
                    Err(err) => {
                        let _ = ready_tx.send(Err(err));
                        return;
                    }
                };
                if let Err(err) = stream.play() {
                    let _ = ready_tx.send(Err(err.to_string()));
                    return;
                }
                let _ = ready_tx.send(Ok(info));
                let _ = stop_rx.recv();
                drop(stream);
            })
            .map_err(|e| e.to_string())?;
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(info)) => Ok(Capture { stop: Some(stop_tx), thread: Some(thread), health, info }),
            Ok(Err(err)) => Err(err),
            Err(_) => Err("the audio device did not answer".into()),
        }
    }

    pub fn info(&self) -> &CaptureInfo { &self.info }

    pub fn error(&self) -> Option<String> { self.health.lock().unwrap().error.clone() }

    pub fn warnings(&self) -> Vec<String> { self.health.lock().unwrap().warnings.clone() }

    pub fn stop(&mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop();
    }
}

fn build_stream(source: &Source, origin: Instant, health: Arc<Mutex<CaptureHealth>>, mut sink: impl FnMut(&[f32], Duration) + Send + 'static) -> Result<(cpal::Stream, CaptureInfo), String> {
    let device = open_device(source).ok_or_else(|| crate::i18n::t(if matches!(source, Source::Mic(Some(_)) | Source::System(Some(_))) { "Selected device (disconnected)" } else { "No device" }))?;
    let supported = if matches!(source, Source::System(_)) && cfg!(windows) {
        device.default_output_config().map_err(|e| e.to_string())?
    } else {
        device.default_input_config().map_err(|e| e.to_string())?
    };
    let devices = if matches!(source, Source::Mic(_)) { input_devices() } else { output_devices() };
    let name = devices.into_iter().find(|d| d.id == device_id(&device)).map(|d| d.name).unwrap_or_else(|| device_name(&device));
    let info = CaptureInfo { id: device_id(&device), name, sample_rate: supported.sample_rate(), channels: supported.channels() };
    let channels = supported.channels().max(1) as usize;
    let rate = supported.sample_rate();
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let mut resampler = Resampler::new(rate);
    let mut mono: Vec<f32> = Vec::with_capacity(4096);
    let mut out: Vec<f32> = Vec::with_capacity(4096);
    let mut push = move |frame_samples: &mut dyn Iterator<Item = f32>, info: &cpal::InputCallbackInfo| {
        let stamp = info.timestamp();
        let at = origin.elapsed().saturating_sub(stamp.callback.duration_since(stamp.capture));
        mono.clear();
        let mut acc = 0.0f32;
        let mut n = 0usize;
        for s in frame_samples {
            acc += s;
            n += 1;
            if n == channels {
                mono.push(acc / channels as f32);
                acc = 0.0;
                n = 0;
            }
        }
        out.clear();
        resampler.process(&mono, &mut out);
        if !out.is_empty() {
            sink(&out, at);
        }
    };
    let err_fn = move |err: cpal::Error| {
        // No file I/O on the real-time callback. The source meter and saved note
        // expose interruptions; terminal device errors still trigger recovery.
        capture_error(&mut health.lock().unwrap(), err);
    };
    let stream = match format {
        cpal::SampleFormat::F32 => device.build_input_stream::<f32, _, _>(
            config,
            move |data: &[f32], info| push(&mut data.iter().copied(), info),
            err_fn,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream::<i16, _, _>(
            config,
            move |data: &[i16], info| push(&mut data.iter().map(|&s| s as f32 / 32768.0), info),
            err_fn,
            None,
        ),
        cpal::SampleFormat::I32 => device.build_input_stream::<i32, _, _>(
            config,
            move |data: &[i32], info| push(&mut data.iter().map(|&s| s as f32 / 2_147_483_648.0), info),
            err_fn,
            None,
        ),
        cpal::SampleFormat::I24 => device.build_input_stream::<cpal::I24, _, _>(
            config,
            move |data: &[cpal::I24], info| push(&mut data.iter().map(|&s| s.inner() as f32 / 8_388_608.0), info),
            err_fn,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream::<u16, _, _>(
            config,
            move |data: &[u16], info| push(&mut data.iter().map(|&s| (s as f32 - 32768.0) / 32768.0), info),
            err_fn,
            None,
        ),
        other => return Err(format!("unsupported sample format {other:?}")),
    };
    stream.map(|stream| (stream, info)).map_err(|e| e.to_string())
}

// ── A recording in memory ─────────────────────────────────────────────────────

/// What a dictation keeps while you speak: the samples, the pill's bands and
/// the loudest moment (silence is never sent to the model).
#[derive(Default)]
pub struct Take {
    pub samples: Vec<f32>,
    pub meter: BandMeter,
    pub peak: f32,
}

// ── Cutting long audio ────────────────────────────────────────────────────────

/// Long audio in stretches the model takes at once, each cut at the quietest
/// 100 ms near its end. Returns (offset in samples, samples).
pub fn split_for_model(samples: &[f32], rate: u32, max_seconds: f64) -> Vec<(usize, &[f32])> {
    let max = (max_seconds * rate as f64) as usize;
    let window = (rate as usize) / 10;
    let mut out = Vec::new();
    let mut start = 0;
    while start < samples.len() {
        if samples.len() - start <= max {
            out.push((start, &samples[start..]));
            break;
        }
        // Look for the quietest window in the last 8 s of the stretch.
        let search_from = start + max.saturating_sub(8 * rate as usize);
        let mut best = start + max;
        let mut best_energy = f64::MAX;
        let mut at = search_from;
        while at + window <= start + max {
            let e: f64 = samples[at..at + window].iter().map(|&s| (s as f64) * (s as f64)).sum();
            if e < best_energy {
                best_energy = e;
                best = at + window / 2;
            }
            at += window / 2;
        }
        out.push((start, &samples[start..best]));
        start = best;
    }
    out
}

// ── WAV files ─────────────────────────────────────────────────────────────────

/// A 16 kHz mono 16-bit WAV written as the samples come in.
pub struct WavWriter {
    out: BufWriter<File>,
    samples: u32,
}

impl WavWriter {
    pub fn create(path: &Path) -> std::io::Result<WavWriter> {
        let mut out = BufWriter::new(File::create(path)?);
        write_header(&mut out, 0)?;
        Ok(WavWriter { out, samples: 0 })
    }

    pub fn write(&mut self, samples: &[f32]) -> std::io::Result<()> {
        for &s in samples {
            let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
            self.out.write_all(&v.to_le_bytes())?;
        }
        self.samples = self.samples.checked_add(u32::try_from(samples.len()).map_err(|_| std::io::Error::other("recording too long"))?)
            .filter(|n| *n <= (u32::MAX - 36) / 2).ok_or_else(|| std::io::Error::other("recording too long"))?;
        Ok(())
    }

    /// Writes the sizes into the header and closes the file.
    pub fn finish(mut self) -> std::io::Result<()> {
        self.out.flush()?;
        let mut file = self.out.into_inner().map_err(|e| e.into_error())?;
        file.seek(SeekFrom::Start(0))?;
        write_header(&mut file, self.samples)?;
        file.flush()
    }
}

fn write_header(out: &mut impl Write, samples: u32) -> std::io::Result<()> {
    let data = samples.saturating_mul(2);
    out.write_all(b"RIFF")?;
    out.write_all(&(36u32.saturating_add(data)).to_le_bytes())?;
    out.write_all(b"WAVEfmt ")?;
    out.write_all(&16u32.to_le_bytes())?;
    out.write_all(&1u16.to_le_bytes())?; // PCM
    out.write_all(&1u16.to_le_bytes())?; // mono
    out.write_all(&SAMPLE_RATE.to_le_bytes())?;
    out.write_all(&(SAMPLE_RATE * 2).to_le_bytes())?;
    out.write_all(&2u16.to_le_bytes())?;
    out.write_all(&16u16.to_le_bytes())?;
    out.write_all(b"data")?;
    out.write_all(&data.to_le_bytes())
}

/// Reads a WAV written by `WavWriter` (or cut short by a crash: the data runs
/// to the end of the file whatever the header says).
pub fn read_wav(path: &Path) -> std::io::Result<Vec<f32>> {
    let mut bytes = Vec::new();
    File::open(path)?.read_to_end(&mut bytes)?;
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "not a WAV file"));
    }
    Ok(bytes[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect())
}


#[cfg(test)]
mod tests {
    use super::*;

    /// A temporary file, removed when dropped.
    struct TempFile(std::path::PathBuf);

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn tone(freq: f64, rate: u32, seconds: f64) -> Vec<f32> {
        (0..(rate as f64 * seconds) as usize)
            .map(|i| (2.0 * std::f64::consts::PI * freq * i as f64 / rate as f64).sin() as f32 * 0.5)
            .collect()
    }

    #[test]
    fn a_discontinuity_is_preserved_without_invalidating_a_working_stream() {
        let mut health = CaptureHealth::default();
        capture_error(&mut health, cpal::ErrorKind::Xrun.into());
        capture_error(&mut health, cpal::ErrorKind::Xrun.into());
        assert!(health.error.is_none());
        assert_eq!(health.warnings, ["Audio was interrupted. Recording continues."]);
        capture_error(&mut health, cpal::ErrorKind::DeviceNotAvailable.into());
        assert!(health.error.is_some());
        assert_eq!(health.warnings.len(), 1);
    }

    #[test]
    fn same_named_audio_endpoints_keep_separate_stable_choices() {
        let device = |id: &str, default| InputDevice { id: id.into(), name: "USB Headset".into(), default };
        let choices = distinct_devices(vec![device("endpoint-00000002", false), device("endpoint-00000001", true), device("endpoint-00000002", false)]);
        assert_eq!(choices.len(), 2);
        assert!(choices[0].default);
        assert_eq!(choices[0].id, "endpoint-00000001");
        assert_ne!(choices[0].name, choices[1].name);
        let reversed = distinct_devices(vec![device("endpoint-00000001", true), device("endpoint-00000002", false)]);
        assert_eq!(choices.iter().map(|d| &d.name).collect::<Vec<_>>(), reversed.iter().map(|d| &d.name).collect::<Vec<_>>());
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "Lists real audio endpoints; opt-in hardware diagnostic"]
    fn live_audio_devices() {
        println!("microphones: {}", serde_json::to_string(&input_devices()).unwrap());
        println!("playback: {}", serde_json::to_string(&output_devices()).unwrap());
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "Opens the real microphone and playback endpoint and plays a very quiet test tone; requires explicit permission"]
    fn live_dual_capture_quiet() {
        #[derive(Default)]
        struct Stats { samples: usize, peak: f32, last: Duration }
        let origin = Instant::now();
        let mut captures = Vec::new();
        let mut stats = Vec::new();
        // Exact IDs allow a laptop mic plus Bluetooth/wired/USB output to be
        // tested independently of the Windows communications defaults.
        let microphone = Source::Mic(std::env::var("COUCOU_TEST_MIC_ID").ok().filter(|v| !v.is_empty()));
        let playback = Source::System(std::env::var("COUCOU_TEST_OUTPUT_ID").ok().filter(|v| !v.is_empty()));
        for source in [microphone, playback.clone()] {
            let measured = Arc::new(Mutex::new(Stats::default()));
            let output = measured.clone();
            let capture = Capture::start_timed(source.clone(), origin, move |samples, at| {
                let mut state = output.lock().unwrap();
                state.samples += samples.len();
                state.peak = samples.iter().fold(state.peak, |peak, sample| peak.max(sample.abs()));
                state.last = at;
            }).unwrap_or_else(|error| panic!("{source:?}: {error}"));
            println!("capture {source:?}: {} · {} Hz · {} channels", capture.info().name, capture.info().sample_rate, capture.info().channels);
            captures.push(capture);
            stats.push(measured);
        }
        let device = open_device(&playback).expect("playback endpoint");
        let supported = device.default_output_config().expect("output format");
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let output_error = Arc::new(Mutex::new(None));
        fn tone_stream<T: cpal::SizedSample + cpal::FromSample<f32>>(device: &cpal::Device, config: cpal::StreamConfig, errors: Arc<Mutex<Option<String>>>) -> cpal::Stream {
            let channels = config.channels as usize;
            let rate = config.sample_rate as f64;
            let mut frame = 0u64;
            device.build_output_stream::<T, _, _>(config, move |data, _| {
                for samples in data.chunks_mut(channels) {
                    let elapsed = frame as f64 / rate;
                    // -54 dBFS peak, with soft edges; never changes the user's mixer volume.
                    let envelope = (elapsed / 0.05).min(1.0) * ((2.0 - elapsed) / 0.05).clamp(0.0, 1.0);
                    let sample = (0.002 * envelope * (std::f64::consts::TAU * 440.0 * elapsed).sin()) as f32;
                    samples.fill(T::from_sample(sample));
                    frame += 1;
                }
            }, move |error| { *errors.lock().unwrap() = Some(error.to_string()); }, None).expect("quiet output stream")
        }
        let tone = match format {
            cpal::SampleFormat::F32 => tone_stream::<f32>(&device, config, output_error.clone()),
            cpal::SampleFormat::I16 => tone_stream::<i16>(&device, config, output_error.clone()),
            cpal::SampleFormat::I32 => tone_stream::<i32>(&device, config, output_error.clone()),
            other => panic!("test tone does not support {other:?}"),
        };
        tone.play().unwrap();
        std::thread::sleep(Duration::from_secs(3));
        drop(tone);
        for (index, capture) in captures.iter_mut().enumerate() {
            capture.stop();
            assert!(capture.error().is_none(), "capture error: {:?}", capture.error());
            println!("source {index} warnings: {:?}", capture.warnings());
            let state = stats[index].lock().unwrap();
            println!("source {index}: samples={} peak={:.6} elapsed={:.3}s", state.samples, state.peak, state.last.as_secs_f64());
            assert!(state.samples > SAMPLE_RATE as usize, "source {index} returned no sustained audio stream");
        }
        assert!(output_error.lock().unwrap().is_none(), "output error");
        assert!(stats[1].lock().unwrap().peak > 0.00001, "playback stream remained silent");
        println!("microphone and digital playback captured together; only aggregate levels retained, no recording saved");
    }

    #[test]
    fn resampling_keeps_the_length_and_the_tone() {
        let input = tone(440.0, 48_000, 1.0);
        let mut r = Resampler::new(48_000);
        let mut out = Vec::new();
        for chunk in input.chunks(480) {
            r.process(chunk, &mut out);
        }
        assert!((out.len() as i64 - 16_000).abs() < 40, "{} samples", out.len());
        // Zero crossings of a 440 Hz tone over the middle half second.
        let mid = &out[4000..12000];
        let crossings = mid.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        assert!((crossings as i64 - 220).abs() <= 2, "{crossings}");
        let peak = mid.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
        assert!((peak - 0.5).abs() < 0.05, "{peak}");
    }

    #[test]
    fn resampling_suppresses_aliases_and_preserves_non_integer_rate_speech() {
        for rate in [44_100, 48_000] {
            let mut converter = Resampler::new(rate);
            let mut speech = Vec::new();
            for chunk in tone(1000.0, rate, 1.0).chunks(441) { converter.process(chunk, &mut speech); }
            let rms = |v: &[f32]| (v.iter().map(|x| x*x).sum::<f32>() / v.len() as f32).sqrt();
            assert!((rms(&speech[2000..14000]) - 0.35355).abs() < 0.02);
            let mut converter = Resampler::new(rate);
            let mut high = Vec::new();
            converter.process(&tone(12_000.0, rate, 1.0), &mut high);
            assert!(rms(&high[2000..14000]) < 0.01, "alias from {rate} Hz");
        }
    }

    #[test]
    fn a_low_note_and_a_high_note_light_different_dots() {
        let mut low = BandMeter::default();
        low.feed(&tone(150.0, 16_000, 0.5));
        let mut high = BandMeter::default();
        high.feed(&tone(5000.0, 16_000, 0.5));
        let argmax = |l: &[f32; 9]| l.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).unwrap().0;
        assert!(argmax(&low.levels) <= 1);
        assert!(argmax(&high.levels) >= 7);
        let mut silent = BandMeter::default();
        silent.feed(&vec![0.0; 8000]);
        assert!(silent.levels.iter().all(|&v| v < 0.01));
    }

    #[test]
    fn long_audio_is_cut_in_the_quiet() {
        let rate = 16_000;
        let mut samples = tone(300.0, rate, 25.0);
        samples.extend(vec![0.0; rate as usize]); // a pause from 25 s to 26 s
        samples.extend(tone(300.0, rate, 20.0));
        let parts = split_for_model(&samples, rate, 28.0);
        assert_eq!(parts.len(), 2);
        let cut = parts[1].0 as f64 / rate as f64;
        assert!((25.0..=26.0).contains(&cut), "cut at {cut}");
        assert_eq!(parts.iter().map(|p| p.1.len()).sum::<usize>(), samples.len());
        assert_eq!(split_for_model(&samples[..1000], rate, 28.0).len(), 1);
    }

    #[test]
    fn a_wav_reads_back_what_was_written() {
        let path = std::env::temp_dir().join(format!("coucou-wav-{}.wav", std::process::id()));
        let _cleanup = TempFile(path.clone());
        let mut w = WavWriter::create(&path).unwrap();
        w.write(&[0.0, 0.5, -0.5, 1.0]).unwrap();
        w.finish().unwrap();
        let back = read_wav(&path).unwrap();
        assert_eq!(back.len(), 4);
        assert!((back[1] - 0.5).abs() < 1e-3 && (back[2] + 0.5).abs() < 1e-3);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]), 8);
    }
}
