//! Modal synthesis instruments: "Felt piano" and "Soft pluck".
//!
//! Each note is a bank of damped sinusoidal resonators (the string's
//! partials), computed with a rotating phasor, so it costs a couple of
//! multiplications per partial per sample. What makes it sound like a soft,
//! warm, lo-fi piano rather than a synth:
//! - slightly inharmonic partials (stiff strings),
//! - two strings per note, a hair apart in tuning, that beat against each
//!   other and decay at different rates (prompt sound + aftersound),
//! - higher partials die faster than the fundamental,
//! - a soft felt hammer: few high partials, more of them the harder you play,
//! - a muffled hammer "thump" and a slow, dark attack,
//! - per-note random detune and a gentle low-pass.
//!
//! The same engine, with a different `ModalProfile`, gives a soft plucked
//! string (kalimba / nylon feel): mid-string pluck, nearly harmonic
//! partials, fast-dying highs and a soft fingertip attack.
//!
//! No samples: everything here is generated, so it is entirely our own sound.

use crate::dsp::{midi_to_hz, Noise};

/// Shape of a modal instrument. Times in seconds, frequencies in Hz.
#[derive(Clone, Copy, Debug)]
pub struct ModalProfile {
    /// Inharmonicity coefficient at middle C (stiffness of the string).
    pub inharmonicity: f32,
    /// Ring time (to -60 dB) of the fundamental at C2 and the clamp range.
    pub ring_c2: f32,
    pub ring_min: f32,
    pub ring_max: f32,
    /// How much faster each higher partial dies (0 = all equal).
    pub decay_tilt: f32,
    /// Excitation point along the string (1/7 for a piano hammer, ~1/4 for a pluck).
    pub strike: f32,
    /// Highest partial frequency for the softest and the hardest touch.
    pub bright_soft: f32,
    pub bright_hard: f32,
    /// Attack time for the hardest and the softest touch.
    pub attack_hard: f32,
    pub attack_soft: f32,
    /// Level of the muffled excitation noise (hammer thump / finger).
    pub thump: f32,
    pub thump_hz: f32,
    /// Tuning offset between the two strings, in cents (beating / chorus).
    pub string_cents: f32,
    /// Damping time after note-off.
    pub release: f32,
    pub lowpass_hz: f32,
    pub gain: f32,
}

impl ModalProfile {
    /// Warm felt upright: long ring, stiff strings, dark hammer.
    pub const FELT_PIANO: ModalProfile = ModalProfile {
        inharmonicity: 0.00018,
        ring_c2: 6.5,
        ring_min: 1.0,
        ring_max: 7.5,
        decay_tilt: 0.5,
        strike: 1.0 / 7.3,
        bright_soft: 650.0,
        bright_hard: 3950.0,
        attack_hard: 0.0015,
        attack_soft: 0.0055,
        thump: 0.12,
        thump_hz: 450.0,
        string_cents: 0.7,
        release: 0.22,
        lowpass_hz: 3600.0,
        gain: 0.8,
    };

    /// Soft plucked string: short ring, nearly harmonic, highs fade fast.
    pub const SOFT_PLUCK: ModalProfile = ModalProfile {
        inharmonicity: 0.00004,
        ring_c2: 2.6,
        ring_min: 0.5,
        ring_max: 2.6,
        decay_tilt: 1.1,
        strike: 1.0 / 4.3,
        bright_soft: 700.0,
        bright_hard: 3000.0,
        attack_hard: 0.002,
        attack_soft: 0.006,
        thump: 0.05,
        thump_hz: 1200.0,
        string_cents: 1.6,
        release: 0.7, // a plucked string keeps ringing after the finger leaves
        lowpass_hz: 3000.0,
        gain: 0.9,
    };
}

const MAX_PARTIALS: usize = 16;
const STRINGS: usize = 2;
const VOICES: usize = 12;

#[derive(Clone, Copy, Default)]
struct Resonator {
    re: f32,
    im: f32,
    /// rotation (decay * cos w, decay * sin w)
    cr: f32,
    ci: f32,
    /// angular step, kept to re-damp the string on note-off
    w: f32,
}

struct PianoVoice {
    active: bool,
    pitch: u8,
    age: u64,
    res: [Resonator; MAX_PARTIALS * STRINGS],
    n: usize,
    /// felt-hammer attack: output rises over a few ms
    attack: f32,
    attack_coef: f32,
    thump: f32,
    thump_lp: f32,
    gain: f32,
    released: bool,
}

/// Polyphonic modal instrument (felt piano, soft pluck…).
pub struct FeltPiano {
    profile: ModalProfile,
    voices: Vec<PianoVoice>,
    noise: Noise,
    sr: f32,
    clock: u64,
    lp: f32,
    lp_coef: f32,
    /// Mix of the two layers: the strings and the hammer (or finger) noise.
    strings: f32,
    thump: f32,
}

impl FeltPiano {
    pub fn new(sr: f32) -> Self {
        Self::with_profile(ModalProfile::FELT_PIANO, sr)
    }

    pub fn with_profile(profile: ModalProfile, sr: f32) -> Self {
        let voices = (0..VOICES)
            .map(|_| PianoVoice {
                active: false,
                pitch: 0,
                age: 0,
                res: [Resonator::default(); MAX_PARTIALS * STRINGS],
                n: 0,
                attack: 0.0,
                attack_coef: 0.0,
                thump: 0.0,
                thump_lp: 0.0,
                gain: 0.0,
                released: false,
            })
            .collect();
        FeltPiano {
            profile,
            voices,
            noise: Noise::new(2024),
            sr,
            clock: 0,
            lp: 0.0,
            // warm, slightly "taped" top end
            lp_coef: 1.0 - (-std::f32::consts::TAU * profile.lowpass_hz / sr).exp(),
            strings: 1.0,
            thump: 1.0,
        }
    }

    pub fn note_on(&mut self, pitch: u8, velocity: f32) {
        self.clock += 1;
        let sr = self.sr;
        let vel = velocity.clamp(0.05, 1.0);
        let detune = 2f32.powf(self.noise.next() * 3.0 / 1200.0);
        let idx = self
            .voices
            .iter()
            .position(|v| v.active && v.pitch == pitch)
            .or_else(|| self.voices.iter().position(|v| !v.active))
            .unwrap_or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, v)| v.age)
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            });
        let pr = self.profile;
        let rel = pitch as f32 - 60.0;
        let f0 = midi_to_hz(pitch as f32) * detune;
        // string stiffness: higher notes are more inharmonic
        let b = (pr.inharmonicity * 2f32.powf(rel / 18.0)).clamp(0.000005, 0.004);
        // felt: soft hits keep only the low partials
        let hardness = 0.25 + 0.75 * vel;
        let max_freq =
            (pr.bright_soft + (pr.bright_hard - pr.bright_soft) * vel.powf(1.5)).min(sr * 0.45);
        // low notes ring longer
        let t60_base =
            (pr.ring_c2 * 2f32.powf(-(pitch as f32 - 36.0) / 26.0)).clamp(pr.ring_min, pr.ring_max);
        let strike = pr.strike;
        let v = &mut self.voices[idx];
        let mut n = 0;
        let mut norm = 0.0;
        for k in 1..=MAX_PARTIALS {
            let kf = k as f32;
            let fk = kf * f0 * (1.0 + b * kf * kf).sqrt();
            if fk > max_freq {
                break;
            }
            let shape = (std::f32::consts::PI * kf * strike).sin().abs();
            let amp = kf.powf(-(1.9 - 0.9 * hardness)) * (0.35 + 0.65 * shape);
            let t60 = t60_base / (1.0 + pr.decay_tilt * (kf - 1.0));
            for s in 0..STRINGS {
                // string 0: louder, shorter (prompt sound); string 1: quieter, longer (aftersound)
                let (cents, a, t) = if s == 0 {
                    (-pr.string_cents, 0.62, t60 * 0.75)
                } else {
                    (pr.string_cents, 0.38, t60 * 1.7)
                };
                let w = std::f32::consts::TAU * fk * 2f32.powf(cents / 1200.0) / sr;
                let r = (-6.9 / (t * sr)).exp();
                v.res[n] = Resonator {
                    re: amp * a,
                    im: 0.0,
                    cr: r * w.cos(),
                    ci: r * w.sin(),
                    w,
                };
                norm += amp * a;
                n += 1;
            }
        }
        v.n = n;
        v.active = true;
        v.pitch = pitch;
        v.age = self.clock;
        v.released = false;
        v.attack = 0.0;
        // softer hits have a slower, rounder attack
        let attack_time = pr.attack_hard + (pr.attack_soft - pr.attack_hard) * (1.0 - vel);
        v.attack_coef = (-1.0 / (attack_time * sr)).exp();
        v.thump = pr.thump * vel;
        v.thump_lp = 0.0;
        v.gain = pr.gain * vel.powf(1.2) / norm.max(0.5);
    }

    pub fn note_off(&mut self, pitch: u8) {
        let damp = (-6.9 / (self.profile.release * self.sr)).exp();
        for v in self
            .voices
            .iter_mut()
            .filter(|v| v.active && v.pitch == pitch && !v.released)
        {
            v.released = true;
            for r in v.res[..v.n].iter_mut() {
                r.cr = damp * r.w.cos();
                r.ci = damp * r.w.sin();
            }
        }
    }

    pub fn all_notes_off(&mut self) {
        for i in 0..self.voices.len() {
            let p = self.voices[i].pitch;
            self.note_off(p);
        }
    }

    pub fn set_part_gains(&mut self, strings: f32, thump: f32) {
        self.strings = strings;
        self.thump = thump;
    }

    pub fn render(&mut self, out: &mut [f32]) {
        let (strings, thump) = (self.strings, self.thump);
        let thump_decay = (-1.0 / (0.012 * self.sr)).exp();
        let thump_lp = 1.0 - (-std::f32::consts::TAU * self.profile.thump_hz / self.sr).exp();
        for o in out.iter_mut() {
            let mut acc = 0.0;
            for v in self.voices.iter_mut().filter(|v| v.active) {
                let mut s = 0.0;
                for r in v.res[..v.n].iter_mut() {
                    let re = r.re * r.cr - r.im * r.ci;
                    let im = r.re * r.ci + r.im * r.cr;
                    r.re = re;
                    r.im = im;
                    s += im;
                }
                v.attack = 1.0 + (v.attack - 1.0) * v.attack_coef;
                // muffled hammer thump
                let n = self.noise.next() * v.thump;
                v.thump *= thump_decay;
                v.thump_lp += thump_lp * (n - v.thump_lp);
                acc += (s * v.attack * strings + v.thump_lp * thump) * v.gain;
            }
            self.lp += self.lp_coef * (acc - self.lp);
            *o += self.lp;
        }
        // retire silent voices (checked once per block)
        for v in self.voices.iter_mut().filter(|v| v.active) {
            let energy: f32 = v.res[..v.n].iter().map(|r| r.re.abs() + r.im.abs()).sum();
            if energy * v.gain < 1e-5 {
                v.active = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(b: &[f32]) -> f32 {
        (b.iter().map(|x| x * x).sum::<f32>() / b.len() as f32).sqrt()
    }

    #[test]
    fn plays_across_the_keyboard_and_decays() {
        let mut p = FeltPiano::new(48000.0);
        for pitch in [36u8, 48, 55, 60, 64, 67, 72, 84, 96] {
            p.note_on(pitch, 0.8);
        }
        let mut buf = vec![0.0; 48000];
        p.render(&mut buf);
        assert!(rms(&buf) > 0.01, "rms {}", rms(&buf));
        assert!(buf.iter().all(|x| x.is_finite() && x.abs() < 3.0));
        p.all_notes_off();
        let mut tail = vec![0.0; 48000 * 2];
        p.render(&mut tail);
        assert!(p.voices.iter().all(|v| !v.active));
    }

    #[test]
    fn soft_pluck_is_shorter_than_piano() {
        let tail = |profile: ModalProfile| {
            let mut p = FeltPiano::with_profile(profile, 48000.0);
            p.note_on(67, 0.8);
            let mut buf = vec![0.0; 48000 * 2];
            p.render(&mut buf);
            rms(&buf[48000 + 24000..]) / rms(&buf[..24000])
        };
        assert!(tail(ModalProfile::SOFT_PLUCK) < tail(ModalProfile::FELT_PIANO) * 0.5);
    }

    #[test]
    fn soft_notes_are_darker_than_hard_ones() {
        let brightness = |vel: f32| {
            let mut p = FeltPiano::new(48000.0);
            p.note_on(60, vel);
            let mut buf = vec![0.0; 24000];
            p.render(&mut buf);
            let diff: f32 = buf
                .windows(2)
                .map(|w| (w[1] - w[0]).powi(2))
                .sum::<f32>()
                .sqrt();
            diff / buf.iter().map(|x| x * x).sum::<f32>().sqrt()
        };
        assert!(brightness(0.2) < brightness(1.0));
    }
}
