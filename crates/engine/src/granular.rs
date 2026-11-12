//! Granular texture module: sprays short overlapping windowed grains read
//! from a source buffer. Each held note is a "stream" that slowly scans the
//! source, so chords become drifting ambient clouds.

use std::sync::Arc;

use crate::dsp::{Adsr, Noise};
use crate::instruments::SampleData;
use crate::model::TextureKind;

const MAX_GRAINS: usize = 96;
const MAX_STREAMS: usize = 10;

#[derive(Clone, Copy)]
struct GrainParams {
    /// Grain length in seconds.
    size: f32,
    /// Grains per second per held note.
    density: f32,
    /// Random spread of the read position (fraction of the source).
    spread: f32,
    /// How fast the read position scans the source (1 = real time).
    scan: f32,
    /// Random detune per grain, in cents.
    jitter_cents: f32,
    gain: f32,
    /// Seconds for a held note to swell in and to die away.
    attack: f32,
    release: f32,
}

fn params_for(texture: TextureKind, from_sample: bool) -> GrainParams {
    if from_sample {
        return GrainParams {
            size: 0.1,
            density: 20.0,
            spread: 0.15,
            scan: 0.25,
            jitter_cents: 8.0,
            gain: 0.5,
            attack: 0.6,
            release: 2.2,
        };
    }
    match texture {
        // a passage of real pads: long grains, a slow walk through it, a
        // little detune so held notes breathe
        // one voice almost alone: short grains close together and little
        // wander, so it speaks like an instrument and answers the key
        TextureKind::ShadowLead | TextureKind::SilentLead => GrainParams {
            size: 0.14,
            density: 18.0,
            spread: 0.1,
            scan: 0.2,
            jitter_cents: 3.0,
            gain: 0.6,
            attack: 0.04,
            release: 0.6,
        },
        TextureKind::ShadowAir | TextureKind::SilentAir | TextureKind::SessionAir => GrainParams {
            size: 0.28,
            density: 12.0,
            spread: 0.3,
            scan: 0.12,
            jitter_cents: 6.0,
            gain: 0.55,
            attack: 0.6,
            release: 2.2,
        },
        TextureKind::Cloud => GrainParams {
            size: 0.18,
            density: 14.0,
            spread: 0.15,
            scan: 0.2,
            jitter_cents: 7.0,
            gain: 0.4,
            attack: 0.6,
            release: 2.2,
        },
        TextureKind::Glass => GrainParams {
            size: 0.12,
            density: 16.0,
            spread: 0.4,
            scan: 0.3,
            jitter_cents: 5.0,
            gain: 0.4,
            attack: 0.6,
            release: 2.2,
        },
    }
}

#[derive(Clone, Copy, Default)]
struct Grain {
    active: bool,
    pos: f64,
    rate: f64,
    age: u32,
    len: u32,
    amp: f32,
}

struct Stream {
    pitch: u8,
    vel: f32,
    env: Adsr,
    pos: f64,
    countdown: f32,
}

pub struct Granular {
    source: Arc<SampleData>,
    /// MIDI note at which the source plays at its own pitch.
    root: f32,
    params: GrainParams,
    streams: Vec<Stream>,
    grains: [Grain; MAX_GRAINS],
    noise: Noise,
    sr: f32,
}

impl Granular {
    pub fn new(
        source: Arc<SampleData>,
        root: u8,
        texture: TextureKind,
        from_sample: bool,
        sr: f32,
    ) -> Self {
        let params = params_for(texture, from_sample);
        let streams = (0..MAX_STREAMS)
            .map(|_| Stream {
                pitch: 0,
                vel: 0.0,
                env: Adsr::new(params.attack, 0.2, 1.0, params.release, sr),
                pos: 0.0,
                countdown: 0.0,
            })
            .collect();
        Granular {
            source,
            root: root as f32,
            params,
            streams,
            grains: [Grain::default(); MAX_GRAINS],
            noise: Noise::new(777),
            sr,
        }
    }

    /// Generate a built-in texture source at `sr`. Runs on the control thread.
    pub fn texture_source(texture: TextureKind, sr: f32) -> (Arc<SampleData>, u8) {
        if let Some((name, root)) = texture.sampled() {
            return (crate::library::tone(name), root);
        }
        let (frames, root) = match texture {
            TextureKind::Glass => (glass(sr), 72),
            _ => (cloud(sr), 60),
        };
        (
            Arc::new(SampleData {
                frames: normalize(frames),
                sample_rate: sr as u32,
            }),
            root,
        )
    }

    fn rand01(&mut self) -> f32 {
        self.noise.next() * 0.5 + 0.5
    }

    pub fn note_on(&mut self, pitch: u8, velocity: f32) {
        let idx = self
            .streams
            .iter()
            .position(|s| s.pitch == pitch && s.env.is_active())
            .or_else(|| self.streams.iter().position(|s| !s.env.is_active()))
            .unwrap_or(0);
        let start = self.rand01() as f64 * self.source.frames.len() as f64;
        let s = &mut self.streams[idx];
        s.pitch = pitch;
        s.vel = velocity;
        s.env.gate_on();
        s.pos = start;
        s.countdown = 0.0;
    }

    pub fn note_off(&mut self, pitch: u8) {
        for s in self.streams.iter_mut().filter(|s| s.pitch == pitch) {
            s.env.gate_off();
        }
    }

    pub fn all_notes_off(&mut self) {
        self.streams.iter_mut().for_each(|s| s.env.gate_off());
    }

    fn spawn(&mut self, stream: usize, amp: f32) {
        let Some(slot) = self.grains.iter().position(|g| !g.active) else {
            return;
        };
        let p = self.params;
        let len_src = self.source.frames.len() as f64;
        let spread = (self.rand01() as f64 - 0.5) * p.spread as f64 * len_src;
        let jitter = (self.rand01() - 0.5) * 2.0 * p.jitter_cents;
        let size = p.size * (0.75 + 0.5 * self.rand01());
        let (pitch, pos) = (self.streams[stream].pitch, self.streams[stream].pos);
        let semis = pitch as f32 - self.root + jitter / 100.0;
        let rate = self.source.sample_rate as f64 / self.sr as f64 * 2f64.powf(semis as f64 / 12.0);
        self.grains[slot] = Grain {
            active: true,
            pos: (pos + spread).rem_euclid(len_src),
            rate,
            age: 0,
            len: (size * self.sr) as u32,
            amp,
        };
    }

    pub fn render(&mut self, out: &mut [f32]) {
        // nothing held and nothing still ringing: nothing to do
        if !self.streams.iter().any(|s| s.env.is_active()) && !self.grains.iter().any(|g| g.active)
        {
            return;
        }
        let sine = crate::dsp::sine_table();
        let p = self.params;
        let interval = self.sr / p.density;
        let scan = p.scan as f64 * self.source.sample_rate as f64 / self.sr as f64;
        let len_src = self.source.frames.len() as f64;
        for n in 0..out.len() {
            // streams: advance envelopes and spawn grains
            for i in 0..self.streams.len() {
                if !self.streams[i].env.is_active() {
                    continue;
                }
                let level = self.streams[i].env.next();
                let s = &mut self.streams[i];
                s.pos = (s.pos + scan) % len_src;
                s.countdown -= 1.0;
                if s.countdown <= 0.0 {
                    let amp = level * s.vel;
                    let jitter = self.noise.next() * 0.3;
                    self.streams[i].countdown = interval * (1.0 + jitter);
                    self.spawn(i, amp);
                }
            }
            // grains
            let frames = &self.source.frames;
            let mut acc = 0.0;
            for g in self.grains.iter_mut().filter(|g| g.active) {
                // half a turn of a sine, squared: the grain's window
                let w = crate::dsp::sin_turns(sine, 0.5 * g.age as f32 / g.len as f32);
                let i = g.pos as usize;
                let frac = (g.pos - i as f64) as f32;
                let last = frames.len() - 1;
                let a = frames[i.min(last)];
                let b = frames[if i < last { i + 1 } else { 0 }];
                acc += (a + (b - a) * frac) * w * w * g.amp;
                g.pos += g.rate;
                if g.pos >= len_src {
                    g.pos -= len_src;
                }
                g.age += 1;
                if g.age >= g.len {
                    g.active = false;
                }
            }
            out[n] += acc * p.gain;
        }
    }
}

// ---------------------------------------------------------- built-in sources

const SECONDS: f32 = 4.0;

fn normalize(mut v: Vec<f32>) -> Vec<f32> {
    let peak = v.iter().fold(0.0f32, |m, x| m.max(x.abs())).max(1e-6);
    v.iter_mut().for_each(|x| *x *= 0.8 / peak);
    v
}

/// Soft pitched cloud at C4: detuned saw stack + sub, low-passed, swelling.
fn cloud(sr: f32) -> Vec<f32> {
    let n = (SECONDS * sr) as usize;
    let f0 = 261.63f32;
    let detunes = [-12.0f32, -5.0, 0.0, 6.0, 11.0];
    let mut phases = [0.0f32; 5];
    let mut sub = 0.0f32;
    let mut lp = 0.0f32;
    let mut lp2 = 0.0f32;
    let a = 1.0 - (-std::f32::consts::TAU * 1200.0 / sr).exp();
    (0..n)
        .map(|i| {
            let t = i as f32 / sr;
            let mut x = 0.0;
            for (ph, d) in phases.iter_mut().zip(detunes) {
                *ph = (*ph + f0 * 2f32.powf(d / 1200.0) / sr) % 1.0;
                x += 2.0 * *ph - 1.0;
            }
            sub = (sub + f0 * 0.5 / sr) % 1.0;
            x = x * 0.2 + (std::f32::consts::TAU * sub).sin() * 0.4;
            lp += a * (x - lp);
            lp2 += a * (lp - lp2);
            let swell = 0.6 + 0.4 * (std::f32::consts::TAU * 0.25 * t).sin();
            lp2 * swell
        })
        .collect()
}

/// Glass: a few FM bell strikes (C5, G5, E6) ringing into each other.
fn glass(sr: f32) -> Vec<f32> {
    let n = (SECONDS * sr) as usize;
    let mut out = vec![0.0f32; n];
    let strikes = [
        (0.0f32, 523.25f32),
        (0.5, 783.99),
        (1.0, 1318.5),
        (1.75, 523.25),
        (2.5, 783.99),
        (3.1, 1046.5),
    ];
    for (at, f) in strikes {
        let start = (at * sr) as usize;
        for (k, o) in out[start..].iter_mut().enumerate() {
            let t = k as f32 / sr;
            let env = (-t * 1.6).exp();
            let index = 2.5 * (-t * 6.0).exp() + 0.3;
            let m = (std::f32::consts::TAU * f * 3.5 * t).sin() * index;
            *o += (std::f32::consts::TAU * f * t + m).sin() * env * 0.5;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(b: &[f32]) -> f32 {
        (b.iter().map(|x| x * x).sum::<f32>() / b.len() as f32).sqrt()
    }

    #[test]
    fn textures_sound_and_fade_out() {
        for tex in TextureKind::ALL {
            let sr = 48000.0;
            let (src, root) = Granular::texture_source(tex, sr);
            assert!(src.frames.iter().all(|x| x.is_finite()));
            let mut g = Granular::new(src, root, tex, false, sr);
            g.note_on(60, 0.9);
            g.note_on(64, 0.9);
            let mut buf = vec![0.0; 48000 * 2];
            g.render(&mut buf);
            let level = rms(&buf[48000..]);
            assert!(level > 0.02, "{tex:?} too quiet: {level}");
            assert!(
                buf.iter().all(|x| x.is_finite() && x.abs() < 2.0),
                "{tex:?} blew up"
            );
            g.note_off(60);
            g.note_off(64);
            let mut tail = vec![0.0; 48000 * 5];
            g.render(&mut tail);
            assert!(rms(&tail[48000 * 4..]) < 1e-4, "{tex:?} never stops");
        }
    }
}
