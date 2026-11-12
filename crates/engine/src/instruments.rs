//! Sound sources: subtractive polysynth, synthesized drum kits and samplers.
//! Every instrument renders mono; the per-track FX chain makes it stereo.

use std::sync::Arc;

use crate::dsp::{midi_to_hz, Adsr, Decay, Noise, Osc, Svf};
use crate::model::{DrumKitId, InstrumentSpec, SynthPreset, DRUM_BASE_NOTE, DRUM_PADS, MAX_PARTS};

/// Mono PCM sample shared between the control and audio threads.
#[derive(Debug, Clone, PartialEq)]
pub struct SampleData {
    pub frames: Vec<f32>,
    pub sample_rate: u32,
}

pub enum Instrument {
    Synth(Synth),
    Drums(DrumKit),
    Sampler(Sampler),
    Granular(crate::granular::Granular),
    Piano(crate::piano::FeltPiano),
}

impl Instrument {
    /// Build an instrument. `sample` must be provided for sample-based specs
    /// (silence is used otherwise).
    pub fn build(spec: &InstrumentSpec, sample: Option<Arc<SampleData>>, sr: f32) -> Instrument {
        let empty = || {
            Arc::new(SampleData {
                frames: vec![0.0; 1],
                sample_rate: sr as u32,
            })
        };
        match spec {
            InstrumentSpec::Synth {
                preset: SynthPreset::Piano,
            } => Instrument::Piano(crate::piano::FeltPiano::new(sr)),
            InstrumentSpec::Synth {
                preset: SynthPreset::Pluck,
            } => Instrument::Piano(crate::piano::FeltPiano::with_profile(
                crate::piano::ModalProfile::SOFT_PLUCK,
                sr,
            )),
            InstrumentSpec::Synth { preset } => Instrument::Synth(Synth::new(*preset, sr)),
            InstrumentSpec::Drums {
                kit: DrumKitId::Dust,
            } => {
                let (kit, slices) = crate::library::dusty_kit();
                Instrument::Sampler(Sampler::kit(kit, slices, sr))
            }
            InstrumentSpec::Drums { kit } => Instrument::Drums(DrumKit::new(*kit, sr)),
            InstrumentSpec::CustomSynth { patch, .. } => {
                Instrument::Synth(Synth::with_patch(patch.clamped(), sr))
            }
            InstrumentSpec::CustomKit { pads, .. } => {
                Instrument::Drums(DrumKit::designed(pads, sr))
            }
            InstrumentSpec::SampleMelodic { root, .. } => {
                Instrument::Sampler(Sampler::melodic(sample.unwrap_or_else(empty), *root, sr))
            }
            InstrumentSpec::SampleKit { slices, .. } => Instrument::Sampler(Sampler::kit(
                sample.unwrap_or_else(empty),
                slices.clone(),
                sr,
            )),
            InstrumentSpec::Granular { texture, .. } => {
                use crate::granular::Granular;
                match sample {
                    Some(src) => {
                        let root = crate::sampling::detect_pitch(&src).unwrap_or(60);
                        Instrument::Granular(Granular::new(src, root, *texture, true, sr))
                    }
                    None => {
                        let (src, root) = Granular::texture_source(*texture, sr);
                        Instrument::Granular(Granular::new(src, root, *texture, false, sr))
                    }
                }
            }
        }
    }

    pub fn note_on(&mut self, pitch: u8, velocity: f32) {
        match self {
            Instrument::Synth(s) => s.note_on(pitch, velocity),
            Instrument::Drums(d) => d.note_on(pitch, velocity),
            Instrument::Sampler(s) => s.note_on(pitch, velocity),
            Instrument::Granular(g) => g.note_on(pitch, velocity),
            Instrument::Piano(p) => p.note_on(pitch, velocity),
        }
    }

    pub fn note_off(&mut self, pitch: u8) {
        match self {
            Instrument::Synth(s) => s.note_off(pitch),
            Instrument::Drums(_) => {}
            Instrument::Sampler(s) => s.note_off(pitch),
            Instrument::Granular(g) => g.note_off(pitch),
            Instrument::Piano(p) => p.note_off(pitch),
        }
    }

    pub fn all_notes_off(&mut self) {
        match self {
            Instrument::Synth(s) => s.voices.iter_mut().for_each(|v| v.amp.gate_off()),
            Instrument::Drums(_) => {}
            Instrument::Sampler(s) => s.voices.iter_mut().for_each(|v| v.env.gate_off()),
            Instrument::Granular(g) => g.all_notes_off(),
            Instrument::Piano(p) => p.all_notes_off(),
        }
    }

    /// Per-part gains (drum pads, synth layers, piano strings/hammer).
    /// Instruments without parts ignore them.
    pub fn set_part_gains(&mut self, gains: &[f32; MAX_PARTS]) {
        match self {
            Instrument::Synth(s) => s.parts.copy_from_slice(&gains[..SYNTH_LAYERS]),
            Instrument::Drums(d) => d.parts = *gains,
            Instrument::Sampler(s) => s.parts = *gains,
            Instrument::Granular(_) => {}
            Instrument::Piano(p) => p.set_part_gains(gains[0], gains[1]),
        }
    }

    /// Add `out.len()` samples into `out`.
    pub fn render(&mut self, out: &mut [f32]) {
        match self {
            Instrument::Synth(s) => s.render(out),
            Instrument::Drums(d) => d.render(out),
            Instrument::Sampler(s) => s.render(out),
            Instrument::Granular(g) => g.render(out),
            Instrument::Piano(p) => p.render(out),
        }
    }
}

// ------------------------------------------------------------------ Synth
//
// A small "modular" voice: sine (optionally phase-modulated by a second
// sine = 2-op FM), triangle, saw pair, square, sub and noise, into a
// resonant low-pass. Optional low-pass gate (LPG) couples the filter to the
// amplitude envelope like a vactrol, for west-coast style plucks and bongos.
// Every voice drifts a little in pitch (analog/tape feel) and the whole
// instrument can share a slow wow or a vibrato.

// The patch is a designable `SynthPatch`; presets are patches written here.
use crate::sound::{DrumPad, SynthPatch as Patch};

/// The patch of a built-in preset (the felt piano and the soft pluck are
/// physical models; designing them starts from the FM electric piano).
pub fn patch_for(p: SynthPreset) -> Patch {
    let base = Patch::default();
    match p {
        // FM electric piano: carrier + 1:1 modulator whose brightness decays,
        // the classic tine "bark" that settles into a round tone.
        SynthPreset::Rhodes => Patch {
            sine: 1.0,
            fm_ratio: 1.0,
            fm_index: 1.8,
            fm_decay: 0.5,
            fm_sustain: 0.12,
            attack: 0.002,
            decay: 2.2,
            sustain: 0.25,
            release: 0.45,
            cutoff: 2600.0,
            env_amount: 1800.0,
            filter_decay: 0.4,
            drift_cents: 5.0,
            vibrato_rate: 4.8,
            vibrato_cents: 3.0,
            gain: 0.3,
            ..base
        },
        // Dusty soft keys: triangle with a hint of square, low-passed.
        SynthPreset::Keys => Patch {
            tri: 0.8,
            square: 0.12,
            detune_cents: 6.0,
            attack: 0.004,
            decay: 1.2,
            sustain: 0.4,
            release: 0.4,
            cutoff: 1100.0,
            env_amount: 1400.0,
            filter_decay: 0.35,
            drift_cents: 7.0,
            gain: 0.3,
            ..base
        },
        // West-coast pluck: triangle + light FM through a low-pass gate.
        SynthPreset::Pluck => Patch {
            tri: 0.7,
            sine: 0.4,
            fm_ratio: 2.0,
            fm_index: 0.8,
            fm_decay: 0.09,
            attack: 0.002,
            decay: 0.85,
            sustain: 0.0,
            release: 0.45,
            cutoff: 180.0,
            env_amount: 3800.0,
            q: 1.1,
            lpg: 1.0,
            drift_cents: 6.0,
            gain: 1.15,
            ..base
        },
        // Round bass: triangle + sine sub, gentle glide.
        SynthPreset::Bass => Patch {
            tri: 0.55,
            sub: 0.75,
            saw: 0.12,
            attack: 0.004,
            decay: 0.6,
            sustain: 0.7,
            release: 0.12,
            cutoff: 320.0,
            env_amount: 650.0,
            filter_decay: 0.2,
            q: 1.0,
            key_track: 0.5,
            drift_cents: 3.0,
            glide: 0.035,
            gain: 0.4,
            mono: 1.0,
            ..base
        },
        // Tape pad: detuned saws, slow attack, dark, with a seasick wow.
        SynthPreset::Pad => Patch {
            saw: 0.55,
            tri: 0.4,
            detune_cents: 13.0,
            attack: 0.9,
            decay: 1.5,
            sustain: 0.8,
            release: 1.8,
            cutoff: 750.0,
            env_amount: 500.0,
            filter_decay: 2.0,
            drift_cents: 9.0,
            vibrato_rate: 0.35,
            vibrato_cents: 9.0,
            gain: 0.4,
            ..base
        },
        // Soft lead: sine + triangle, vibrato and glide.
        SynthPreset::Lead => Patch {
            sine: 0.7,
            tri: 0.5,
            fm_ratio: 2.0,
            fm_index: 0.4,
            fm_decay: 0.3,
            fm_sustain: 0.1,
            attack: 0.02,
            decay: 0.6,
            sustain: 0.75,
            release: 0.25,
            cutoff: 2200.0,
            env_amount: 900.0,
            vibrato_rate: 5.2,
            vibrato_cents: 14.0,
            glide: 0.07,
            gain: 0.24,
            mono: 1.0,
            ..base
        },
        // Procedural felt piano, built by `Instrument::build`; kept total for safety.
        SynthPreset::Piano => patch_for(SynthPreset::Rhodes),
        SynthPreset::SubBass => Patch {
            sub: 1.0,
            tri: 0.08,
            decay: 1.2,
            sustain: 0.0,
            release: 0.15,
            cutoff: 400.0,
            env_amount: 200.0,
            drift_cents: 1.0,
            gain: 0.55,
            mono: 1.0,
            ..base
        },
        // 808: a pure sine that drops into the note and rings for seconds,
        // driven into soft clipping. Mono with glide: overlapping notes slide.
        SynthPreset::Bass808 => Patch {
            sine: 1.0,
            attack: 0.001,
            decay: 3.2,
            sustain: 0.0,
            release: 0.18,
            cutoff: 2400.0,
            env_amount: 0.0,
            key_track: 0.0,
            drift_cents: 1.0,
            glide: 0.06,
            pitch_drop: 10.0,
            pitch_drop_decay: 0.035,
            drive: 0.45,
            gain: 0.5,
            mono: 1.0,
            ..base
        },
        // FM bell: inharmonic 3.5 ratio, long ring.
        SynthPreset::FmBell => Patch {
            sine: 1.0,
            fm_ratio: 3.5,
            fm_index: 3.2,
            fm_decay: 0.45,
            fm_sustain: 0.05,
            attack: 0.001,
            decay: 2.4,
            sustain: 0.0,
            release: 1.4,
            cutoff: 5000.0,
            env_amount: 2000.0,
            drift_cents: 3.0,
            gain: 0.3,
            ..base
        },
        // Mallet / "bongo": pinged sine with a quick FM knock through an LPG.
        SynthPreset::Mallet => Patch {
            sine: 1.0,
            fm_ratio: 1.5,
            fm_index: 2.4,
            fm_decay: 0.05,
            attack: 0.001,
            decay: 0.32,
            sustain: 0.0,
            release: 0.25,
            cutoff: 200.0,
            env_amount: 6000.0,
            q: 1.1,
            lpg: 1.0,
            drift_cents: 2.0,
            gain: 0.9,
            ..base
        },
    }
}

/// Synth layers mixed on their own, in part order.
pub const SYNTH_LAYERS: usize = 6;

/// The parts of an instrument that can be mixed on their own, as
/// (part index, name). Empty when there is only one sound to mix.
pub fn part_names(spec: &InstrumentSpec) -> Vec<(usize, String)> {
    match spec {
        InstrumentSpec::Drums { kit } => kit
            .pad_names()
            .iter()
            .enumerate()
            .map(|(i, n)| (i, n.to_string()))
            .collect(),
        InstrumentSpec::SampleKit { .. } => (0..DRUM_PADS)
            .map(|i| (i, format!("Slice {}", i + 1)))
            .collect(),
        InstrumentSpec::Synth {
            preset: SynthPreset::Piano,
        } => vec![(0, "Strings".into()), (1, "Hammer".into())],
        InstrumentSpec::Synth {
            preset: SynthPreset::Pluck,
        } => vec![(0, "Strings".into()), (1, "Finger".into())],
        InstrumentSpec::CustomKit { names, .. } => (0..DRUM_PADS)
            .map(|i| {
                (
                    i,
                    names
                        .get(i)
                        .cloned()
                        .unwrap_or_else(|| format!("Pad {}", i + 1)),
                )
            })
            .collect(),
        InstrumentSpec::Synth { .. } | InstrumentSpec::CustomSynth { .. } => {
            let p = match spec {
                InstrumentSpec::CustomSynth { patch, .. } => *patch,
                InstrumentSpec::Synth { preset } => patch_for(*preset),
                _ => unreachable!(),
            };
            let fm = if p.fm_index > 0.0 { "FM tone" } else { "Sine" };
            let layers: Vec<(usize, String)> = [
                (p.sine, fm),
                (p.tri, "Triangle"),
                (p.saw, "Saw"),
                (p.square, "Square"),
                (p.sub, "Sub"),
                (p.noise, "Noise"),
            ]
            .into_iter()
            .enumerate()
            .filter(|(_, (level, _))| *level > 0.0)
            .map(|(i, (_, name))| (i, name.to_string()))
            .collect();
            if layers.len() > 1 {
                layers
            } else {
                Vec::new()
            }
        }
        InstrumentSpec::SampleMelodic { .. } | InstrumentSpec::Granular { .. } => Vec::new(),
    }
}

struct SynthVoice {
    pitch: u8,
    freq: f32,
    target_freq: f32,
    vel: f32,
    carrier: Osc,
    modulator: Osc,
    tri: Osc,
    saw1: Osc,
    saw2: Osc,
    square: Osc,
    sub: Osc,
    amp: Adsr,
    fenv: Decay,
    fm_env: Decay,
    pitch_env: Decay,
    filter: Svf,
    drift: f32,
    drift_target: f32,
    /// Per-note random variation (brightness, FM depth) so no two hits match.
    var_bright: f32,
    var_fm: f32,
    /// Vactrol state of the low-pass gate (lags behind the envelope).
    vactrol: f32,
    age: u64,
}

pub struct Synth {
    voices: Vec<SynthVoice>,
    patch: Patch,
    /// Gain of each layer: sine/FM, triangle, saw, square, sub, noise.
    parts: [f32; SYNTH_LAYERS],
    noise: Noise,
    sr: f32,
    clock: u64,
    vib_phase: f32,
    drift_timer: u32,
}

#[inline]
fn tri_wave(osc: &mut Osc, freq: f32, sr: f32) -> f32 {
    let p = osc.phase;
    osc.phase += freq / sr;
    if osc.phase >= 1.0 {
        osc.phase -= osc.phase.floor();
    }
    4.0 * (p - 0.5).abs() - 1.0
}

impl Synth {
    pub fn new(preset: SynthPreset, sr: f32) -> Self {
        Self::with_patch(patch_for(preset), sr)
    }

    /// Change the patch live: oscillators, filter and pitch follow at once;
    /// envelopes take the new times from the next note.
    pub fn set_patch(&mut self, patch: Patch) {
        self.patch = patch;
    }

    pub fn with_patch(patch: Patch, sr: f32) -> Self {
        // always 12 voices, so a designed patch can switch Mono on and off live
        let n = 12;
        let voices = (0..n)
            .map(|i| SynthVoice {
                pitch: 0,
                freq: 440.0,
                target_freq: 440.0,
                vel: 0.0,
                carrier: Osc::default(),
                modulator: Osc::default(),
                tri: Osc { phase: 0.25 },
                saw1: Osc {
                    phase: 0.33 * i as f32 % 1.0,
                },
                saw2: Osc {
                    phase: 0.71 * i as f32 % 1.0,
                },
                square: Osc::default(),
                sub: Osc::default(),
                amp: Adsr::new(patch.attack, patch.decay, patch.sustain, patch.release, sr),
                fenv: Decay::default(),
                fm_env: Decay::default(),
                pitch_env: Decay::default(),
                filter: Svf::default(),
                drift: 0.0,
                drift_target: 0.0,
                var_bright: 1.0,
                var_fm: 1.0,
                vactrol: 0.0,
                age: 0,
            })
            .collect();
        Synth {
            voices,
            patch,
            parts: [1.0; SYNTH_LAYERS],
            noise: Noise::new(1234),
            sr,
            clock: 0,
            vib_phase: 0.0,
            drift_timer: 0,
        }
    }

    pub fn note_on(&mut self, pitch: u8, velocity: f32) {
        self.clock += 1;
        let idx = if self.patch.mono > 0.5 {
            0
        } else if let Some(i) = self
            .voices
            .iter()
            .position(|v| v.pitch == pitch && v.amp.is_active())
        {
            i
        } else if let Some(i) = self.voices.iter().position(|v| !v.amp.is_active()) {
            i
        } else if let Some(i) = self
            .voices
            .iter()
            .enumerate()
            .filter(|(_, v)| v.amp.is_releasing())
            .min_by_key(|(_, v)| v.age)
            .map(|(i, _)| i)
        {
            i
        } else {
            self.voices
                .iter()
                .enumerate()
                .min_by_key(|(_, v)| v.age)
                .map(|(i, _)| i)
                .unwrap_or(0)
        };
        let p = self.patch;
        let sr = self.sr;
        let (r1, r2, r3, r4) = (
            self.noise.next(),
            self.noise.next(),
            self.noise.next(),
            self.noise.next(),
        );
        let v = &mut self.voices[idx];
        let legato = p.mono > 0.5 && v.amp.is_active() && !v.amp.is_releasing();
        // small per-note differences: brightness, FM depth, decay, start phase
        v.var_bright = 1.0 + 0.14 * r1;
        v.var_fm = 1.0 + 0.18 * r2;
        v.amp.set(
            p.attack,
            p.decay * (1.0 + 0.12 * r3),
            p.sustain,
            p.release,
            sr,
        );
        v.pitch = pitch;
        v.target_freq = midi_to_hz(pitch as f32);
        if !legato || p.glide <= 0.0 {
            v.freq = v.target_freq;
        }
        if !legato {
            // free-running-ish phases: each attack lands somewhere different
            let ph = r4 * 0.5 + 0.5;
            // carrier and modulator keep the same relative phase (no DC from PM)
            v.carrier.phase = ph;
            v.modulator.phase = ph;
            v.tri.phase = (ph * 0.7 + 0.25) % 1.0;
        }
        v.vel = velocity;
        v.age = self.clock;
        v.amp.gate_on();
        v.fenv.trigger(1.0, p.filter_decay, sr);
        v.fm_env.trigger(1.0, p.fm_decay, sr);
        // a slide keeps its pitch; only a fresh attack drops into the note
        if p.pitch_drop > 0.0 && !legato {
            v.pitch_env.trigger(1.0, p.pitch_drop_decay, sr);
        }
    }

    pub fn note_off(&mut self, pitch: u8) {
        for v in self.voices.iter_mut().filter(|v| v.pitch == pitch) {
            v.amp.gate_off();
        }
    }

    fn render(&mut self, out: &mut [f32]) {
        let mut p = self.patch;
        // the layer mix scales each oscillator's level (a silenced layer is skipped)
        let g = self.parts;
        p.sine *= g[0];
        p.tri *= g[1];
        p.saw *= g[2];
        p.square *= g[3];
        p.sub *= g[4];
        p.noise *= g[5];
        let sr = self.sr;
        let det = 2f32.powf(p.detune_cents / 1200.0);
        let glide = if p.glide > 0.0 {
            (-1.0 / (p.glide * sr)).exp()
        } else {
            0.0
        };
        // shared vibrato / wow for this block
        let vib = if p.vibrato_cents > 0.0 {
            self.vib_phase = (self.vib_phase + p.vibrato_rate * out.len() as f32 / sr) % 1.0;
            2f32.powf(p.vibrato_cents * (std::f32::consts::TAU * self.vib_phase).sin() / 1200.0)
        } else {
            1.0
        };
        // pick new drift targets now and then (~every 0.4 s)
        self.drift_timer += out.len() as u32;
        let new_drift = self.drift_timer as f32 > 0.4 * sr;
        if new_drift {
            self.drift_timer = 0;
        }
        let drift_coef = (-(out.len() as f32) / (0.5 * sr)).exp();
        let vac_up = (-1.0 / (0.002 * sr)).exp();
        let vac_down = (-1.0 / (0.07 * sr)).exp();
        let lpg = p.lpg > 0.5;
        let drive = 1.0 + p.drive * 5.0;
        let drive_norm = 1.0 / drive.tanh();
        for v in self.voices.iter_mut() {
            if new_drift {
                v.drift_target = self.noise.next() * p.drift_cents;
            }
            v.drift = v.drift_target + (v.drift - v.drift_target) * drift_coef;
            if !v.amp.is_active() {
                continue;
            }
            let pitch_mul = vib * 2f32.powf(v.drift / 1200.0);
            let key = (v.target_freq / 261.63).powf(p.key_track);
            let mut filter_update = 0;
            for o in out.iter_mut() {
                v.freq = v.target_freq + (v.freq - v.target_freq) * glide;
                let mut f = v.freq * pitch_mul;
                if p.pitch_drop > 0.0 {
                    f *= (p.pitch_drop * v.pitch_env.next() / 12.0).exp2();
                }
                let mut s = 0.0;
                if p.sine > 0.0 {
                    let index =
                        (p.fm_sustain + (p.fm_index - p.fm_sustain) * v.fm_env.value) * v.var_fm;
                    let m = if p.fm_index > 0.0 {
                        v.modulator.sine(f * p.fm_ratio, sr) * index
                    } else {
                        0.0
                    };
                    // phase modulation of the carrier
                    let ph = v.carrier.phase + m / std::f32::consts::TAU;
                    s += p.sine * (std::f32::consts::TAU * ph).sin();
                    v.carrier.phase = (v.carrier.phase + f / sr).fract();
                }
                if p.tri > 0.0 {
                    s += p.tri * tri_wave(&mut v.tri, f, sr);
                }
                if p.saw > 0.0 {
                    s += p.saw * 0.5 * (v.saw1.saw(f, sr) + v.saw2.saw(f * det, sr));
                }
                if p.square > 0.0 {
                    s += p.square * v.square.square(f / det, sr);
                }
                if p.sub > 0.0 {
                    s += p.sub * v.sub.sine(f * 0.5, sr);
                }
                if p.noise > 0.0 {
                    s += p.noise * self.noise.next();
                }
                let a = v.amp.next();
                if lpg {
                    // vactrol: opens in ~2 ms, closes over ~70 ms with a soft curve
                    let target = a.powf(1.6);
                    let coef = if target > v.vactrol { vac_up } else { vac_down };
                    v.vactrol = target + (v.vactrol - target) * coef;
                }
                if filter_update == 0 {
                    let env = if lpg { v.vactrol } else { v.fenv.value };
                    let cutoff =
                        (p.cutoff + p.env_amount * env * (0.5 + 0.5 * v.vel)) * key * v.var_bright;
                    v.filter.set(cutoff, p.q, sr);
                    filter_update = 8;
                }
                filter_update -= 1;
                v.fenv.next();
                v.fm_env.next();
                let mut y = v.filter.process(s).low;
                if p.drive > 0.0 {
                    y = (y * drive).tanh() * drive_norm;
                }
                // the vactrol also rounds off the loudness tail
                let amp = if lpg {
                    a * (0.35 + 0.65 * v.vactrol.sqrt())
                } else {
                    a
                };
                *o += y * amp * v.vel * p.gain;
            }
        }
    }
}

// ------------------------------------------------------------------ Drums

/// Recipe for one synthesized drum sound.
#[derive(Clone, Copy, Default)]
struct DrumSound {
    tone_start: f32,
    tone_end: f32,
    pitch_decay: f32,
    tone_level: f32,
    tone_decay: f32,
    noise_level: f32,
    noise_decay: f32,
    /// 0 = lowpass, 1 = bandpass, 2 = highpass
    noise_mode: u8,
    noise_freq: f32,
    noise_q: f32,
    metal_level: f32,
    metal_decay: f32,
    metal_freq: f32,
    click: f32,
    drive: f32,
    /// Number of noise bursts (claps).
    bursts: u8,
    gain: f32,
    /// Choke group (0 = none). A hit chokes other voices in the same group.
    choke: u8,
    /// Sample-rate reduction: hold each sample this many times (0/1 = off).
    crush: u8,
    /// Bit depth reduction (0 = off), vintage-sampler grit.
    bits: u8,
}

impl From<DrumPad> for DrumSound {
    fn from(p: DrumPad) -> Self {
        DrumSound {
            tone_start: p.tone_start,
            tone_end: p.tone_end,
            pitch_decay: p.pitch_decay,
            tone_level: p.tone_level,
            tone_decay: p.tone_decay,
            noise_level: p.noise_level,
            noise_decay: p.noise_decay,
            noise_mode: p.noise_mode.clamp(0.0, 2.0) as u8,
            noise_freq: p.noise_freq,
            noise_q: p.noise_q,
            metal_level: p.metal_level,
            metal_decay: p.metal_decay,
            metal_freq: p.metal_freq,
            click: p.click,
            drive: p.drive,
            bursts: p.bursts.clamp(1.0, 8.0) as u8,
            gain: p.gain,
            choke: p.choke.clamp(0.0, 4.0) as u8,
            crush: p.crush.clamp(0.0, 16.0) as u8,
            bits: p.bits.clamp(0.0, 16.0) as u8,
        }
    }
}

impl From<DrumSound> for DrumPad {
    fn from(s: DrumSound) -> Self {
        DrumPad {
            tone_start: s.tone_start,
            tone_end: s.tone_end,
            pitch_decay: s.pitch_decay,
            tone_level: s.tone_level,
            tone_decay: s.tone_decay,
            noise_level: s.noise_level,
            noise_decay: s.noise_decay,
            noise_mode: s.noise_mode as f32,
            noise_freq: s.noise_freq,
            noise_q: s.noise_q,
            metal_level: s.metal_level,
            metal_decay: s.metal_decay,
            metal_freq: s.metal_freq,
            click: s.click,
            drive: s.drive,
            bursts: s.bursts.max(1) as f32,
            gain: s.gain,
            choke: s.choke as f32,
            crush: s.crush as f32,
            bits: s.bits as f32,
        }
        .clamped()
    }
}

/// The pads of a synthesized kit as designable sounds (the sampled Dust kit
/// has none: designing it starts from the Lo-Fi kit).
pub fn kit_pads(kit: DrumKitId) -> Vec<DrumPad> {
    let kit = if kit == DrumKitId::Dust {
        DrumKitId::LoFi
    } else {
        kit
    };
    kit_sounds(kit).into_iter().map(DrumPad::from).collect()
}

fn kit_sounds(kit: DrumKitId) -> [DrumSound; DRUM_PADS] {
    let d = DrumSound {
        gain: 1.0,
        noise_q: 0.7,
        ..Default::default()
    };
    // Shared building blocks, tweaked per kit below.
    let kick = DrumSound {
        tone_start: 150.0,
        tone_end: 50.0,
        pitch_decay: 0.05,
        tone_level: 1.0,
        tone_decay: 0.35,
        click: 0.3,
        ..d
    };
    let snare = DrumSound {
        tone_start: 220.0,
        tone_end: 180.0,
        pitch_decay: 0.02,
        tone_level: 0.5,
        tone_decay: 0.1,
        noise_level: 0.7,
        noise_decay: 0.18,
        noise_mode: 2,
        noise_freq: 1500.0,
        ..d
    };
    let clap = DrumSound {
        noise_level: 0.9,
        noise_decay: 0.2,
        noise_mode: 1,
        noise_freq: 1200.0,
        noise_q: 1.2,
        bursts: 3,
        ..d
    };
    let chh = DrumSound {
        metal_level: 0.5,
        metal_decay: 0.05,
        metal_freq: 320.0,
        noise_level: 0.3,
        noise_decay: 0.04,
        noise_mode: 2,
        noise_freq: 7000.0,
        choke: 1,
        gain: 0.6,
        ..d
    };
    let ohh = DrumSound {
        metal_decay: 0.45,
        noise_decay: 0.4,
        ..chh
    };
    let tom = |f: f32| DrumSound {
        tone_start: f * 1.5,
        tone_end: f,
        pitch_decay: 0.08,
        tone_level: 0.9,
        tone_decay: 0.3,
        click: 0.1,
        ..d
    };
    let rim = DrumSound {
        tone_start: 1700.0,
        tone_end: 1600.0,
        pitch_decay: 0.01,
        tone_level: 0.6,
        tone_decay: 0.03,
        click: 0.6,
        noise_level: 0.2,
        noise_decay: 0.02,
        noise_mode: 1,
        noise_freq: 3000.0,
        gain: 0.7,
        ..d
    };
    let cowbell = DrumSound {
        metal_level: 0.0,
        tone_start: 800.0,
        tone_end: 800.0,
        tone_level: 0.5,
        tone_decay: 0.25,
        noise_level: 0.0,
        ..d
    };
    let crash = DrumSound {
        metal_level: 0.5,
        metal_decay: 1.4,
        metal_freq: 420.0,
        noise_level: 0.4,
        noise_decay: 1.2,
        noise_mode: 2,
        noise_freq: 5000.0,
        gain: 0.5,
        ..d
    };
    let ride = DrumSound {
        metal_level: 0.45,
        metal_decay: 0.9,
        metal_freq: 520.0,
        noise_level: 0.15,
        noise_decay: 0.6,
        noise_mode: 1,
        noise_freq: 8000.0,
        noise_q: 2.0,
        gain: 0.45,
        ..d
    };
    let shaker = DrumSound {
        noise_level: 0.6,
        noise_decay: 0.07,
        noise_mode: 1,
        noise_freq: 6000.0,
        noise_q: 1.5,
        gain: 0.5,
        ..d
    };
    let clave = DrumSound {
        tone_start: 2500.0,
        tone_end: 2500.0,
        tone_level: 0.7,
        tone_decay: 0.05,
        gain: 0.6,
        ..d
    };

    let mut s = [
        kick,
        snare,
        clap,
        chh,
        ohh,
        tom(80.0),
        tom(120.0),
        tom(170.0),
        rim,
        cowbell,
        crash,
        ride,
        shaker,
        kick,
        snare,
        clave,
    ];
    match kit {
        // Sampled kit, built by `Instrument::build`; synthesized fallback only.
        DrumKitId::Dust => {}
        DrumKitId::Tr808 => {
            s[0] = DrumSound {
                tone_start: 110.0,
                tone_end: 48.0,
                pitch_decay: 0.04,
                tone_decay: 0.9,
                click: 0.05,
                drive: 0.2,
                ..kick
            };
            s[1] = DrumSound {
                tone_start: 240.0,
                tone_end: 190.0,
                tone_level: 0.6,
                noise_level: 0.6,
                noise_decay: 0.15,
                noise_freq: 2500.0,
                ..snare
            };
            s[9] = DrumSound {
                tone_start: 540.0,
                tone_end: 540.0,
                tone_level: 0.35,
                tone_decay: 0.3,
                metal_level: 0.35,
                metal_decay: 0.3,
                metal_freq: 800.0,
                noise_level: 0.0,
                ..d
            };
            s[13] = DrumSound {
                tone_start: 70.0,
                tone_end: 40.0,
                tone_decay: 1.6,
                drive: 0.5,
                click: 0.0,
                ..kick
            };
            s[14] = DrumSound {
                tone_level: 0.2,
                noise_level: 0.9,
                noise_decay: 0.25,
                ..s[1]
            };
        }
        DrumKitId::Tr909 => {
            s[0] = DrumSound {
                tone_start: 260.0,
                tone_end: 55.0,
                pitch_decay: 0.03,
                tone_decay: 0.45,
                click: 0.7,
                drive: 0.6,
                ..kick
            };
            s[1] = DrumSound {
                tone_start: 260.0,
                tone_end: 200.0,
                tone_level: 0.45,
                noise_level: 0.9,
                noise_decay: 0.22,
                noise_freq: 3500.0,
                ..snare
            };
            s[3] = DrumSound {
                metal_level: 0.2,
                noise_level: 0.7,
                noise_freq: 9000.0,
                ..chh
            };
            s[4] = DrumSound {
                metal_level: 0.2,
                noise_level: 0.7,
                noise_freq: 9000.0,
                ..ohh
            };
            s[13] = DrumSound {
                drive: 1.0,
                tone_decay: 0.3,
                ..s[0]
            };
        }
        DrumKitId::Acoustic => {
            s[0] = DrumSound {
                tone_start: 95.0,
                tone_end: 58.0,
                pitch_decay: 0.03,
                tone_decay: 0.25,
                click: 0.5,
                noise_level: 0.25,
                noise_decay: 0.03,
                noise_mode: 0,
                noise_freq: 1500.0,
                ..kick
            };
            s[1] = DrumSound {
                tone_start: 200.0,
                tone_end: 170.0,
                tone_level: 0.55,
                tone_decay: 0.12,
                noise_level: 0.85,
                noise_decay: 0.28,
                noise_mode: 1,
                noise_freq: 3000.0,
                noise_q: 0.6,
                ..snare
            };
            s[2] = DrumSound {
                noise_level: 0.9,
                noise_decay: 0.15,
                noise_mode: 1,
                noise_freq: 1800.0,
                bursts: 4,
                ..clap
            };
            s[3] = DrumSound {
                metal_level: 0.25,
                noise_level: 0.6,
                noise_decay: 0.05,
                noise_freq: 8000.0,
                ..chh
            };
            s[4] = DrumSound {
                metal_level: 0.25,
                noise_level: 0.6,
                noise_decay: 0.6,
                metal_decay: 0.6,
                noise_freq: 8000.0,
                ..ohh
            };
            s[5] = DrumSound {
                tone_decay: 0.45,
                noise_level: 0.15,
                noise_decay: 0.1,
                noise_mode: 0,
                noise_freq: 900.0,
                ..tom(90.0)
            };
            s[6] = DrumSound {
                tone_decay: 0.4,
                noise_level: 0.15,
                noise_decay: 0.1,
                noise_mode: 0,
                noise_freq: 1100.0,
                ..tom(130.0)
            };
            s[7] = DrumSound {
                tone_decay: 0.35,
                noise_level: 0.15,
                noise_decay: 0.1,
                noise_mode: 0,
                noise_freq: 1300.0,
                ..tom(180.0)
            };
        }
        // Dusty sampler drums: soft boomy kick, warm snare, dark hats, all
        // through sample-rate and bit reduction.
        DrumKitId::LoFi => {
            let dust = DrumSound {
                crush: 3,
                bits: 10,
                ..d
            };
            s = [
                DrumSound {
                    tone_start: 95.0,
                    tone_end: 52.0,
                    pitch_decay: 0.035,
                    tone_level: 1.0,
                    tone_decay: 0.32,
                    click: 0.12,
                    noise_level: 0.1,
                    noise_decay: 0.02,
                    noise_mode: 0,
                    noise_freq: 900.0,
                    drive: 0.35,
                    ..dust
                },
                DrumSound {
                    tone_start: 185.0,
                    tone_end: 165.0,
                    pitch_decay: 0.02,
                    tone_level: 0.45,
                    tone_decay: 0.1,
                    noise_level: 0.6,
                    noise_decay: 0.2,
                    noise_mode: 1,
                    noise_freq: 1600.0,
                    noise_q: 0.8,
                    drive: 0.3,
                    ..dust
                },
                DrumSound {
                    noise_level: 0.7,
                    noise_decay: 0.2,
                    noise_mode: 1,
                    noise_freq: 1000.0,
                    noise_q: 1.1,
                    bursts: 3,
                    gain: 0.8,
                    ..dust
                },
                DrumSound {
                    noise_level: 0.45,
                    noise_decay: 0.035,
                    noise_mode: 1,
                    noise_freq: 6500.0,
                    noise_q: 1.0,
                    metal_level: 0.12,
                    metal_decay: 0.03,
                    metal_freq: 260.0,
                    gain: 0.45,
                    choke: 1,
                    ..dust
                },
                DrumSound {
                    noise_level: 0.45,
                    noise_decay: 0.3,
                    noise_mode: 1,
                    noise_freq: 6000.0,
                    noise_q: 1.0,
                    metal_level: 0.12,
                    metal_decay: 0.3,
                    metal_freq: 260.0,
                    gain: 0.4,
                    choke: 1,
                    ..dust
                },
                DrumSound {
                    crush: 3,
                    bits: 10,
                    ..tom(85.0)
                },
                DrumSound {
                    crush: 3,
                    bits: 10,
                    ..tom(120.0)
                },
                DrumSound {
                    crush: 3,
                    bits: 10,
                    ..tom(165.0)
                },
                DrumSound {
                    noise_freq: 2200.0,
                    crush: 3,
                    bits: 10,
                    ..rim
                },
                DrumSound {
                    crush: 3,
                    bits: 10,
                    gain: 0.7,
                    ..cowbell
                },
                DrumSound {
                    noise_freq: 3500.0,
                    metal_freq: 300.0,
                    metal_decay: 0.9,
                    noise_decay: 0.8,
                    gain: 0.35,
                    crush: 3,
                    bits: 10,
                    ..crash
                },
                DrumSound {
                    gain: 0.35,
                    crush: 3,
                    bits: 10,
                    ..ride
                },
                DrumSound {
                    noise_freq: 5000.0,
                    noise_decay: 0.06,
                    gain: 0.4,
                    crush: 3,
                    bits: 10,
                    ..shaker
                },
                DrumSound {
                    tone_start: 70.0,
                    tone_end: 45.0,
                    pitch_decay: 0.05,
                    tone_level: 1.0,
                    tone_decay: 0.6,
                    drive: 0.5,
                    ..dust
                },
                DrumSound {
                    tone_start: 400.0,
                    tone_end: 330.0,
                    pitch_decay: 0.01,
                    tone_level: 0.5,
                    tone_decay: 0.05,
                    noise_level: 0.5,
                    noise_decay: 0.08,
                    noise_mode: 1,
                    noise_freq: 2500.0,
                    ..dust
                },
                DrumSound {
                    tone_start: 1800.0,
                    tone_end: 1800.0,
                    tone_level: 0.6,
                    tone_decay: 0.04,
                    gain: 0.6,
                    ..dust
                },
            ];
        }
        // Trap through a dusty sampler: a long, driven 808 kick, snappy
        // snare and clap, very short hats (rolls stay crisp) and a choked 808
        // boom, all lightly crushed for the lo-fi grain.
        DrumKitId::Trap => {
            let grit = DrumSound {
                crush: 2,
                bits: 12,
                ..d
            };
            let perc = |f: f32| DrumSound {
                tone_start: f * 1.6,
                tone_end: f,
                pitch_decay: 0.012,
                tone_level: 0.8,
                tone_decay: 0.11,
                click: 0.15,
                gain: 0.7,
                ..grit
            };
            s = [
                DrumSound {
                    tone_start: 190.0,
                    tone_end: 50.0,
                    pitch_decay: 0.03,
                    tone_level: 1.0,
                    tone_decay: 0.55,
                    click: 0.25,
                    drive: 0.4,
                    ..grit
                },
                DrumSound {
                    tone_start: 235.0,
                    tone_end: 190.0,
                    pitch_decay: 0.015,
                    tone_level: 0.55,
                    tone_decay: 0.08,
                    noise_level: 0.85,
                    noise_decay: 0.16,
                    noise_mode: 1,
                    noise_freq: 2800.0,
                    noise_q: 0.9,
                    drive: 0.25,
                    ..grit
                },
                DrumSound {
                    noise_level: 0.9,
                    noise_decay: 0.18,
                    noise_mode: 1,
                    noise_freq: 1400.0,
                    noise_q: 1.3,
                    bursts: 4,
                    gain: 0.85,
                    ..grit
                },
                DrumSound {
                    metal_level: 0.35,
                    metal_decay: 0.03,
                    metal_freq: 340.0,
                    noise_level: 0.5,
                    noise_decay: 0.028,
                    noise_mode: 2,
                    noise_freq: 8000.0,
                    choke: 1,
                    gain: 0.5,
                    ..grit
                },
                DrumSound {
                    metal_level: 0.35,
                    metal_decay: 0.35,
                    metal_freq: 340.0,
                    noise_level: 0.45,
                    noise_decay: 0.32,
                    noise_mode: 2,
                    noise_freq: 7500.0,
                    choke: 1,
                    gain: 0.42,
                    ..grit
                },
                perc(160.0),
                perc(240.0),
                perc(360.0),
                DrumSound {
                    crush: 2,
                    bits: 12,
                    ..rim
                },
                DrumSound {
                    noise_level: 0.8,
                    noise_decay: 0.05,
                    noise_mode: 1,
                    noise_freq: 2500.0,
                    noise_q: 2.0,
                    bursts: 2,
                    gain: 0.7,
                    ..grit
                },
                DrumSound {
                    noise_freq: 4000.0,
                    metal_decay: 1.0,
                    noise_decay: 0.9,
                    gain: 0.4,
                    crush: 2,
                    bits: 12,
                    ..crash
                },
                DrumSound {
                    gain: 0.38,
                    crush: 2,
                    bits: 12,
                    ..ride
                },
                DrumSound {
                    noise_decay: 0.05,
                    gain: 0.42,
                    crush: 2,
                    bits: 12,
                    ..shaker
                },
                DrumSound {
                    tone_start: 90.0,
                    tone_end: 41.0,
                    pitch_decay: 0.04,
                    tone_level: 1.0,
                    tone_decay: 1.6,
                    click: 0.05,
                    drive: 0.6,
                    choke: 2,
                    ..grit
                },
                DrumSound {
                    tone_start: 420.0,
                    tone_end: 380.0,
                    pitch_decay: 0.01,
                    tone_level: 0.45,
                    tone_decay: 0.04,
                    noise_level: 0.55,
                    noise_decay: 0.09,
                    noise_mode: 1,
                    noise_freq: 3000.0,
                    click: 0.4,
                    gain: 0.75,
                    ..grit
                },
                DrumSound {
                    metal_level: 0.15,
                    metal_decay: 0.012,
                    metal_freq: 380.0,
                    noise_level: 0.5,
                    noise_decay: 0.012,
                    noise_mode: 2,
                    noise_freq: 9000.0,
                    choke: 1,
                    gain: 0.45,
                    ..grit
                },
            ];
        }
        // Modular percussion: clean sine thumps and pinged "bongos" (a sine
        // with a tiny pitch drop, like a low-pass gate being struck).
        DrumKitId::Modular => {
            let ping = |f: f32, decay: f32| DrumSound {
                tone_start: f * 1.08,
                tone_end: f,
                pitch_decay: 0.015,
                tone_level: 0.9,
                tone_decay: decay,
                click: 0.05,
                ..d
            };
            s = [
                DrumSound {
                    tone_start: 140.0,
                    tone_end: 48.0,
                    pitch_decay: 0.04,
                    tone_level: 1.0,
                    tone_decay: 0.4,
                    click: 0.03,
                    drive: 0.1,
                    ..d
                },
                DrumSound {
                    tone_start: 210.0,
                    tone_end: 180.0,
                    pitch_decay: 0.02,
                    tone_level: 0.5,
                    tone_decay: 0.07,
                    noise_level: 0.55,
                    noise_decay: 0.12,
                    noise_mode: 1,
                    noise_freq: 2000.0,
                    noise_q: 1.6,
                    bits: 12,
                    ..d
                },
                DrumSound {
                    noise_level: 0.8,
                    noise_decay: 0.15,
                    noise_mode: 1,
                    noise_freq: 1300.0,
                    noise_q: 2.0,
                    bursts: 4,
                    gain: 0.75,
                    ..d
                },
                DrumSound {
                    noise_level: 0.5,
                    noise_decay: 0.025,
                    noise_mode: 1,
                    noise_freq: 8500.0,
                    noise_q: 2.5,
                    gain: 0.4,
                    choke: 1,
                    ..d
                },
                DrumSound {
                    noise_level: 0.5,
                    noise_decay: 0.22,
                    noise_mode: 1,
                    noise_freq: 8000.0,
                    noise_q: 2.5,
                    gain: 0.35,
                    choke: 1,
                    ..d
                },
                ping(200.0, 0.16),
                ping(280.0, 0.14),
                ping(380.0, 0.12),
                DrumSound {
                    tone_start: 1150.0,
                    tone_end: 1000.0,
                    pitch_decay: 0.01,
                    tone_level: 0.6,
                    tone_decay: 0.035,
                    click: 0.2,
                    gain: 0.6,
                    ..d
                },
                DrumSound {
                    metal_level: 0.3,
                    metal_freq: 500.0,
                    metal_decay: 0.12,
                    tone_start: 760.0,
                    tone_end: 760.0,
                    tone_level: 0.3,
                    tone_decay: 0.1,
                    gain: 0.6,
                    ..d
                },
                DrumSound {
                    noise_level: 0.35,
                    noise_decay: 1.2,
                    noise_mode: 1,
                    noise_freq: 3000.0,
                    noise_q: 0.5,
                    gain: 0.3,
                    ..d
                },
                DrumSound {
                    metal_level: 0.3,
                    metal_freq: 620.0,
                    metal_decay: 0.6,
                    gain: 0.3,
                    ..d
                },
                DrumSound {
                    noise_level: 0.45,
                    noise_decay: 0.05,
                    noise_mode: 1,
                    noise_freq: 9000.0,
                    noise_q: 1.5,
                    gain: 0.45,
                    ..d
                },
                DrumSound {
                    tone_start: 75.0,
                    tone_end: 55.0,
                    pitch_decay: 0.06,
                    tone_level: 1.0,
                    tone_decay: 0.9,
                    ..d
                },
                DrumSound {
                    noise_level: 0.6,
                    noise_decay: 0.1,
                    noise_mode: 1,
                    noise_freq: 1200.0,
                    noise_q: 3.0,
                    tone_start: 320.0,
                    tone_end: 300.0,
                    tone_level: 0.3,
                    tone_decay: 0.05,
                    ..d
                },
                ping(2400.0, 0.03),
            ];
        }
    }
    s
}

const METAL_RATIOS: [f32; 6] = [1.0, 1.4471, 1.6170, 1.9265, 2.5028, 2.6637];

struct DrumVoice {
    active: bool,
    pad: usize,
    snd: DrumSound,
    vel: f32,
    tone: Osc,
    tone_freq: f32,
    pitch_coef: f32,
    tone_env: Decay,
    noise_env: Decay,
    metal_env: Decay,
    click_env: Decay,
    metal: [Osc; 6],
    noise_filter: Svf,
    metal_hp: Svf,
    burst_timer: u32,
    crush_count: u8,
    crush_hold: f32,
    bursts_left: u8,
    choke_fade: f32,
    choke_group: u8,
}

pub struct DrumKit {
    sounds: [DrumSound; DRUM_PADS],
    /// Gain of each pad in the kit's own mix.
    parts: [f32; MAX_PARTS],
    voices: Vec<DrumVoice>,
    noise: Noise,
    sr: f32,
    next: usize,
}

impl DrumKit {
    pub fn new(kit: DrumKitId, sr: f32) -> Self {
        Self::with_sounds(kit_sounds(kit), sr)
    }

    /// A kit of designed pads (missing pads are silent).
    pub fn designed(pads: &[DrumPad], sr: f32) -> Self {
        let silent = DrumSound {
            gain: 0.0,
            ..Default::default()
        };
        Self::with_sounds(
            std::array::from_fn(|i| pads.get(i).map_or(silent, |p| DrumSound::from(*p))),
            sr,
        )
    }

    /// Replace one pad's sound live (the next hit uses it).
    pub fn set_pad(&mut self, pad: usize, sound: DrumPad) {
        if let Some(s) = self.sounds.get_mut(pad) {
            *s = sound.into();
        }
    }

    fn with_sounds(sounds: [DrumSound; DRUM_PADS], sr: f32) -> Self {
        let voices = (0..24)
            .map(|_| DrumVoice {
                active: false,
                pad: 0,
                snd: DrumSound::default(),
                vel: 0.0,
                tone: Osc::default(),
                tone_freq: 0.0,
                pitch_coef: 0.0,
                tone_env: Decay::default(),
                noise_env: Decay::default(),
                metal_env: Decay::default(),
                click_env: Decay::default(),
                metal: Default::default(),
                noise_filter: Svf::default(),
                metal_hp: Svf::default(),
                burst_timer: 0,
                crush_count: 0,
                crush_hold: 0.0,
                bursts_left: 0,
                choke_fade: 1.0,
                choke_group: 0,
            })
            .collect();
        DrumKit {
            sounds,
            parts: [1.0; MAX_PARTS],
            voices,
            noise: Noise::new(98765),
            sr,
            next: 0,
        }
    }

    pub fn note_on(&mut self, pitch: u8, velocity: f32) {
        let pad = (pitch as i32 - DRUM_BASE_NOTE as i32).rem_euclid(DRUM_PADS as i32) as usize;
        let snd = self.sounds[pad];
        if snd.choke != 0 {
            for v in self
                .voices
                .iter_mut()
                .filter(|v| v.active && v.choke_group == snd.choke)
            {
                v.choke_fade = 0.995; // start fast fade
            }
        }
        let idx = self
            .voices
            .iter()
            .position(|v| !v.active)
            .unwrap_or_else(|| {
                self.next = (self.next + 1) % self.voices.len();
                self.next
            });
        let sr = self.sr;
        // every hit slightly different: pitch, decay and noise colour
        let (r1, r2, r3) = (self.noise.next(), self.noise.next(), self.noise.next());
        let mut snd = snd;
        snd.tone_start *= 1.0 + 0.015 * r1;
        snd.tone_end *= 1.0 + 0.015 * r1;
        snd.tone_decay *= 1.0 + 0.08 * r2;
        snd.noise_decay *= 1.0 + 0.08 * r2;
        snd.noise_freq *= 1.0 + 0.06 * r3;
        let v = &mut self.voices[idx];
        v.active = true;
        v.pad = pad;
        v.snd = snd;
        v.vel = velocity;
        v.tone.phase = 0.0;
        v.tone_freq = snd.tone_start;
        v.pitch_coef = if snd.pitch_decay > 0.0 {
            (-1.0 / (snd.pitch_decay * sr)).exp()
        } else {
            0.0
        };
        v.tone_env.trigger(snd.tone_level, snd.tone_decay, sr);
        v.noise_env.trigger(snd.noise_level, snd.noise_decay, sr);
        v.metal_env.trigger(snd.metal_level, snd.metal_decay, sr);
        v.click_env.trigger(snd.click, 0.004, sr);
        let q = snd.noise_q.max(0.3);
        v.noise_filter.reset();
        v.noise_filter.set(snd.noise_freq.max(40.0), q, sr);
        v.metal_hp.reset();
        v.metal_hp.set(6000.0, 0.7, sr);
        v.burst_timer = (0.011 * sr) as u32;
        v.bursts_left = snd.bursts.saturating_sub(1);
        v.choke_fade = 1.0;
        v.choke_group = snd.choke;
    }

    fn render(&mut self, out: &mut [f32]) {
        let sr = self.sr;
        for v in self.voices.iter_mut().filter(|v| v.active) {
            let s = v.snd;
            let part = self.parts[v.pad];
            let mut fade = 1.0f32;
            for o in out.iter_mut() {
                // pitch sweep
                v.tone_freq = s.tone_end + (v.tone_freq - s.tone_end) * v.pitch_coef;
                let mut x = 0.0;
                if s.tone_level > 0.0 {
                    x += v.tone.sine(v.tone_freq, sr) * v.tone_env.next();
                }
                if s.noise_level > 0.0 {
                    if s.bursts > 1 && v.bursts_left > 0 {
                        v.burst_timer = v.burst_timer.saturating_sub(1);
                        if v.burst_timer == 0 {
                            v.bursts_left -= 1;
                            v.noise_env.trigger(
                                s.noise_level,
                                s.noise_decay * if v.bursts_left == 0 { 1.0 } else { 0.08 },
                                sr,
                            );
                            v.burst_timer = (0.011 * sr) as u32;
                        }
                    }
                    let n = self.noise.next();
                    let f = v.noise_filter.process(n);
                    let y = match s.noise_mode {
                        0 => f.low,
                        1 => f.band,
                        _ => f.high,
                    };
                    x += y * v.noise_env.next();
                }
                if s.metal_level > 0.0 {
                    let mut m = 0.0;
                    for (osc, r) in v.metal.iter_mut().zip(METAL_RATIOS) {
                        m += osc.square(s.metal_freq * r, sr);
                    }
                    x += v.metal_hp.process(m * 0.25).high * v.metal_env.next();
                }
                if s.click > 0.0 {
                    x += self.noise.next() * v.click_env.next();
                }
                if s.drive > 0.0 {
                    let g = 1.0 + s.drive * 6.0;
                    x = (x * g).tanh() / g.sqrt();
                }
                if s.bits > 0 {
                    let steps = (1u32 << (s.bits - 1)) as f32;
                    x = (x * steps).round() / steps;
                }
                if s.crush > 1 {
                    if v.crush_count == 0 {
                        v.crush_hold = x;
                    }
                    v.crush_count = (v.crush_count + 1) % s.crush;
                    x = v.crush_hold;
                }
                if v.choke_fade < 1.0 {
                    fade *= v.choke_fade;
                }
                *o += x * v.vel * s.gain * fade * 0.8 * part;
            }
            if v.choke_fade < 1.0 {
                v.choke_fade = fade;
            }
            let level = v
                .tone_env
                .value
                .max(v.noise_env.value)
                .max(v.metal_env.value)
                .max(v.click_env.value);
            if (level < 1e-4 && v.bursts_left == 0) || fade < 1e-3 {
                v.active = false;
            }
        }
    }
}

// ---------------------------------------------------------------- Sampler

struct SamplerVoice {
    pitch: u8,
    /// Pad of a kit (its slot in the part mix); 0 when melodic.
    pad: usize,
    pos: f64,
    rate: f64,
    start: f64,
    end: f64,
    vel: f32,
    env: Adsr,
}

pub struct Sampler {
    sample: Arc<SampleData>,
    /// None = melodic (root note), Some = kit slices.
    slices: Option<Vec<(u32, u32)>>,
    root: u8,
    voices: Vec<SamplerVoice>,
    sr: f32,
    /// Gain of each pad when the sample is a kit.
    parts: [f32; MAX_PARTS],
}

impl Sampler {
    pub fn melodic(sample: Arc<SampleData>, root: u8, sr: f32) -> Self {
        Self::make(sample, None, root, sr)
    }

    pub fn kit(sample: Arc<SampleData>, slices: Vec<(u32, u32)>, sr: f32) -> Self {
        Self::make(sample, Some(slices), 60, sr)
    }

    fn make(sample: Arc<SampleData>, slices: Option<Vec<(u32, u32)>>, root: u8, sr: f32) -> Self {
        let voices = (0..16)
            .map(|_| SamplerVoice {
                pitch: 0,
                pad: 0,
                pos: 0.0,
                rate: 1.0,
                start: 0.0,
                end: 0.0,
                vel: 0.0,
                env: Adsr::new(0.002, 0.1, 1.0, 0.08, sr),
            })
            .collect();
        Sampler {
            sample,
            slices,
            root,
            voices,
            sr,
            parts: [1.0; MAX_PARTS],
        }
    }

    pub fn note_on(&mut self, pitch: u8, velocity: f32) {
        let len = self.sample.frames.len() as f64;
        let base_rate = self.sample.sample_rate as f64 / self.sr as f64;
        let pad = (pitch as i32 - DRUM_BASE_NOTE as i32).rem_euclid(DRUM_PADS as i32) as usize;
        let (start, end, rate) = match &self.slices {
            Some(slices) if !slices.is_empty() => {
                // pads beyond the number of slices replay slices pitched up/down
                let (s, e) = slices[pad % slices.len()];
                const SHIFTS: [f64; 4] = [0.0, 7.0, -5.0, 12.0];
                let semis = SHIFTS[(pad / slices.len()) % SHIFTS.len()];
                (
                    s as f64,
                    (e as f64).min(len),
                    base_rate * 2f64.powf(semis / 12.0),
                )
            }
            _ => (
                0.0,
                len,
                base_rate * 2f64.powf((pitch as f64 - self.root as f64) / 12.0),
            ),
        };
        let idx = self
            .voices
            .iter()
            .position(|v| !v.env.is_active())
            .unwrap_or(0);
        let kit = self.slices.is_some();
        let v = &mut self.voices[idx];
        v.pitch = pitch;
        v.pad = if kit { pad } else { 0 };
        v.pos = start;
        v.start = start;
        v.end = end;
        v.rate = rate;
        v.vel = velocity;
        v.env.gate_on();
    }

    pub fn note_off(&mut self, pitch: u8) {
        if self.slices.is_some() {
            return; // one-shot pads
        }
        for v in self.voices.iter_mut().filter(|v| v.pitch == pitch) {
            v.env.gate_off();
        }
    }

    fn render(&mut self, out: &mut [f32]) {
        let frames = &self.sample.frames;
        let kit = self.slices.is_some();
        for v in self.voices.iter_mut().filter(|v| v.env.is_active()) {
            let part = if kit { self.parts[v.pad] } else { 1.0 };
            for o in out.iter_mut() {
                if v.pos >= v.end - 1.0 {
                    // samples are faded out when recorded, so a hard stop is clean
                    v.env.kill();
                    break;
                }
                let i = v.pos as usize;
                let frac = (v.pos - i as f64) as f32;
                let a = frames[i];
                let b = frames[(i + 1).min(frames.len() - 1)];
                *o += (a + (b - a) * frac) * v.env.next() * v.vel * part;
                v.pos += v.rate;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(buf: &[f32]) -> f32 {
        (buf.iter().map(|x| x * x).sum::<f32>() / buf.len() as f32).sqrt()
    }

    #[test]
    fn every_kit_pad_makes_sound_and_stops() {
        for kit in DrumKitId::ALL {
            for pad in 0..DRUM_PADS as u8 {
                let mut k = DrumKit::new(kit, 48000.0);
                k.note_on(DRUM_BASE_NOTE + pad, 1.0);
                let mut buf = vec![0.0; 4800];
                k.render(&mut buf);
                assert!(rms(&buf) > 0.001, "kit {kit:?} pad {pad} silent");
                assert!(buf.iter().all(|x| x.is_finite() && x.abs() < 4.0));
                let mut tail = vec![0.0; 48000 * 4];
                k.render(&mut tail);
                assert!(
                    k.voices.iter().all(|v| !v.active),
                    "kit {kit:?} pad {pad} never ends"
                );
            }
        }
    }

    #[test]
    fn synth_presets_sound_and_release() {
        for preset in SynthPreset::ALL {
            let mut s = Synth::new(preset, 48000.0);
            s.note_on(48, 1.0);
            let mut buf = vec![0.0; 9600];
            s.render(&mut buf);
            assert!(rms(&buf) > 0.005, "{preset:?} silent");
            s.note_off(48);
            let mut tail = vec![0.0; 48000 * 4];
            s.render(&mut tail);
            assert!(
                s.voices.iter().all(|v| !v.amp.is_active()),
                "{preset:?} stuck"
            );
        }
    }

    #[test]
    fn part_gains_mix_pads_and_layers() {
        let mut gains = [1.0; MAX_PARTS];
        gains[1] = 0.0;
        let mut kit = Instrument::build(
            &InstrumentSpec::Drums {
                kit: DrumKitId::Trap,
            },
            None,
            48000.0,
        );
        kit.set_part_gains(&gains);
        let mut buf = vec![0.0; 4800];
        kit.note_on(DRUM_BASE_NOTE + 1, 1.0); // pad 1 (snare) is silenced
        kit.render(&mut buf);
        assert_eq!(rms(&buf), 0.0);
        kit.note_on(DRUM_BASE_NOTE, 1.0); // the kick still plays
        kit.render(&mut buf);
        assert!(rms(&buf) > 0.001);

        // round bass: triangle (1), saw (2) and sub (4); keep only the sub
        let bass = InstrumentSpec::Synth {
            preset: SynthPreset::Bass,
        };
        let names: Vec<usize> = part_names(&bass).iter().map(|(i, _)| *i).collect();
        assert_eq!(names, vec![1, 2, 4]);
        let mut full = Instrument::build(&bass, None, 48000.0);
        let mut sub_only = Instrument::build(&bass, None, 48000.0);
        let mut g = [0.0; MAX_PARTS];
        g[4] = 1.0;
        sub_only.set_part_gains(&g);
        let (mut a, mut b) = (vec![0.0; 9600], vec![0.0; 9600]);
        full.note_on(40, 1.0);
        sub_only.note_on(40, 1.0);
        full.render(&mut a);
        sub_only.render(&mut b);
        assert!(rms(&b) > 0.001 && rms(&b) < rms(&a));

        // one-layer presets have nothing to mix on their own
        assert!(part_names(&InstrumentSpec::Synth {
            preset: SynthPreset::Bass808
        })
        .is_empty());
    }

    #[test]
    fn sampler_pitches_up_an_octave() {
        let frames: Vec<f32> = (0..48000).map(|i| ((i as f32) * 0.05).sin()).collect();
        let sample = Arc::new(SampleData {
            frames,
            sample_rate: 48000,
        });
        let mut s = Sampler::melodic(sample, 60, 48000.0);
        s.note_on(72, 1.0);
        let mut buf = vec![0.0; 48000];
        s.render(&mut buf);
        // the sample lasts half as long one octave up
        let tail = rms(&buf[25000..]);
        assert!(tail < 1e-3, "tail {tail}");
        assert!(rms(&buf[1000..20000]) > 0.1);
    }
}
