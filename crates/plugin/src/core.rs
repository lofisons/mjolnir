//! The instrument behind the plugin, with nothing of any plugin format in it:
//! one Mjolnir track, played by a host's notes.
//!
//! Two threads share it. The audio thread owns the engine and gives it notes
//! and the host's transport as they come. A helper thread owns the controller
//! and does everything that may allocate: loading a sound, writing the drum
//! loop, following the knobs. Between them: a handful of atomics.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;

use mjolnir_engine::controller::Controller;
use mjolnir_engine::engine::{Command, Engine};
use mjolnir_engine::grooves::GrooveId;
use mjolnir_engine::macro_synth::PRESETS;
use mjolnir_engine::model::{
    DrumKitId, InstrumentSpec, MacroParam, Master, Project, SynthPreset, TextureKind, TrackKind,
    PPQ,
};

/// One sound of the list a host can choose from.
pub struct Sound {
    /// Never changes: what a saved state remembers.
    pub id: String,
    /// What the host shows.
    pub label: String,
    pub drums: bool,
    pub spec: InstrumentSpec,
}

/// Every sound, in the order of the app's own shelves.
pub fn sounds() -> &'static [Sound] {
    static ALL: OnceLock<Vec<Sound>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut v = Vec::new();
        for k in DrumKitId::ALL {
            v.push(Sound {
                id: format!("kit:{k:?}"),
                label: format!("Drums: {}", k.label()),
                drums: true,
                spec: InstrumentSpec::Drums { kit: k },
            });
        }
        for p in SynthPreset::ALL {
            v.push(Sound {
                id: format!("preset:{p:?}"),
                label: p.label().to_string(),
                drums: false,
                spec: InstrumentSpec::Synth { preset: p },
            });
        }
        for t in TextureKind::ALL {
            v.push(Sound {
                id: format!("texture:{t:?}"),
                label: t.label().to_string(),
                drums: false,
                spec: InstrumentSpec::Granular {
                    texture: t,
                    sample: None,
                },
            });
        }
        for p in PRESETS.iter() {
            v.push(Sound {
                id: format!("macro:{}", p.id),
                label: p.label.to_string(),
                drums: false,
                spec: InstrumentSpec::Macro {
                    name: p.label.to_string(),
                    patch: p.patch,
                },
            });
        }
        v
    })
}

/// What a parameter is, for the host.
#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    /// 0..1, shown in percent.
    Amount,
    /// One of the sounds.
    Sound,
    /// Off, or one of the ready-made drum loops.
    Beat,
}

pub struct Param {
    /// Never changes: what a saved state and a host's automation remember.
    pub key: &'static str,
    pub title: &'static str,
    pub kind: Kind,
    pub default: f32,
}

pub const SOUND: usize = 0;
pub const MACRO_0: usize = 1;
pub const BEAT: usize = 8;
pub const SWING: usize = 9;
pub const TAPE: usize = 10;
pub const VINYL: usize = 11;
pub const WARMTH: usize = 12;
pub const SPACE: usize = 13;

const fn amount(key: &'static str, title: &'static str, default: f32) -> Param {
    Param {
        key,
        title,
        kind: Kind::Amount,
        default,
    }
}

/// The parameters, by id. The order is the id: add at the end, never between.
pub const PARAMS: [Param; 14] = [
    Param {
        key: "sound",
        title: "Sound",
        kind: Kind::Sound,
        default: 0.0,
    },
    amount("filter", "Filter", 1.0),
    amount("resonance", "Resonance", 0.1),
    amount("drive", "Drive", 0.0),
    amount("saturation", "Saturation", 0.0),
    amount("reverb", "Reverb", 0.0),
    amount("delay", "Delay", 0.0),
    amount("volume", "Volume", 0.8),
    Param {
        key: "beat",
        title: "Beat",
        kind: Kind::Beat,
        default: 0.0,
    },
    amount("swing", "Swing", 0.0),
    amount("tape", "Tape", 0.0),
    amount("vinyl", "Vinyl", 0.0),
    amount("warmth", "Warmth", 0.0),
    amount("space", "Room size", 0.6),
];

/// How many choices a parameter has (0 for a continuous one).
pub fn choices(param: usize) -> usize {
    match PARAMS[param].kind {
        Kind::Amount => 0,
        Kind::Sound => sounds().len(),
        Kind::Beat => GrooveId::ALL.len() + 1,
    }
}

/// The choice a normalized value stands for.
pub fn choice(param: usize, value: f32) -> usize {
    let n = choices(param);
    if n < 2 {
        return 0;
    }
    ((value.clamp(0.0, 1.0) * (n - 1) as f32).round() as usize).min(n - 1)
}

/// The normalized value of a choice.
pub fn value_of(param: usize, choice: usize) -> f32 {
    let n = choices(param);
    if n < 2 {
        return 0.0;
    }
    choice.min(n - 1) as f32 / (n - 1) as f32
}

/// What a host shows for a value.
pub fn display(param: usize, value: f32) -> String {
    match PARAMS[param].kind {
        Kind::Amount => format!("{}", (value.clamp(0.0, 1.0) * 100.0).round()),
        Kind::Sound => sounds()[choice(param, value)].label.clone(),
        Kind::Beat => match choice(param, value) {
            0 => "Off".to_string(),
            n => GrooveId::ALL[n - 1].label().to_string(),
        },
    }
}

/// The values of every parameter (normalized), and what the host's song is doing.
pub struct Shared {
    values: [AtomicU32; PARAMS.len()],
    /// The host's tempo, as f32 bits.
    bpm: AtomicU32,
    quit: AtomicBool,
}

impl Shared {
    fn new() -> Self {
        Shared {
            values: std::array::from_fn(|i| AtomicU32::new(PARAMS[i].default.to_bits())),
            bpm: AtomicU32::new(120f32.to_bits()),
            quit: AtomicBool::new(false),
        }
    }
    pub fn get(&self, param: usize) -> f32 {
        f32::from_bits(self.values[param].load(Ordering::Relaxed))
    }
    pub fn set(&self, param: usize, value: f32) {
        self.values[param].store(value.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }
}

/// The parameters of an instance: they live as long as it does, whatever the
/// audio does.
pub struct Params(Arc<Shared>);

impl Default for Params {
    fn default() -> Self {
        Params(Arc::new(Shared::new()))
    }
}

impl Params {
    pub fn get(&self, param: usize) -> f32 {
        self.0.get(param)
    }
    pub fn set(&self, param: usize, value: f32) {
        self.0.set(param, value)
    }

    /// Everything a host needs to bring the instance back, as text.
    pub fn save(&self) -> String {
        let mut s = String::from("mjolnir 1\n");
        for (i, p) in PARAMS.iter().enumerate() {
            let v = self.get(i);
            match p.kind {
                Kind::Amount => s.push_str(&format!("{}={v}\n", p.key)),
                Kind::Sound => s.push_str(&format!("{}={}\n", p.key, sounds()[choice(i, v)].id)),
                Kind::Beat => {
                    let id = match choice(i, v) {
                        0 => "off".to_string(),
                        n => format!("{:?}", GrooveId::ALL[n - 1]),
                    };
                    s.push_str(&format!("{}={id}\n", p.key));
                }
            }
        }
        s
    }

    /// Take what `save` wrote. Lines it does not know are skipped, so a state
    /// from a newer version still opens.
    pub fn open(&self, text: &str) -> bool {
        if !text.starts_with("mjolnir ") {
            return false;
        }
        for line in text.lines().skip(1) {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let Some(i) = PARAMS.iter().position(|p| p.key == key) else {
                continue;
            };
            match PARAMS[i].kind {
                Kind::Amount => {
                    if let Ok(v) = value.parse::<f32>() {
                        self.set(i, v);
                    }
                }
                Kind::Sound => {
                    if let Some(n) = sounds().iter().position(|s| s.id == value) {
                        self.set(i, value_of(i, n));
                    }
                }
                Kind::Beat => {
                    let n = GrooveId::ALL
                        .iter()
                        .position(|g| format!("{g:?}") == value)
                        .map_or(0, |n| n + 1);
                    self.set(i, value_of(i, n));
                }
            }
        }
        true
    }
}

/// A note from the host, `at` frames into the block.
#[derive(Clone, Copy)]
pub struct NoteEvent {
    pub at: usize,
    pub pitch: u8,
    /// 0 lets the note go.
    pub velocity: f32,
}

/// The host's song, for one block.
#[derive(Clone, Copy, Default)]
pub struct Transport {
    pub playing: bool,
    pub bpm: Option<f64>,
    /// Where the block starts, in quarter notes.
    pub quarters: Option<f64>,
}

/// The audio side: built for one sample rate, used by the audio thread alone.
pub struct Audio {
    engine: Engine,
    shared: Arc<Shared>,
    helper: Option<JoinHandle<()>>,
    scratch: Vec<f32>,
    playing: bool,
}

const SCRATCH_FRAMES: usize = 1024;

impl Audio {
    pub fn new(params: &Params, sample_rate: f32) -> Audio {
        let (engine, endpoints) = Engine::new(sample_rate);
        let shared = params.0.clone();
        shared.quit.store(false, Ordering::Relaxed);
        let mut ctrl = Controller::new(endpoints, sample_rate);
        let mut project = Project::default();
        project.add_scene("A");
        ctrl.load(project, Default::default());
        let mut helper = Helper {
            ctrl,
            shared: shared.clone(),
            sound: usize::MAX,
            beat: usize::MAX,
            macros: [f32::NAN; 7],
            master: [f32::NAN; 4],
            swing: f32::NAN,
            bpm: f32::NAN,
        };
        // the first sound is ready before the first block
        helper.follow();
        let handle = std::thread::Builder::new()
            .name("mjolnir".into())
            .spawn(move || {
                while !helper.shared.quit.load(Ordering::Relaxed) {
                    helper.follow();
                    std::thread::sleep(Duration::from_millis(4));
                }
            })
            .ok();
        Audio {
            engine,
            shared,
            helper: handle,
            scratch: vec![0.0; SCRATCH_FRAMES * 2],
            playing: false,
        }
    }

    /// Render one block. `events` are in order of `at`.
    pub fn process(
        &mut self,
        transport: Transport,
        events: &[NoteEvent],
        left: &mut [f32],
        right: &mut [f32],
    ) {
        if let Some(bpm) = transport.bpm {
            self.shared
                .bpm
                .store((bpm as f32).to_bits(), Ordering::Relaxed);
        }
        if transport.playing != self.playing {
            self.playing = transport.playing;
            if transport.playing {
                self.engine.handle_now(Command::Play);
                if let Some(q) = transport.quarters {
                    self.engine.follow_host(q * PPQ as f64);
                }
            } else {
                self.engine.handle_now(Command::Stop);
            }
        }
        let frames = left.len().min(right.len());
        let mut done = 0;
        let mut next = 0;
        while done < frames {
            while next < events.len() && events[next].at <= done {
                let e = events[next];
                next += 1;
                self.engine.handle_now(if e.velocity > 0.0 {
                    Command::NoteOn {
                        track: 0,
                        pitch: e.pitch,
                        velocity: e.velocity,
                    }
                } else {
                    Command::NoteOff {
                        track: 0,
                        pitch: e.pitch,
                    }
                });
            }
            let until = events.get(next).map_or(frames, |e| e.at.min(frames));
            let n = (until - done).min(SCRATCH_FRAMES);
            let out = &mut self.scratch[..n * 2];
            self.engine.process(out, 2);
            for (i, frame) in out.chunks_exact(2).enumerate() {
                left[done + i] = frame[0];
                right[done + i] = frame[1];
            }
            done += n;
        }
        // notes that fall on the block's last edge
        for e in &events[next..] {
            self.engine.handle_now(if e.velocity > 0.0 {
                Command::NoteOn {
                    track: 0,
                    pitch: e.pitch,
                    velocity: e.velocity,
                }
            } else {
                Command::NoteOff {
                    track: 0,
                    pitch: e.pitch,
                }
            });
        }
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        self.shared.quit.store(true, Ordering::Relaxed);
        if let Some(h) = self.helper.take() {
            let _ = h.join();
        }
    }
}

/// The helper thread's side: makes the track what the parameters say.
struct Helper {
    ctrl: Controller,
    shared: Arc<Shared>,
    sound: usize,
    beat: usize,
    macros: [f32; 7],
    master: [f32; 4],
    swing: f32,
    bpm: f32,
}

impl Helper {
    fn follow(&mut self) {
        let s = &self.shared;
        let sound = choice(SOUND, s.get(SOUND));
        let mut fresh = false;
        if sound != self.sound {
            let now = &sounds()[sound];
            let was_drums = sounds().get(self.sound).map(|s| s.drums);
            if was_drums == Some(now.drums) {
                let _ = self.ctrl.swap_instrument(0, now.spec.clone());
            } else {
                if was_drums.is_some() {
                    let _ = self.ctrl.remove_track(0);
                }
                let kind = if now.drums {
                    TrackKind::Drums
                } else {
                    TrackKind::Melodic
                };
                let _ = self.ctrl.add_track(kind, now.spec.clone());
                fresh = true;
            }
            self.sound = sound;
        }
        if fresh {
            // a new track starts from its defaults
            self.macros = [f32::NAN; 7];
            self.beat = usize::MAX;
        }

        for (i, param) in MacroParam::ALL.iter().enumerate() {
            let v = self.shared.get(MACRO_0 + i);
            if v != self.macros[i] {
                self.macros[i] = v;
                let _ = self.ctrl.set_macro(0, *param, v);
            }
        }

        let master = [TAPE, VINYL, WARMTH, SPACE].map(|p| self.shared.get(p));
        if master != self.master {
            self.master = master;
            self.ctrl.set_master(Master {
                volume: 0.8,
                tape: master[0],
                vinyl: master[1],
                warmth: master[2],
                space: master[3],
                pump: 0.0,
            });
        }

        let swing = self.shared.get(SWING);
        if swing != self.swing {
            self.swing = swing;
            self.ctrl.set_swing(swing);
        }
        let bpm = f32::from_bits(self.shared.bpm.load(Ordering::Relaxed));
        if bpm != self.bpm && bpm.is_finite() && bpm > 0.0 {
            self.bpm = bpm;
            self.ctrl.set_bpm(bpm);
        }

        let beat = choice(BEAT, self.shared.get(BEAT));
        if beat != self.beat {
            self.beat = beat;
            let _ = self.ctrl.launch(0, None, true);
            let _ = self.ctrl.delete_pattern(0, 0);
            if beat > 0 && sounds()[self.sound].drums {
                let groove = GrooveId::ALL[beat - 1];
                if self.ctrl.load_groove(0, 0, groove).is_ok() {
                    let _ = self.ctrl.launch(0, Some(0), true);
                }
            }
        }

        self.ctrl.poll();
    }
}
