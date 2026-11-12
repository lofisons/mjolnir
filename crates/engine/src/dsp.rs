//! Small, allocation-free DSP building blocks.

use std::f32::consts::PI;

/// Fast xorshift white noise in -1..1.
#[derive(Clone)]
pub struct Noise(u32);

impl Noise {
    pub fn new(seed: u32) -> Self {
        Noise(seed.max(1))
    }
    #[inline]
    pub fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// Topology-preserving-transform state variable filter (Zavalishin).
#[derive(Clone, Default)]
pub struct Svf {
    ic1: f32,
    ic2: f32,
    g: f32,
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
}

#[derive(Clone, Copy)]
pub struct SvfOut {
    pub low: f32,
    pub band: f32,
    pub high: f32,
}

impl Svf {
    pub fn set(&mut self, cutoff_hz: f32, q: f32, sample_rate: f32) {
        let fc = cutoff_hz.clamp(10.0, sample_rate * 0.49);
        self.g = (PI * fc / sample_rate).tan();
        self.k = 1.0 / q.max(0.05);
        self.a1 = 1.0 / (1.0 + self.g * (self.g + self.k));
        self.a2 = self.g * self.a1;
        self.a3 = self.g * self.a2;
    }

    #[inline]
    pub fn process(&mut self, v0: f32) -> SvfOut {
        let v3 = v0 - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        SvfOut {
            low: v2,
            band: v1,
            high: v0 - self.k * v1 - v2,
        }
    }

    pub fn reset(&mut self) {
        self.ic1 = 0.0;
        self.ic2 = 0.0;
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Stage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// Linear-attack, exponential decay/release ADSR. Times in seconds.
#[derive(Clone)]
pub struct Adsr {
    stage: Stage,
    value: f32,
    attack_inc: f32,
    decay_coef: f32,
    release_coef: f32,
    sustain: f32,
}

impl Adsr {
    pub fn new(attack: f32, decay: f32, sustain: f32, release: f32, sr: f32) -> Self {
        let mut e = Adsr {
            stage: Stage::Idle,
            value: 0.0,
            attack_inc: 1.0,
            decay_coef: 0.0,
            release_coef: 0.0,
            sustain,
        };
        e.set(attack, decay, sustain, release, sr);
        e
    }

    pub fn set(&mut self, attack: f32, decay: f32, sustain: f32, release: f32, sr: f32) {
        self.attack_inc = 1.0 / (attack.max(0.0005) * sr);
        self.decay_coef = exp_coef(decay, sr);
        self.release_coef = exp_coef(release, sr);
        self.sustain = sustain.clamp(0.0, 1.0);
    }

    pub fn gate_on(&mut self) {
        self.stage = Stage::Attack;
    }

    pub fn gate_off(&mut self) {
        if self.stage != Stage::Idle {
            self.stage = Stage::Release;
        }
    }

    /// Silence immediately.
    pub fn kill(&mut self) {
        self.stage = Stage::Idle;
        self.value = 0.0;
    }

    pub fn is_active(&self) -> bool {
        self.stage != Stage::Idle
    }

    pub fn is_releasing(&self) -> bool {
        self.stage == Stage::Release
    }

    #[inline]
    pub fn next(&mut self) -> f32 {
        match self.stage {
            Stage::Idle => {}
            Stage::Attack => {
                self.value += self.attack_inc;
                if self.value >= 1.0 {
                    self.value = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.value = self.sustain + (self.value - self.sustain) * self.decay_coef;
                if (self.value - self.sustain).abs() < 1e-4 {
                    self.value = self.sustain;
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => {
                self.value = self.sustain;
                if self.sustain <= 1e-4 {
                    self.stage = Stage::Idle;
                    self.value = 0.0;
                }
            }
            Stage::Release => {
                self.value *= self.release_coef;
                if self.value < 1e-4 {
                    self.value = 0.0;
                    self.stage = Stage::Idle;
                }
            }
        }
        self.value
    }
}

/// Coefficient so that an exponential decays to ~-60 dB in `time` seconds.
pub fn exp_coef(time: f32, sr: f32) -> f32 {
    (-6.9 / (time.max(0.0005) * sr)).exp()
}

/// One-shot exponential decay envelope.
#[derive(Clone, Default)]
pub struct Decay {
    pub value: f32,
    coef: f32,
}

impl Decay {
    pub fn trigger(&mut self, level: f32, time: f32, sr: f32) {
        self.value = level;
        self.coef = exp_coef(time, sr);
    }
    #[inline]
    pub fn next(&mut self) -> f32 {
        let v = self.value;
        self.value *= self.coef;
        v
    }
}

#[inline]
fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let t = t / dt;
        t + t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

/// Band-limited saw / square / sine oscillator.
#[derive(Clone, Default)]
pub struct Osc {
    pub phase: f32,
}

impl Osc {
    #[inline]
    pub fn saw(&mut self, freq: f32, sr: f32) -> f32 {
        let dt = (freq / sr).min(0.5);
        let v = 2.0 * self.phase - 1.0 - poly_blep(self.phase, dt);
        self.advance(dt);
        v
    }
    #[inline]
    pub fn square(&mut self, freq: f32, sr: f32) -> f32 {
        let dt = (freq / sr).min(0.5);
        let mut v = if self.phase < 0.5 { 1.0 } else { -1.0 };
        v += poly_blep(self.phase, dt);
        v -= poly_blep((self.phase + 0.5) % 1.0, dt);
        self.advance(dt);
        v
    }
    #[inline]
    pub fn sine(&mut self, freq: f32, sr: f32) -> f32 {
        let v = (self.phase * 2.0 * PI).sin();
        self.advance(freq / sr);
        v
    }
    #[inline]
    fn advance(&mut self, dt: f32) {
        self.phase += dt;
        if self.phase >= 1.0 {
            self.phase -= self.phase.floor();
        }
    }
}

/// One-pole parameter smoother.
#[derive(Clone)]
pub struct Smoother {
    pub value: f32,
    coef: f32,
}

impl Smoother {
    pub fn new(value: f32, time: f32, sr: f32) -> Self {
        Smoother {
            value,
            coef: (-1.0 / (time * sr)).exp(),
        }
    }
    #[inline]
    pub fn next(&mut self, target: f32) -> f32 {
        self.value = target + (self.value - target) * self.coef;
        self.value
    }
}

/// A soft clipper shaped like tanh at a fraction of its cost: exact at 0,
/// within a few thousandths up to 2, flat at one from 3 on.
#[inline]
pub fn soft(x: f32) -> f32 {
    if x <= -3.0 {
        -1.0
    } else if x >= 3.0 {
        1.0
    } else {
        x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
    }
}

const SINE_SIZE: usize = 2048;

/// One cycle of a sine, for oscillators that would otherwise call `sin`
/// for every sample.
pub fn sine_table() -> &'static [f32; SINE_SIZE + 1] {
    static CELL: std::sync::OnceLock<Box<[f32; SINE_SIZE + 1]>> = std::sync::OnceLock::new();
    CELL.get_or_init(|| {
        let mut t = Box::new([0.0f32; SINE_SIZE + 1]);
        for (i, v) in t.iter_mut().enumerate() {
            *v = (std::f32::consts::TAU * i as f32 / SINE_SIZE as f32).sin();
        }
        t
    })
}

/// The sine of `turns` of a circle (1.0 = once round), read from the table.
#[inline]
pub fn sin_turns(table: &[f32; SINE_SIZE + 1], turns: f32) -> f32 {
    let x = (turns - turns.floor()) * SINE_SIZE as f32;
    let i = x as usize;
    let f = x - i as f32;
    // i is below SINE_SIZE + 1 because x is below SINE_SIZE
    let a = table[i.min(SINE_SIZE - 1)];
    let b = table[i.min(SINE_SIZE - 1) + 1];
    a + (b - a) * f
}

/// A slow sine and cosine without calling either: a point turning on a circle.
#[derive(Clone, Copy)]
pub struct Turn {
    pub sin: f32,
    pub cos: f32,
    step_sin: f32,
    step_cos: f32,
}

impl Turn {
    pub fn new(hz: f32, sr: f32) -> Self {
        let w = std::f32::consts::TAU * hz / sr;
        Turn {
            sin: 0.0,
            cos: 1.0,
            step_sin: w.sin(),
            step_cos: w.cos(),
        }
    }

    #[inline]
    pub fn next(&mut self) {
        let s = self.sin * self.step_cos + self.cos * self.step_sin;
        let c = self.cos * self.step_cos - self.sin * self.step_sin;
        // pull the point back onto the circle, so rounding never lets it drift
        let k = 1.5 - 0.5 * (s * s + c * c);
        self.sin = s * k;
        self.cos = c * k;
    }
}

pub fn midi_to_hz(pitch: f32) -> f32 {
    440.0 * 2f32.powf((pitch - 69.0) / 12.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adsr_reaches_idle() {
        let mut e = Adsr::new(0.001, 0.01, 0.5, 0.01, 1000.0);
        e.gate_on();
        for _ in 0..100 {
            e.next();
        }
        assert!((e.next() - 0.5).abs() < 0.01);
        e.gate_off();
        for _ in 0..200 {
            e.next();
        }
        assert!(!e.is_active());
    }

    #[test]
    fn svf_lowpass_attenuates_highs() {
        let sr = 48000.0;
        let mut f = Svf::default();
        f.set(200.0, 0.707, sr);
        let mut osc = Osc::default();
        let mut peak: f32 = 0.0;
        for i in 0..48000 {
            let y = f.process(osc.sine(5000.0, sr)).low;
            if i > 1000 {
                peak = peak.max(y.abs());
            }
        }
        assert!(peak < 0.01, "peak {peak}");
    }

    #[test]
    fn saw_is_bounded() {
        let mut o = Osc::default();
        for _ in 0..10000 {
            assert!(o.saw(440.0, 48000.0).abs() <= 1.1);
        }
    }
}
