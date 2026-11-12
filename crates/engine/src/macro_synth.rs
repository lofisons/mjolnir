//! The macro synth: ten ways of making a tone, each steered by the same three
//! knobs (harmonics, timbre, morph), an envelope, an ensemble chorus and a
//! little drift. Every sound made with it is open: all of it can be edited.
//!
//! The engines are small physical or spectral models written for slow, dark
//! music: a string machine, a wavetable, drawbar harmonics, two-operator FM,
//! a plucked and a bowed string, struck bars and bells, a vowel, a swarm of
//! saws and the wind.

use std::f32::consts::TAU;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::dsp::{midi_to_hz, sin_turns, sine_table, Adsr, Noise, Svf};
use crate::sound::ParamSpec;

pub const ENGINES: [&str; 10] = [
    "Strings",
    "Wavetable",
    "Harmonic",
    "FM",
    "Pluck",
    "Bowed",
    "Modal",
    "Vowel",
    "Swarm",
    "Wind",
];

const VOICES: usize = 8;
const MAX_LINE: usize = 4096;

/// Everything that defines a macro sound. Every field is a knob.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MacroPatch {
    /// Which engine (index into `ENGINES`).
    pub engine: f32,
    pub harmonics: f32,
    pub timbre: f32,
    pub morph: f32,
    /// Seconds.
    pub attack: f32,
    pub decay: f32,
    /// 0..1
    pub sustain: f32,
    pub release: f32,
    /// Ensemble chorus, 0..1.
    pub chorus: f32,
    /// Slow pitch wander of every voice, 0..1.
    pub drift: f32,
    pub level: f32,
}

impl Default for MacroPatch {
    fn default() -> Self {
        MacroPatch {
            engine: 0.0,
            harmonics: 0.5,
            timbre: 0.5,
            morph: 0.5,
            attack: 0.01,
            decay: 0.6,
            sustain: 0.8,
            release: 0.5,
            chorus: 0.3,
            drift: 0.3,
            level: 0.8,
        }
    }
}

const fn knob(
    key: &'static str,
    label: &'static str,
    group: &'static str,
    min: f32,
    max: f32,
    log: bool,
    unit: &'static str,
    help: &'static str,
) -> ParamSpec {
    ParamSpec {
        key,
        label,
        group,
        min,
        max,
        log,
        step: 0.0,
        unit,
        choices: &[],
        help,
    }
}

/// The macro synth's knobs, in the order the editor shows them.
pub const MACRO_PARAMS: [ParamSpec; 11] = [
    ParamSpec {
        key: "engine",
        label: "Engine",
        group: "Tone",
        min: 0.0,
        max: (ENGINES.len() - 1) as f32,
        log: false,
        step: 1.0,
        unit: "",
        choices: &ENGINES,
        help: "the way the tone is made",
    },
    knob(
        "harmonics",
        "Harmonics",
        "Tone",
        0.0,
        1.0,
        false,
        "",
        "the engine's first colour: which overtones, which interval, how long a string rings",
    ),
    knob(
        "timbre",
        "Timbre",
        "Tone",
        0.0,
        1.0,
        false,
        "",
        "the engine's second colour: mostly brightness",
    ),
    knob(
        "morph",
        "Morph",
        "Tone",
        0.0,
        1.0,
        false,
        "",
        "the engine's third colour: shape, hardness, breath",
    ),
    knob(
        "attack",
        "Attack",
        "Envelope",
        0.001,
        4.0,
        true,
        "s",
        "time to reach full level",
    ),
    knob(
        "decay",
        "Decay",
        "Envelope",
        0.02,
        8.0,
        true,
        "s",
        "time to fall to the sustain level; how long a struck or plucked sound rings",
    ),
    knob(
        "sustain",
        "Sustain",
        "Envelope",
        0.0,
        1.0,
        false,
        "",
        "level while the key is held",
    ),
    knob(
        "release",
        "Release",
        "Envelope",
        0.01,
        8.0,
        true,
        "s",
        "time to fade after the key is let go",
    ),
    knob(
        "chorus",
        "Ensemble",
        "Space",
        0.0,
        1.0,
        false,
        "",
        "three slow delays that make one voice sound like a section",
    ),
    knob(
        "drift",
        "Drift",
        "Space",
        0.0,
        1.0,
        false,
        "",
        "each voice wanders a little in pitch, like tape",
    ),
    knob(
        "level",
        "Level",
        "Space",
        0.0,
        1.5,
        false,
        "",
        "output level",
    ),
];

impl MacroPatch {
    pub fn get(&self, key: &str) -> Option<f32> {
        Some(match key {
            "engine" => self.engine,
            "harmonics" => self.harmonics,
            "timbre" => self.timbre,
            "morph" => self.morph,
            "attack" => self.attack,
            "decay" => self.decay,
            "sustain" => self.sustain,
            "release" => self.release,
            "chorus" => self.chorus,
            "drift" => self.drift,
            "level" => self.level,
            _ => return None,
        })
    }

    pub fn set(&mut self, key: &str, value: f32) -> Result<(), String> {
        let spec = MACRO_PARAMS
            .iter()
            .find(|p| p.key == key)
            .ok_or_else(|| format!("no such knob: {key}"))?;
        let v = spec.clamp(value);
        match key {
            "engine" => self.engine = v,
            "harmonics" => self.harmonics = v,
            "timbre" => self.timbre = v,
            "morph" => self.morph = v,
            "attack" => self.attack = v,
            "decay" => self.decay = v,
            "sustain" => self.sustain = v,
            "release" => self.release = v,
            "chorus" => self.chorus = v,
            "drift" => self.drift = v,
            _ => self.level = v,
        }
        Ok(())
    }

    /// Every knob inside its range.
    pub fn clamped(mut self) -> Self {
        for p in MACRO_PARAMS.iter() {
            if let Some(v) = self.get(p.key) {
                let _ = self.set(p.key, v);
            }
        }
        self
    }
}

/// A ready-made macro sound: its id, name, kind, home octave and patch.
pub struct MacroPreset {
    pub id: &'static str,
    pub label: &'static str,
    pub category: &'static str,
    pub octave: i32,
    pub patch: MacroPatch,
}

#[allow(clippy::too_many_arguments)]
const fn patch(
    engine: usize,
    harmonics: f32,
    timbre: f32,
    morph: f32,
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
    chorus: f32,
    drift: f32,
    level: f32,
) -> MacroPatch {
    MacroPatch {
        engine: engine as f32,
        harmonics,
        timbre,
        morph,
        attack,
        decay,
        sustain,
        release,
        chorus,
        drift,
        level,
    }
}

const fn preset(
    id: &'static str,
    label: &'static str,
    category: &'static str,
    octave: i32,
    patch: MacroPatch,
) -> MacroPreset {
    MacroPreset {
        id,
        label,
        category,
        octave,
        patch,
    }
}

/// The sounds the browser offers, written for slow, dark, dusty music.
pub const PRESETS: [MacroPreset; 24] = [
    // ---- pads
    preset(
        "solina",
        "Night strings",
        "pads",
        3,
        patch(0, 0.35, 0.42, 0.15, 0.45, 1.5, 0.9, 1.6, 0.85, 0.4, 0.8),
    ),
    preset(
        "hollow",
        "Hollow strings",
        "pads",
        3,
        patch(0, 0.1, 0.3, 0.85, 0.7, 2.0, 0.85, 2.2, 0.7, 0.5, 0.8),
    ),
    preset(
        "choir",
        "Fog choir",
        "pads",
        3,
        patch(7, 0.72, 0.42, 0.25, 0.6, 1.5, 0.9, 1.8, 0.6, 0.35, 0.8),
    ),
    preset(
        "glass_organ",
        "Glass organ",
        "pads",
        3,
        patch(2, 0.18, 0.3, 0.6, 0.25, 1.0, 0.9, 1.4, 0.45, 0.3, 0.8),
    ),
    preset(
        "swarm",
        "Swarm",
        "pads",
        3,
        patch(8, 0.55, 0.38, 0.2, 0.8, 2.0, 0.9, 2.4, 0.4, 0.5, 0.75),
    ),
    preset(
        "bowed_glass",
        "Bowed glass",
        "pads",
        4,
        patch(5, 0.8, 0.35, 0.3, 0.5, 1.0, 1.0, 1.5, 0.35, 0.4, 0.9),
    ),
    preset(
        "wind",
        "Wind",
        "pads",
        4,
        patch(9, 0.35, 0.45, 0.7, 1.2, 2.0, 1.0, 2.5, 0.2, 0.3, 1.3),
    ),
    preset(
        "drift_table",
        "Slow table",
        "pads",
        3,
        patch(1, 0.3, 0.15, 0.6, 0.9, 2.0, 0.9, 2.0, 0.6, 0.6, 0.8),
    ),
    // ---- keys
    preset(
        "tine",
        "Tine piano",
        "keys",
        4,
        patch(3, 0.12, 0.42, 0.05, 0.003, 2.2, 0.0, 0.6, 0.25, 0.25, 0.85),
    ),
    preset(
        "dark_bell",
        "Dark bell",
        "keys",
        4,
        patch(6, 0.4, 0.35, 0.3, 0.001, 4.0, 0.0, 2.5, 0.15, 0.2, 0.9),
    ),
    preset(
        "bowl",
        "Singing bowl",
        "keys",
        4,
        patch(6, 0.72, 0.6, 0.1, 0.001, 6.0, 0.0, 4.0, 0.2, 0.15, 0.9),
    ),
    preset(
        "kalimba",
        "Kalimba",
        "keys",
        4,
        patch(6, 0.05, 0.2, 0.55, 0.001, 1.1, 0.0, 0.8, 0.1, 0.2, 0.9),
    ),
    preset(
        "fm_bell",
        "Cold bell",
        "keys",
        5,
        patch(3, 0.62, 0.5, 0.0, 0.002, 3.5, 0.0, 2.0, 0.3, 0.2, 0.7),
    ),
    preset(
        "nylon",
        "Nylon",
        "keys",
        4,
        patch(4, 0.62, 0.4, 0.45, 0.001, 3.0, 1.0, 0.5, 0.1, 0.2, 1.0),
    ),
    preset(
        "dust_keys",
        "Dust keys",
        "keys",
        4,
        patch(1, 0.12, 0.05, 0.25, 0.004, 1.6, 0.25, 0.5, 0.35, 0.5, 0.85),
    ),
    // ---- leads
    preset(
        "whistle",
        "Whistle",
        "leads",
        5,
        patch(9, 0.92, 0.1, 0.25, 0.06, 0.5, 1.0, 0.35, 0.15, 0.3, 0.7),
    ),
    preset(
        "sine_lead",
        "Lost signal",
        "leads",
        5,
        patch(2, 0.0, 0.05, 0.3, 0.03, 0.6, 0.9, 0.5, 0.3, 0.6, 0.55),
    ),
    preset(
        "vox_lead",
        "Voice",
        "leads",
        4,
        patch(7, 0.1, 0.5, 0.1, 0.05, 0.6, 0.9, 0.4, 0.3, 0.4, 0.75),
    ),
    preset(
        "reed",
        "Reed",
        "leads",
        4,
        patch(1, 0.55, 0.2, 0.1, 0.03, 0.5, 0.9, 0.3, 0.2, 0.35, 0.65),
    ),
    preset(
        "saw_lead",
        "Thin saw",
        "leads",
        4,
        patch(8, 0.12, 0.5, 0.0, 0.01, 0.5, 0.85, 0.3, 0.2, 0.3, 0.7),
    ),
    // ---- bass
    preset(
        "fm_bass",
        "Round FM bass",
        "bass",
        2,
        patch(3, 0.12, 0.3, 0.2, 0.003, 0.5, 0.6, 0.2, 0.0, 0.15, 0.9),
    ),
    preset(
        "pluck_bass",
        "String bass",
        "bass",
        2,
        patch(4, 0.5, 0.3, 0.6, 0.001, 2.0, 1.0, 0.25, 0.0, 0.1, 1.1),
    ),
    preset(
        "reese",
        "Low swarm",
        "bass",
        2,
        patch(8, 0.3, 0.22, 0.0, 0.01, 0.6, 0.9, 0.25, 0.1, 0.3, 0.85),
    ),
    preset(
        "sub_organ",
        "Sub organ",
        "bass",
        2,
        patch(2, 0.0, 0.25, 0.0, 0.005, 0.5, 0.95, 0.2, 0.0, 0.1, 1.0),
    ),
];

pub fn preset_by_id(id: &str) -> Option<&'static MacroPreset> {
    PRESETS.iter().find(|p| p.id == id)
}

// ------------------------------------------------------------------ tables

const TABLE: usize = 2048;
const WAVES: usize = 8;
/// Most overtones a table keeps, per band-limited copy (low, mid, high notes).
const MIPS: [usize; 3] = [64, 20, 7];

/// Eight single cycles, from a sine to glass, each in three band-limited copies.
fn tables() -> &'static Vec<f32> {
    static CELL: OnceLock<Vec<f32>> = OnceLock::new();
    CELL.get_or_init(|| {
        let amp = |wave: usize, k: usize| -> f32 {
            let kf = k as f32;
            match wave {
                0 => (k == 1) as u8 as f32,
                // triangle: odd overtones falling fast
                1 => {
                    if k % 2 == 1 {
                        1.0 / (kf * kf)
                    } else {
                        0.0
                    }
                }
                // a saw with its top rolled off
                2 => 1.0 / kf / (1.0 + (kf / 9.0) * (kf / 9.0)),
                // square: odd overtones
                3 => {
                    if k % 2 == 1 {
                        1.0 / kf
                    } else {
                        0.0
                    }
                }
                // reed: a hollow body with a bump around the sixth overtone
                4 => {
                    (1.0 / kf)
                        * (0.4 + 1.6 * (-((kf - 6.0) / 2.5).powi(2)).exp())
                        * if k % 2 == 1 { 1.0 } else { 0.45 }
                }
                // nasal: energy pushed up to the ninth
                5 => {
                    (1.0 / kf.sqrt()) * (-((kf - 9.0) / 4.0).powi(2)).exp()
                        + if k == 1 { 0.7 } else { 0.0 }
                }
                // the whole saw
                6 => 1.0 / kf,
                // glass: every third overtone
                _ => {
                    if k % 3 == 1 {
                        1.0 / kf.powf(0.8)
                    } else {
                        0.0
                    }
                }
            }
        };
        let mut out = vec![0.0f32; WAVES * MIPS.len() * TABLE];
        for wave in 0..WAVES {
            for (m, &top) in MIPS.iter().enumerate() {
                let slice = &mut out[(wave * MIPS.len() + m) * TABLE..][..TABLE];
                for k in 1..=top {
                    let a = amp(wave, k);
                    if a == 0.0 {
                        continue;
                    }
                    for (i, s) in slice.iter_mut().enumerate() {
                        *s += a * (TAU * k as f32 * i as f32 / TABLE as f32).sin();
                    }
                }
                let peak = slice.iter().fold(0.0f32, |p, s| p.max(s.abs())).max(1e-6);
                slice.iter_mut().for_each(|s| *s /= peak);
            }
        }
        out
    })
}

/// PolyBLEP residual: rounds the step of a saw so it does not alias.
#[inline]
fn blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

#[inline]
fn saw(phase: &mut f32, dt: f32) -> f32 {
    *phase += dt;
    if *phase >= 1.0 {
        *phase -= 1.0;
    }
    2.0 * *phase - 1.0 - blep(*phase, dt)
}

// modal ratio sets: a free bar, a bell, a bowl, a string
const MODES: [[f32; 6]; 4] = [
    [1.0, 2.756, 5.404, 8.933, 13.34, 18.64],
    // a bell: its note first, then the hum an octave under it
    [1.0, 0.5, 1.19, 1.5, 2.0, 2.51],
    [1.0, 2.32, 4.25, 6.63, 9.38, 12.22],
    [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
];
// vowel formants (Hz): a e i o u
const FORMANTS: [[f32; 3]; 5] = [
    [730.0, 1090.0, 2440.0],
    [530.0, 1840.0, 2480.0],
    [270.0, 2290.0, 3010.0],
    [570.0, 840.0, 2410.0],
    [300.0, 870.0, 2240.0],
];
const FM_RATIOS: [f32; 10] = [0.5, 1.0, 2.0, 3.0, 3.5, 4.0, 5.0, 7.0, 9.0, 14.0];
const SWARM: [f32; 7] = [-1.0, -0.58, -0.21, 0.0, 0.2, 0.61, 1.0];

struct Voice {
    pitch: u8,
    vel: f32,
    freq: f32,
    env: Adsr,
    age: u64,
    held: bool,
    ph: [f32; 8],
    lp: f32,
    lp2: f32,
    fb: f32,
    /// Excitation that dies away after the note starts (FM index, strike).
    exc: f32,
    exc_coef: f32,
    /// Samples of excitation still to feed a string.
    burst: u32,
    line: Vec<f32>,
    wpos: usize,
    modes: [[f32; 2]; 6],
    formants: [Svf; 3],
    band: Svf,
    noise: Noise,
    drift_phase: f32,
    drift_rate: f32,
    gust: f32,
}

impl Voice {
    fn new(seed: u32, sr: f32) -> Self {
        let mut noise = Noise::new(seed);
        Voice {
            pitch: 0,
            vel: 0.0,
            freq: 110.0,
            env: Adsr::new(0.01, 0.5, 0.8, 0.5, sr),
            age: 0,
            held: false,
            ph: [0.0; 8],
            lp: 0.0,
            lp2: 0.0,
            fb: 0.0,
            exc: 0.0,
            exc_coef: 0.999,
            burst: 0,
            line: vec![0.0; MAX_LINE],
            wpos: 0,
            modes: [[0.0; 2]; 6],
            formants: [Svf::default(), Svf::default(), Svf::default()],
            band: Svf::default(),
            drift_phase: noise.next().abs(),
            drift_rate: 0.11 + 0.17 * noise.next().abs(),
            noise,
            gust: 0.5,
        }
    }

    fn start(&mut self, pitch: u8, vel: f32, p: &MacroPatch, sr: f32, age: u64) {
        self.pitch = pitch;
        self.vel = vel;
        self.freq = midi_to_hz(pitch as f32);
        self.age = age;
        self.held = true;
        self.env.set(p.attack, p.decay, p.sustain, p.release, sr);
        self.env.gate_on();
        self.exc = 1.0;
        self.fb = 0.0;
        let engine = p.engine.round() as usize;
        match engine {
            // strings and struck bars start from rest
            4 | 5 => {
                self.line.iter_mut().for_each(|s| *s = 0.0);
                self.lp = 0.0;
                self.lp2 = 0.0;
                self.burst = (sr / self.freq).ceil() as u32;
            }
            6 => {
                self.modes = [[0.0; 2]; 6];
                self.burst = (0.003 * sr) as u32;
            }
            3 => self.exc_coef = (-1.0 / (0.05 + p.decay * 0.35).max(0.02) / sr).exp(),
            _ => {}
        }
        // free-running oscillators start at scattered phases so stacked notes do not flange
        if !matches!(engine, 3 | 4 | 5 | 6) {
            for ph in self.ph.iter_mut() {
                *ph = self.noise.next().abs().fract();
            }
        } else {
            self.ph = [0.0; 8];
        }
    }

    /// Add this voice to `out`.
    fn render(&mut self, p: &MacroPatch, out: &mut [f32], sr: f32) {
        // slow wander, once per block
        self.drift_phase = (self.drift_phase + self.drift_rate * out.len() as f32 / sr).fract();
        let cents = (TAU * self.drift_phase).sin() * p.drift * 9.0;
        let f = (self.freq * 2f32.powf(cents / 1200.0)).min(sr * 0.45);
        let dt = f / sr;
        let (h, t, m) = (p.harmonics, p.timbre, p.morph);
        let gain = self.vel * p.level;
        let sine = sine_table();
        match p.engine.round() as usize {
            // ---- string machine: three saws a hair apart and one an octave
            // up, morphing to a hollow pulse
            0 => {
                let a = 1.0 - (-TAU * (250.0 * 2f32.powf(t * 5.6)).min(sr * 0.4) / sr).exp();
                for o in out.iter_mut() {
                    let s0 = saw(&mut self.ph[0], dt);
                    let s1 = saw(&mut self.ph[1], dt * 1.0041);
                    let s2 = saw(&mut self.ph[2], dt * 0.9958);
                    let up = saw(&mut self.ph[3], (dt * 2.0).min(0.45));
                    // a pulse is a saw minus the same saw half a cycle later
                    let mut half = self.ph[0] + 0.5;
                    if half >= 1.0 {
                        half -= 1.0;
                    }
                    let pulse = s0 - (2.0 * half - 1.0 - blep(half, dt));
                    let body = (s0 + s1 + s2) * (1.0 - m) * 0.33 + pulse * m * 0.6;
                    let x = body * (1.0 - 0.4 * h) + up * h * 0.5;
                    self.lp += a * (x - self.lp);
                    *o += self.lp * self.env.next() * gain * 0.75;
                }
            }
            // ---- wavetable: a position among eight cycles, a fold, a
            // detuned double with a sub
            1 => {
                let mip = if f < 170.0 {
                    0
                } else if f < 520.0 {
                    1
                } else {
                    2
                };
                let pos = h * (WAVES - 1) as f32;
                let w = (pos as usize).min(WAVES - 2);
                let mix = pos - w as f32;
                let fold = 1.0 + t * 2.4;
                let all = tables();
                let lo = &all[(w * MIPS.len() + mip) * TABLE..][..TABLE];
                let hi = &all[((w + 1) * MIPS.len() + mip) * TABLE..][..TABLE];
                let read = |ph: f32| {
                    let x = ph * TABLE as f32;
                    let i = x as usize & (TABLE - 1);
                    let j = (i + 1) & (TABLE - 1);
                    let f = x - x.floor();
                    let a = lo[i] + (lo[j] - lo[i]) * f;
                    let b = hi[i] + (hi[j] - hi[i]) * f;
                    a + (b - a) * mix
                };
                for o in out.iter_mut() {
                    self.ph[0] = (self.ph[0] + dt).fract();
                    self.ph[1] = (self.ph[1] + dt * 1.0052).fract();
                    self.ph[2] = (self.ph[2] + dt * 0.5).fract();
                    let mut x = read(self.ph[0]);
                    if t > 0.01 {
                        x = sin_turns(sine, x * fold * 0.25);
                    }
                    let double = read(self.ph[1]);
                    let sub = sin_turns(sine, self.ph[2]);
                    let y = x * (1.0 - 0.35 * m) + double * m * 0.45 + sub * m * 0.4;
                    *o += y * self.env.next() * gain * 0.5;
                }
            }
            // ---- harmonic: eight drawbars, a bump that moves up the series
            2 => {
                let centre = h * 6.0;
                let width = 0.55 + t * 3.0;
                let mut amps = [0.0f32; 8];
                let mut sum = 0.0;
                for (k, a) in amps.iter_mut().enumerate() {
                    if (k + 1) as f32 * f > sr * 0.45 {
                        break;
                    }
                    let d = (k as f32 - centre) / width;
                    let even = if k % 2 == 1 { 1.0 - m * 0.9 } else { 1.0 };
                    *a = (-d * d).exp() * even;
                    sum += *a;
                }
                let norm = 1.0 / sum.max(0.3);
                for o in out.iter_mut() {
                    self.ph[0] = (self.ph[0] + dt).fract();
                    let mut x = 0.0;
                    for (k, a) in amps.iter().enumerate() {
                        if *a > 0.001 {
                            x += a * sin_turns(sine, (k + 1) as f32 * self.ph[0]);
                        }
                    }
                    *o += x * norm * self.env.next() * gain * 0.6;
                }
            }
            // ---- FM: one sine bending another; the bend dies away after the strike
            3 => {
                let ratio = FM_RATIOS[((h * 9.99) as usize).min(9)];
                let index = t * t * 7.0;
                let fb = m * 1.1;
                let dm = (dt * ratio).min(0.45);
                for o in out.iter_mut() {
                    self.exc *= self.exc_coef;
                    self.ph[0] = (self.ph[0] + dt).fract();
                    self.ph[1] = (self.ph[1] + dm).fract();
                    let modulator = sin_turns(sine, self.ph[1] + fb * self.fb / TAU);
                    self.fb = modulator;
                    let depth = index * (0.25 + 0.75 * self.exc);
                    let x = sin_turns(sine, self.ph[0] + depth * modulator / TAU);
                    *o += x * self.env.next() * gain * 0.6;
                }
            }
            // ---- a string: plucked (4) or bowed (5)
            4 | 5 => {
                let bowed = p.engine.round() as usize == 5;
                // seconds for the string to die away, and how dark it rings
                let ring = if bowed {
                    0.6 + h * 5.0
                } else {
                    0.25 + h * h * 9.0
                };
                let dark = if bowed {
                    0.25 + 0.7 * t
                } else {
                    1.0 - 0.75 * m
                };
                let a = dark.clamp(0.05, 1.0);
                // the loop filter delays by (1 - a) / a samples: take them off the line
                let len = (sr / f - (1.0 - a) / a).clamp(2.0, (MAX_LINE - 2) as f32);
                let mut g = 10f32.powf(-3.0 / (ring * f));
                if !self.held {
                    g *= 0.985;
                }
                let pick = 0.08 + 0.9 * t;
                let vib_depth = if bowed { m * 0.012 } else { 0.0 };
                for o in out.iter_mut() {
                    let e = self.env.next();
                    let mut length = len;
                    if vib_depth > 0.0 {
                        self.ph[7] = (self.ph[7] + 5.3 / sr).fract();
                        length *= 1.0 + vib_depth * sin_turns(sine, self.ph[7]) * e.min(1.0);
                    }
                    let read = self.wpos as f32 + MAX_LINE as f32 - length;
                    let i = read as usize;
                    let frac = read - i as f32;
                    let y0 = self.line[i % MAX_LINE];
                    let y1 = self.line[(i + 1) % MAX_LINE];
                    let y = y0 + (y1 - y0) * frac;
                    self.lp += a * (y - self.lp);
                    let mut input = 0.0;
                    if bowed {
                        // the bow: breath-coloured noise for as long as the key is down
                        self.lp2 += (0.02 + 0.3 * t) * (self.noise.next() - self.lp2);
                        input = self.lp2 * e * (1.0 - g) * 14.0;
                    } else if self.burst > 0 {
                        self.burst -= 1;
                        self.lp2 += pick * (self.noise.next() - self.lp2);
                        input = self.lp2 * 0.9;
                    }
                    self.line[self.wpos] = self.lp * g + input;
                    self.wpos = (self.wpos + 1) % MAX_LINE;
                    // a plucked string is shaped by the envelope; a bowed one by the bow
                    let shape = if bowed { 1.0 } else { e };
                    *o += y * shape * gain * if bowed { 3.0 } else { 0.8 };
                }
            }
            // ---- modal: six partials struck together; bar, bell, bowl or string
            6 => {
                let pos = h * 3.0;
                let s = (pos as usize).min(2);
                let mix = pos - s as f32;
                let ring = p.decay.max(0.05);
                let mut coef = [(0.0f32, 0.0f32, 0.0f32, 0.0f32); 6];
                for (k, c) in coef.iter_mut().enumerate() {
                    let ratio = MODES[s][k] + (MODES[s + 1][k] - MODES[s][k]) * mix;
                    let fk = f * ratio;
                    if fk > sr * 0.45 {
                        continue;
                    }
                    // upper partials die sooner; timbre lets them ring
                    let t60 = ring / (1.0 + k as f32 * (1.6 - 1.4 * t))
                        * if self.held { 1.0 } else { 0.25 };
                    let r = 10f32.powf(-3.0 / (t60.max(0.01) * sr));
                    let w = TAU * fk / sr;
                    let level = 1.0 / (1.0 + k as f32).powf(1.1 - 0.8 * m);
                    *c = (2.0 * r * w.cos(), -r * r, w.sin(), level);
                }
                for o in out.iter_mut() {
                    let mut x = 0.0;
                    if self.burst > 0 {
                        // the strike: a click, with a puff of noise for a hard mallet
                        let first = self.burst == (0.003 * sr) as u32;
                        self.burst -= 1;
                        x = if first { 1.0 } else { 0.0 } + self.noise.next() * m * 0.25;
                    }
                    let mut y = 0.0;
                    for (k, &(a1, a2, b, level)) in coef.iter().enumerate() {
                        if b == 0.0 {
                            continue;
                        }
                        let v = x * b + a1 * self.modes[k][0] + a2 * self.modes[k][1];
                        self.modes[k][1] = self.modes[k][0];
                        self.modes[k][0] = v;
                        y += v * level;
                    }
                    // the envelope only shapes the release: the bars ring on their own
                    let e = self.env.next();
                    let tail = if self.held { 1.0 } else { e.min(1.0) };
                    *o += y * tail * gain * 0.3;
                }
            }
            // ---- vowel: a buzz through three formants, from a to u
            7 => {
                let pos = h * 4.0;
                let v = (pos as usize).min(3);
                let mix = pos - v as f32;
                let size = 0.72 + t * 0.7;
                for k in 0..3 {
                    let fk = (FORMANTS[v][k] + (FORMANTS[v + 1][k] - FORMANTS[v][k]) * mix) * size;
                    self.formants[k].set(fk.min(sr * 0.4), 9.0, sr);
                }
                for o in out.iter_mut() {
                    self.ph[7] = (self.ph[7] + 5.1 / sr).fract();
                    let vib = 1.0 + 0.004 * sin_turns(sine, self.ph[7]);
                    let buzz = saw(&mut self.ph[0], dt * vib);
                    let x = buzz * (1.0 - 0.5 * m) + self.noise.next() * m * 0.5;
                    let y = self.formants[0].process(x).band
                        + self.formants[1].process(x).band * 0.6
                        + self.formants[2].process(x).band * 0.35;
                    *o += y * self.env.next() * gain * 0.42;
                }
            }
            // ---- swarm: seven saws spread around the note
            8 => {
                let spread = h * h * 0.055;
                let a = 1.0 - (-TAU * (220.0 * 2f32.powf(t * 5.8)).min(sr * 0.4) / sr).exp();
                for o in out.iter_mut() {
                    let mut x = 0.0;
                    for (k, d) in SWARM.iter().enumerate() {
                        x += saw(&mut self.ph[k], dt * (1.0 + d * spread));
                    }
                    let up = saw(&mut self.ph[7], (dt * 2.0).min(0.45));
                    let y = x * 0.38 + up * m * 0.4;
                    self.lp += a * (y - self.lp);
                    *o += self.lp * self.env.next() * gain * 0.5;
                }
            }
            // ---- wind: noise through a band that follows the key
            _ => {
                let q = 1.5 + h * h * 60.0;
                self.band.set(f.min(sr * 0.4), q, sr);
                let air = 1.0 - (-TAU * (300.0 + t * 5000.0) / sr).exp();
                for o in out.iter_mut() {
                    let n = self.noise.next();
                    // gusts: the level wanders
                    self.gust += 0.00004 * (self.noise.next() * 2.0 - (self.gust - 0.5) * 4.0);
                    let gust = 1.0 - m * (1.0 - self.gust.clamp(0.0, 1.0));
                    self.lp += air * (n - self.lp);
                    let tone = self.band.process(n).band * 2.2 / (1.0 + q).sqrt();
                    let y = tone * (1.0 - 0.6 * t) + self.lp * t * 1.2;
                    *o += y * gust * self.env.next() * gain * 0.9;
                }
            }
        }
    }
}

/// Three slow, out-of-step delays: one voice becomes a section.
struct Ensemble {
    line: Vec<f32>,
    pos: usize,
    slow: crate::dsp::Turn,
    fast: crate::dsp::Turn,
    ready: bool,
}

impl Ensemble {
    fn new() -> Self {
        Ensemble {
            line: vec![0.0; 4096],
            pos: 0,
            slow: crate::dsp::Turn::new(0.62, 48000.0),
            fast: crate::dsp::Turn::new(5.7, 48000.0),
            ready: false,
        }
    }

    #[inline]
    fn process(&mut self, x: f32, amount: f32, sr: f32) -> f32 {
        if !self.ready {
            self.slow = crate::dsp::Turn::new(0.62, sr);
            self.fast = crate::dsp::Turn::new(5.7, sr);
            self.ready = true;
        }
        let mask = self.line.len() - 1;
        self.line[self.pos] = x;
        self.slow.next();
        self.fast.next();
        if amount <= 0.001 {
            self.pos = (self.pos + 1) & mask;
            return x;
        }
        // three taps a third of a turn apart: sin(a + 120°) from the sine and cosine
        const C: f32 = -0.5;
        const S: f32 = 0.866_025_4;
        let (s1, c1, s2, c2) = (self.slow.sin, self.slow.cos, self.fast.sin, self.fast.cos);
        let slow = [s1, s1 * C + c1 * S, s1 * C - c1 * S];
        let fast = [s2, s2 * C + c2 * S, s2 * C - c2 * S];
        let mut wet = 0.0;
        for k in 0..3 {
            let d = (13.0 + 4.5 * slow[k] + 0.55 * fast[k]) * 0.001 * sr;
            let i = d as usize;
            let frac = d - i as f32;
            let a = self.line[self.pos.wrapping_sub(i) & mask];
            let b = self.line[self.pos.wrapping_sub(i + 1) & mask];
            wet += a + (b - a) * frac;
        }
        self.pos = (self.pos + 1) & mask;
        x * (1.0 - 0.45 * amount) + wet * amount * 0.42
    }
}

pub struct MacroSynth {
    patch: MacroPatch,
    voices: Vec<Voice>,
    ensemble: Ensemble,
    scratch: [f32; 64],
    sr: f32,
    clock: u64,
}

impl MacroSynth {
    pub fn new(patch: MacroPatch, sr: f32) -> Self {
        // build the tables on the control thread, never in the audio callback
        let _ = tables();
        MacroSynth {
            patch: patch.clamped(),
            voices: (0..VOICES)
                .map(|i| Voice::new(0x5EED + i as u32 * 977, sr))
                .collect(),
            ensemble: Ensemble::new(),
            scratch: [0.0; 64],
            sr,
            clock: 0,
        }
    }

    pub fn set_patch(&mut self, patch: MacroPatch) {
        let patch = patch.clamped();
        for v in self.voices.iter_mut() {
            v.env.set(
                patch.attack,
                patch.decay,
                patch.sustain,
                patch.release,
                self.sr,
            );
        }
        self.patch = patch;
    }

    pub fn note_on(&mut self, pitch: u8, velocity: f32) {
        self.clock += 1;
        // the same key again takes its own voice; else a free one; else the oldest
        let idx = self
            .voices
            .iter()
            .position(|v| v.pitch == pitch && v.env.is_active())
            .or_else(|| self.voices.iter().position(|v| !v.env.is_active()))
            .unwrap_or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, v)| v.age)
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            });
        let (patch, sr, clock) = (self.patch, self.sr, self.clock);
        self.voices[idx].start(pitch, velocity, &patch, sr, clock);
    }

    pub fn note_off(&mut self, pitch: u8) {
        for v in self
            .voices
            .iter_mut()
            .filter(|v| v.pitch == pitch && v.held)
        {
            v.held = false;
            v.env.gate_off();
        }
    }

    pub fn all_notes_off(&mut self) {
        for v in self.voices.iter_mut() {
            v.held = false;
            v.env.gate_off();
        }
    }

    /// Add `out.len()` samples into `out`.
    pub fn render(&mut self, out: &mut [f32]) {
        let (patch, sr) = (self.patch, self.sr);
        for block in out.chunks_mut(64) {
            let n = block.len();
            let mix = &mut self.scratch[..n];
            mix.fill(0.0);
            for v in self.voices.iter_mut().filter(|v| v.env.is_active()) {
                v.render(&patch, mix, sr);
            }
            for (o, x) in block.iter_mut().zip(mix.iter()) {
                // a stray NaN must never reach the mix
                let x = if x.is_finite() { *x } else { 0.0 };
                *o += self.ensemble.process(x, patch.chorus, sr);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn every_preset_sounds_stays_finite_and_ends() {
        for p in PRESETS.iter() {
            let mut s = MacroSynth::new(p.patch, 48000.0);
            let pitch = (p.octave as u8 + 1) * 12 + 9;
            s.note_on(pitch, 0.8);
            let mut buf = vec![0.0; 48000];
            for c in buf.chunks_mut(16) {
                s.render(c);
            }
            assert!(buf.iter().all(|x| x.is_finite()), "{} not finite", p.id);
            let level = rms(&buf[..24000]);
            assert!(level > 0.003, "{} too quiet: {level}", p.id);
            assert!(buf.iter().all(|x| x.abs() < 2.5), "{} too loud", p.id);
            s.note_off(pitch);
            let mut tail = vec![0.0; 48000 * 14];
            for c in tail.chunks_mut(16) {
                s.render(c);
            }
            assert!(rms(&tail[48000 * 13..]) < 0.002, "{} never ends", p.id);
        }
    }

    #[test]
    fn knobs_stay_in_range() {
        let mut p = MacroPatch::default();
        p.set("engine", 99.0).unwrap();
        assert_eq!(p.engine, (ENGINES.len() - 1) as f32);
        p.set("harmonics", -3.0).unwrap();
        assert_eq!(p.harmonics, 0.0);
        assert!(p.set("nope", 1.0).is_err());
    }
}
