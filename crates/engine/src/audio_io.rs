//! Real audio devices via cpal (ALSA / PipeWire-ALSA / JACK on Linux).
//! Streams live on their own threads because cpal streams are not `Send`
//! on every platform.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::engine::{Engine, EngineEndpoints};

/// Connection state of the output, shared with the UI.
pub struct AudioState {
    connected: AtomicBool,
    device: Mutex<String>,
    /// What the person chose: an output by name (`None` follows the system's default) and a
    /// buffer in frames (`None` lets the device decide).
    choice: Mutex<Choice>,
    /// The buffer the open stream really uses, in frames, when the device says.
    buffer: Mutex<Option<u32>>,
}

/// The output and buffer the person chose.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Choice {
    pub device: Option<String>,
    pub buffer: Option<u32>,
}

/// Buffer sizes offered, in frames; smaller is less latency and more risk of dropouts.
pub const BUFFERS: [u32; 5] = [64, 128, 256, 512, 1024];

impl AudioState {
    pub fn connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }
    pub fn device_name(&self) -> String {
        self.device.lock().map(|d| d.clone()).unwrap_or_default()
    }
    fn set(&self, connected: bool, name: &str) {
        self.connected.store(connected, Ordering::Relaxed);
        if let Ok(mut d) = self.device.lock() {
            *d = name.to_string();
        }
    }
}

enum Msg {
    Stop,
    /// The choice of output or buffer changed: reopen with it.
    Reopen,
    /// The stream died (device unplugged, server restarted…): rebuild it.
    Lost(String),
}

/// Keeps the output running; dropping it stops audio.
pub struct AudioOutput {
    tx: mpsc::Sender<Msg>,
    pub sample_rate: u32,
    pub state: Arc<AudioState>,
}

impl AudioOutput {
    /// Pretend the device vanished (for tests and diagnostics): the
    /// supervisor drops the stream and reconnects like after an unplug.
    #[doc(hidden)]
    pub fn simulate_device_loss(&self) {
        let _ = self.tx.send(Msg::Lost("simulated".into()));
    }

    /// Every output device the system offers, by name.
    pub fn devices(&self) -> Vec<String> {
        output_devices()
    }

    pub fn choice(&self) -> Choice {
        self.state
            .choice
            .lock()
            .map(|c| c.clone())
            .unwrap_or_default()
    }

    /// Choose an output (`None`: the system's default) and a buffer (`None`: the device's);
    /// the stream reopens with them. A choice the device refuses falls back and says so.
    pub fn choose(&self, choice: Choice) {
        if let Ok(mut c) = self.state.choice.lock() {
            *c = choice;
        }
        let _ = self.tx.send(Msg::Reopen);
    }

    /// The open stream's buffer in frames and its latency in milliseconds, when known.
    pub fn latency(&self) -> Option<(u32, f32)> {
        let frames = (*self.state.buffer.lock().ok()?)?;
        Some((frames, frames as f32 * 1000.0 / self.sample_rate as f32))
    }

    pub fn device_name(&self) -> String {
        self.state.device_name()
    }
    /// False while no device is open (a silent clock keeps the engine going).
    pub fn real_device(&self) -> bool {
        self.state.connected()
    }
}

impl Drop for AudioOutput {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Stop);
    }
}

const NO_AUDIO: &str = "No audio";
/// Retry delays grow from 2 s to 10 s while no device is available.
const RETRY_MIN: Duration = Duration::from_secs(2);
const RETRY_MAX: Duration = Duration::from_secs(10);

/// Create the engine (at the default device's sample rate, or 48 kHz) and
/// keep it running for the life of the app. A supervisor thread owns the
/// device: if it is missing or gets lost, the engine keeps running on a
/// silent real-time clock and the device is retried every couple of seconds.
pub fn start_output() -> (AudioOutput, EngineEndpoints) {
    let sample_rate = cpal::default_host()
        .default_output_device()
        .and_then(|d| d.default_output_config().ok())
        .map(|c| c.sample_rate())
        .unwrap_or(48000);
    let (engine, endpoints) = Engine::new(sample_rate as f32);
    let engine = Arc::new(Mutex::new(engine));
    let state = Arc::new(AudioState {
        connected: AtomicBool::new(false),
        device: Mutex::new(NO_AUDIO.into()),
        choice: Mutex::new(Choice::default()),
        buffer: Mutex::new(None),
    });
    let (tx, rx) = mpsc::channel::<Msg>();
    let (first_tx, first_rx) = mpsc::channel::<()>();
    {
        let (state, tx) = (state.clone(), tx.clone());
        thread::Builder::new()
            .name("yggdrasil-audio".into())
            .spawn(move || supervise(engine, sample_rate, state, tx, rx, first_tx))
            .expect("spawn audio thread");
    }
    // wait for the first attempt so the UI starts with the right status
    let _ = first_rx.recv_timeout(Duration::from_secs(3));
    (
        AudioOutput {
            tx,
            sample_rate,
            state,
        },
        endpoints,
    )
}

fn supervise(
    engine: Arc<Mutex<Engine>>,
    sample_rate: u32,
    state: Arc<AudioState>,
    tx: mpsc::Sender<Msg>,
    rx: mpsc::Receiver<Msg>,
    first: mpsc::Sender<()>,
) {
    let mut first = Some(first);
    let mut last_error = String::new();
    let mut retry = RETRY_MIN;
    loop {
        let choice = state.choice.lock().map(|c| c.clone()).unwrap_or_default();
        match open_stream(sample_rate, &choice, engine.clone(), tx.clone()) {
            Ok((stream, name, buffer)) => {
                if let Ok(mut b) = state.buffer.lock() {
                    *b = buffer;
                }
                state.set(true, &name);
                retry = RETRY_MIN;
                if !last_error.is_empty() {
                    eprintln!("yggdrasil: audio reconnected ({name})");
                    last_error.clear();
                }
                if let Some(f) = first.take() {
                    let _ = f.send(());
                }
                // block until told to stop, to reopen, or the stream dies
                match rx.recv() {
                    Ok(Msg::Reopen) => {
                        drop(stream);
                        continue;
                    }
                    Ok(Msg::Lost(why)) => {
                        drop(stream);
                        state.set(false, NO_AUDIO);
                        eprintln!("yggdrasil: audio output lost ({why}); retrying…");
                        last_error = why;
                    }
                    Ok(Msg::Stop) | Err(_) => return,
                }
            }
            Err(e) => {
                state.set(false, NO_AUDIO);
                if e != last_error {
                    eprintln!("yggdrasil: no audio output ({e}); the engine keeps running on a silent clock");
                    last_error = e;
                }
                if let Some(f) = first.take() {
                    let _ = f.send(());
                }
            }
        }
        // No device: drive the engine silently in real time until the next retry.
        if !run_silent(&engine, sample_rate, &rx, retry) {
            return;
        }
        retry = (retry * 2).min(RETRY_MAX);
    }
}

/// Process the engine at real-time pace without a device. Returns false if
/// asked to stop.
fn run_silent(
    engine: &Arc<Mutex<Engine>>,
    sample_rate: u32,
    rx: &mpsc::Receiver<Msg>,
    duration: Duration,
) -> bool {
    let block = 256usize;
    let mut buf = vec![0.0f32; block * 2];
    let start = Instant::now();
    let mut frames: u64 = 0;
    while start.elapsed() < duration {
        match rx.try_recv() {
            Ok(Msg::Stop) | Err(mpsc::TryRecvError::Disconnected) => return false,
            // a new choice is worth trying at once
            Ok(Msg::Reopen) => return true,
            Ok(Msg::Lost(_)) | Err(mpsc::TryRecvError::Empty) => {}
        }
        let due = (start.elapsed().as_secs_f64() * sample_rate as f64) as u64;
        if let Ok(mut eng) = engine.lock() {
            while frames + block as u64 <= due {
                eng.process(&mut buf, 2);
                frames += block as u64;
            }
        }
        thread::sleep(Duration::from_millis(3));
    }
    true
}

fn device_label(device: &cpal::Device) -> String {
    device
        .description()
        .map(|d| d.to_string())
        .unwrap_or_else(|_| "Audio output".into())
}

/// Every output device the default host offers, by name.
pub fn output_devices() -> Vec<String> {
    let mut names: Vec<String> = cpal::default_host()
        .output_devices()
        .map(|all| all.map(|d| device_label(&d)).collect())
        .unwrap_or_default();
    names.dedup();
    names
}

/// Open the chosen output (else the default) at exactly `sample_rate` (the engine's rate), with
/// the chosen buffer (else 256 frames, else whatever the device gives). Returns the stream, the
/// device's name and the buffer it really uses when known.
fn open_stream(
    sample_rate: u32,
    choice: &Choice,
    engine: Arc<Mutex<Engine>>,
    tx: mpsc::Sender<Msg>,
) -> Result<(cpal::Stream, String, Option<u32>), String> {
    let host = cpal::default_host();
    let chosen = choice.device.as_ref().and_then(|want| {
        host.output_devices()
            .ok()?
            .find(|d| &device_label(d) == want)
    });
    let device = match chosen {
        Some(d) => d,
        None => host.default_output_device().ok_or("no output device")?,
    };
    let name = device_label(&device);
    // prefer f32, stereo, at the engine's rate
    let mut candidates: Vec<cpal::SupportedStreamConfigRange> = device
        .supported_output_configs()
        .map_err(|e| e.to_string())?
        .filter(|r| r.min_sample_rate() <= sample_rate && sample_rate <= r.max_sample_rate())
        .collect();
    candidates.sort_by_key(|r| {
        (
            r.sample_format() != cpal::SampleFormat::F32,
            r.channels() != 2,
        )
    });
    let range = candidates
        .into_iter()
        .next()
        .ok_or_else(|| format!("the device does not support {sample_rate} Hz"))?;
    let supported = range.with_sample_rate(sample_rate);
    let channels = supported.channels() as usize;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let wanted = choice.buffer.unwrap_or(256);
    let mut fixed = config;
    fixed.buffer_size = cpal::BufferSize::Fixed(wanted);
    let (stream, buffer) =
        match build_output(&device, fixed, format, channels, engine.clone(), tx.clone()) {
            Ok(stream) => (stream, Some(wanted)),
            Err(_) => (
                build_output(&device, config, format, channels, engine, tx)?,
                None,
            ),
        };
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, name, buffer))
}

fn build_output(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    format: cpal::SampleFormat,
    channels: usize,
    engine: Arc<Mutex<Engine>>,
    tx: mpsc::Sender<Msg>,
) -> Result<cpal::Stream, String> {
    let err_fn = move |e: cpal::Error| {
        use cpal::ErrorKind::*;
        match e.kind() {
            // the stream is gone: the supervisor rebuilds it
            DeviceNotAvailable | StreamInvalidated | HostUnavailable | BackendError => {
                let _ = tx.send(Msg::Lost(e.to_string()));
            }
            // glitches and automatic reroutes: nothing to rebuild
            _ => eprintln!("yggdrasil: audio warning: {e}"),
        }
    };
    macro_rules! build {
        ($t:ty, $conv:expr) => {{
            let mut scratch: Vec<f32> = vec![0.0; 8192 * channels.max(1)];
            device.build_output_stream::<$t, _, _>(
                config,
                move |data: &mut [$t], _| {
                    if scratch.len() < data.len() {
                        // only on unexpectedly huge buffers
                        scratch.resize(data.len(), 0.0);
                    }
                    let buf = &mut scratch[..data.len()];
                    // Only this callback ever locks the engine, so this never blocks.
                    match engine.try_lock() {
                        Ok(mut eng) => eng.process(buf, channels),
                        Err(_) => buf.fill(0.0),
                    }
                    for (o, &s) in data.iter_mut().zip(buf.iter()) {
                        *o = $conv(s);
                    }
                },
                err_fn,
                None,
            )
        }};
    }
    let result = match format {
        cpal::SampleFormat::F32 => build!(f32, |s: f32| s),
        cpal::SampleFormat::I16 => build!(i16, |s: f32| (s.clamp(-1.0, 1.0) * 32767.0) as i16),
        cpal::SampleFormat::I32 => build!(i32, |s: f32| (s.clamp(-1.0, 1.0) as f64 * 2147483647.0)
            as i32),
        cpal::SampleFormat::U16 => build!(u16, |s: f32| ((s.clamp(-1.0, 1.0) * 32767.0) + 32768.0)
            as u16),
        other => return Err(format!("unsupported sample format: {other:?}")),
    };
    result.map_err(|e| e.to_string())
}
