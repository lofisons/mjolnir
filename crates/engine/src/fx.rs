//! Per-track effect chain driven by the macro knobs:
//! filter (cutoff + resonance) -> distortion -> saturation -> volume, with
//! two sends: to the echo and to the reverb the whole mix shares.

use crate::dsp::{Noise, Smoother, Svf};
use crate::model::{MacroParam, Macros, Master};

pub struct FxChain {
    sr: f32,
    filter: Svf,
    smooth: [Smoother; 7],
    counter: u32,
    // DC blocker state
    dc_x: f32,
    dc_y: f32,
}

/// Map the 0..1 cutoff macro to Hz (exponential 30 Hz .. 20 kHz).
pub fn cutoff_hz(v: f32) -> f32 {
    30.0 * (20000.0f32 / 30.0).powf(v.clamp(0.0, 1.0))
}

impl FxChain {
    pub fn new(sr: f32, macros: &Macros) -> Self {
        FxChain {
            sr,
            filter: Svf::default(),
            smooth: std::array::from_fn(|i| Smoother::new(macros.0[i], 0.02, sr)),
            counter: 0,
            dc_x: 0.0,
            dc_y: 0.0,
        }
    }

    /// Process mono `input` into stereo `left`/`right` and into what the
    /// track sends to the shared reverb and echo (all overwritten).
    pub fn process(
        &mut self,
        input: &[f32],
        left: &mut [f32],
        right: &mut [f32],
        to_reverb: &mut [f32],
        to_echo: &mut [f32],
        macros: &Macros,
    ) {
        for i in 0..input.len() {
            let m: [f32; 7] = std::array::from_fn(|k| self.smooth[k].next(macros.0[k]));
            let cutoff = m[MacroParam::Cutoff.index()];
            let res = m[MacroParam::Resonance.index()];
            if self.counter == 0 {
                self.filter
                    .set(cutoff_hz(cutoff), 0.6 + res * res * 14.0, self.sr);
                self.counter = 8;
            }
            self.counter -= 1;

            // DC blocker (~5 Hz high-pass) so no instrument can push the mix off centre
            let raw = input[i];
            self.dc_y = raw - self.dc_x + 0.9993 * self.dc_y;
            self.dc_x = raw;
            let mut x = self.dc_y;
            if cutoff < 0.999 || res > 0.15 {
                x = self.filter.process(x).low;
            }

            let drive = m[MacroParam::Drive.index()];
            if drive > 0.001 {
                let g = 1.0 + drive * 30.0;
                let d = (x * g).clamp(-1.0, 1.0);
                // asymmetric soft-ish clip blended with the dry signal
                let shaped = d - d * d * d / 3.0;
                x = x * (1.0 - drive) + shaped * 1.4 * drive;
            }

            let sat = m[MacroParam::Saturation.index()];
            if sat > 0.001 {
                let g = 1.0 + sat * 5.0;
                x = (x * g).tanh() / g.sqrt();
            }

            let vol = m[MacroParam::Volume.index()];
            let gain = vol * vol * 1.4;
            let y = x * gain;
            let rmix = m[MacroParam::Reverb.index()];
            let dmix = m[MacroParam::Delay.index()];
            // a sound deep in the room steps back a little
            let dry = y * (1.0 - 0.3 * rmix);
            left[i] = dry;
            right[i] = dry;
            to_reverb[i] = y * rmix * rmix * 1.6;
            to_echo[i] = y * dmix * 0.9;
        }
    }
}

/// Master bus: lo-fi character (tape wow & flutter, warmth, vinyl) then
/// volume and a gentle limiter / soft clipper.
pub struct MasterBus {
    sr: f32,
    env: f32,
    release: f32,
    settings: Master,
    smooth: [Smoother; 4],
    // tape: modulated delay line
    tape_l: Vec<f32>,
    tape_r: Vec<f32>,
    tape_idx: usize,
    wow_phase: f32,
    flutter_phase: f32,
    // warmth: one-pole low-pass per channel
    lp_l: f32,
    lp_r: f32,
    // vinyl
    noise: Noise,
    crackle: f32,
    hiss_lp: f32,
}

const TAPE_BUF: usize = 4096;

impl MasterBus {
    pub fn new(sr: f32) -> Self {
        let m = Master::default();
        MasterBus {
            sr,
            env: 0.0,
            release: (-1.0 / (0.15 * sr)).exp(),
            settings: m,
            smooth: [
                Smoother::new(m.volume, 0.03, sr),
                Smoother::new(m.vinyl, 0.03, sr),
                Smoother::new(m.tape, 0.03, sr),
                Smoother::new(m.warmth, 0.03, sr),
            ],
            tape_l: vec![0.0; TAPE_BUF],
            tape_r: vec![0.0; TAPE_BUF],
            tape_idx: 0,
            wow_phase: 0.0,
            flutter_phase: 0.0,
            lp_l: 0.0,
            lp_r: 0.0,
            noise: Noise::new(4242),
            crackle: 0.0,
            hiss_lp: 0.0,
        }
    }

    pub fn set(&mut self, settings: Master) {
        self.settings = settings;
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let volume = self.smooth[0].next(self.settings.volume);
        let vinyl = self.smooth[1].next(self.settings.vinyl);
        let tape = self.smooth[2].next(self.settings.tape);
        let warmth = self.smooth[3].next(self.settings.warmth);
        let (mut l, mut r) = (l, r);

        // Tape: read a short delay line whose length drifts slowly (wow) and
        // quickly (flutter). Always written so turning it up is seamless.
        self.tape_l[self.tape_idx] = l;
        self.tape_r[self.tape_idx] = r;
        if tape > 0.001 {
            let two_pi = std::f32::consts::TAU;
            self.wow_phase = (self.wow_phase + 0.55 / self.sr) % 1.0;
            self.flutter_phase = (self.flutter_phase + 6.3 / self.sr) % 1.0;
            let ms = 6.0
                + tape
                    * (2.2 * (two_pi * self.wow_phase).sin()
                        + 0.25 * (two_pi * self.flutter_phase).sin());
            let d = (ms * 0.001 * self.sr).clamp(1.0, (TAPE_BUF - 2) as f32);
            let pos = self.tape_idx as f32 + TAPE_BUF as f32 - d;
            let i = pos as usize;
            let frac = pos - i as f32;
            let read = |buf: &[f32]| {
                let a = buf[i % TAPE_BUF];
                let b = buf[(i + 1) % TAPE_BUF];
                a + (b - a) * frac
            };
            let (tl, tr) = (read(&self.tape_l), read(&self.tape_r));
            l = l + (tl - l) * tape.min(1.0);
            r = r + (tr - r) * tape.min(1.0);
        }
        self.tape_idx = (self.tape_idx + 1) % TAPE_BUF;

        // Warmth: low-pass sweeping from ~18 kHz down to ~2.2 kHz plus soft drive.
        if warmth > 0.001 {
            let cutoff = 18000.0 * (2200.0f32 / 18000.0).powf(warmth);
            let a = 1.0 - (-std::f32::consts::TAU * cutoff / self.sr).exp();
            self.lp_l += a * (l - self.lp_l);
            self.lp_r += a * (r - self.lp_r);
            let g = 1.0 + warmth * 1.5;
            l = (self.lp_l * g).tanh() / g.tanh();
            r = (self.lp_r * g).tanh() / g.tanh();
        } else {
            self.lp_l = l;
            self.lp_r = r;
        }

        // Vinyl: random pops that decay fast, over band-limited hiss.
        if vinyl > 0.001 {
            let n = self.noise.next();
            if (n * 0.5 + 0.5) < vinyl * 0.0004 {
                self.crackle = self.noise.next() * (0.2 + 0.4 * vinyl);
            }
            let pop = self.crackle;
            self.crackle *= 0.86;
            self.hiss_lp += 0.15 * (self.noise.next() - self.hiss_lp);
            let hiss = self.hiss_lp * 0.012 * vinyl;
            l += pop + hiss;
            r += pop * 0.8 + hiss;
        }

        let gain = volume * volume * 1.5;
        l *= gain;
        r *= gain;
        let peak = l.abs().max(r.abs());
        self.env = if peak > self.env {
            peak
        } else {
            peak + (self.env - peak) * self.release
        };
        let lim = if self.env > 0.9 { 0.9 / self.env } else { 1.0 };
        ((l * lim).tanh(), (r * lim).tanh())
    }
}
