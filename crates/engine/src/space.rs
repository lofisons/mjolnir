//! What the whole mix shares: one plate reverb and one tape echo that every
//! track sends to, the pump that makes room for the kick, and the punch
//! effects played by hand over the master.

use std::f32::consts::TAU;

use crate::dsp::{Svf, Turn};

/// A delay line read at fixed or moving taps. Its length is a power of two,
/// so finding a tap is a mask, not a division.
struct Line {
    buf: Vec<f32>,
    mask: usize,
    pos: usize,
}

impl Line {
    fn new(len: usize) -> Self {
        let n = len.max(4).next_power_of_two();
        Line {
            buf: vec![0.0; n],
            mask: n - 1,
            pos: 0,
        }
    }

    #[inline]
    fn write(&mut self, x: f32) {
        self.buf[self.pos] = x;
        self.pos = (self.pos + 1) & self.mask;
    }

    /// The sample written `delay` samples ago (1 = the last one written).
    #[inline]
    fn tap(&self, delay: usize) -> f32 {
        self.buf[self.pos.wrapping_sub(delay) & self.mask]
    }

    #[inline]
    fn tap_frac(&self, delay: f32) -> f32 {
        let i = delay as usize;
        let f = delay - i as f32;
        let a = self.buf[self.pos.wrapping_sub(i) & self.mask];
        let b = self.buf[self.pos.wrapping_sub(i + 1) & self.mask];
        a + (b - a) * f
    }
}

/// A plate reverb after Dattorro's figure-of-eight tank: four input
/// diffusers, then two halves that feed each other, each with a wandering
/// all-pass, a delay, a damping filter, a second all-pass and a second delay.
pub struct Plate {
    pre: Line,
    pre_len: usize,
    bandwidth: f32,
    in_lp: f32,
    diff: [Line; 4],
    diff_len: [usize; 4],
    // left and right halves of the tank
    ap1: [Line; 2],
    ap1_len: [f32; 2],
    d1: [Line; 2],
    d1_len: [usize; 2],
    damp: [f32; 2],
    ap2: [Line; 2],
    ap2_len: [usize; 2],
    d2: [Line; 2],
    d2_len: [usize; 2],
    cross: [f32; 2],
    lfo: Turn,
    excursion: f32,
    /// Where the left and the right output read the tank, in samples.
    taps_l: [usize; 7],
    taps_r: [usize; 7],
    /// 0..1: how long the tail lasts.
    pub decay: f32,
    /// 0..1: how fast the top of the tail dies.
    pub damping: f32,
    frozen: bool,
}

impl Plate {
    pub fn new(sr: f32) -> Self {
        // the published lengths are for 29761 Hz
        let k = sr / 29761.0;
        let n = |x: f32| ((x * k).round() as usize).max(2);
        let line = |x: f32| Line::new(n(x) + 64);
        Plate {
            pre: Line::new((0.05 * sr) as usize),
            pre_len: (0.012 * sr) as usize,
            bandwidth: 0.55,
            in_lp: 0.0,
            diff: [line(142.0), line(107.0), line(379.0), line(277.0)],
            diff_len: [n(142.0), n(107.0), n(379.0), n(277.0)],
            ap1: [line(672.0 + 40.0), line(908.0 + 40.0)],
            ap1_len: [672.0 * k, 908.0 * k],
            d1: [line(4453.0), line(4217.0)],
            d1_len: [n(4453.0), n(4217.0)],
            damp: [0.0; 2],
            ap2: [line(1800.0), line(2656.0)],
            ap2_len: [n(1800.0), n(2656.0)],
            d2: [line(3720.0), line(3163.0)],
            d2_len: [n(3720.0), n(3163.0)],
            cross: [0.0; 2],
            lfo: Turn::new(0.8, sr),
            excursion: 14.0 * k,
            taps_l: [266.0, 2974.0, 1913.0, 1996.0, 1990.0, 187.0, 1066.0]
                .map(|x: f32| ((x * k) as usize).max(1)),
            taps_r: [353.0, 3627.0, 1228.0, 2673.0, 2111.0, 335.0, 121.0]
                .map(|x: f32| ((x * k) as usize).max(1)),
            decay: 0.86,
            damping: 0.5,
            frozen: false,
        }
    }

    /// Hold the tail for ever and let nothing new in.
    pub fn freeze(&mut self, on: bool) {
        self.frozen = on;
    }

    #[inline]
    pub fn process(&mut self, input: f32) -> (f32, f32) {
        let (decay, damping, gate) = if self.frozen {
            (1.0, 0.0, 0.0)
        } else {
            (self.decay, self.damping, 1.0)
        };
        self.pre.write(input * gate);
        let x = self.pre.tap(self.pre_len.max(1));
        self.in_lp += self.bandwidth * (x - self.in_lp);
        // input diffusion
        let mut v = self.in_lp;
        for (i, g) in [0.75f32, 0.75, 0.625, 0.625].into_iter().enumerate() {
            let d = self.diff[i].tap(self.diff_len[i]);
            let w = v - g * d;
            self.diff[i].write(w);
            v = d + g * w;
        }
        self.lfo.next();
        let wander = [self.lfo.sin, self.lfo.cos];
        let mut out_half = [0.0f32; 2];
        for h in 0..2 {
            let x = v + decay * self.cross[1 - h];
            // the wandering all-pass (its coefficient is negative)
            let d = self.ap1[h].tap_frac(self.ap1_len[h] + self.excursion * wander[h]);
            let w = x + 0.7 * d;
            self.ap1[h].write(w);
            let a = d - 0.7 * w;
            self.d1[h].write(a);
            let b = self.d1[h].tap(self.d1_len[h]);
            self.damp[h] += (1.0 - damping) * (b - self.damp[h]);
            let c = self.damp[h] * decay;
            let d = self.ap2[h].tap(self.ap2_len[h]);
            let w = c - 0.5 * d;
            self.ap2[h].write(w);
            let e = d + 0.5 * w;
            self.d2[h].write(e);
            out_half[h] = self.d2[h].tap(self.d2_len[h]);
        }
        self.cross = out_half;
        // the output taps of the paper, left and right
        let (tl, tr) = (&self.taps_l, &self.taps_r);
        let l = self.d1[1].tap(tl[0]) + self.d1[1].tap(tl[1]) - self.ap2[1].tap(tl[2])
            + self.d2[1].tap(tl[3])
            - self.d1[0].tap(tl[4])
            - self.ap2[0].tap(tl[5])
            - self.d2[0].tap(tl[6]);
        let r = self.d1[0].tap(tr[0]) + self.d1[0].tap(tr[1]) - self.ap2[0].tap(tr[2])
            + self.d2[0].tap(tr[3])
            - self.d1[1].tap(tr[4])
            - self.ap2[1].tap(tr[5])
            - self.d2[1].tap(tr[6]);
        (l * 0.45, r * 0.45)
    }
}

/// A tape echo: two heads that answer each other left and right, each
/// repeat a little duller and softer, and a time that glides when it changes.
pub struct Echo {
    lines: [Line; 2],
    time: f32,
    lp: [f32; 2],
    hp: [f32; 2],
    wobble: Turn,
    pub feedback: f32,
}

impl Echo {
    pub fn new(sr: f32) -> Self {
        Echo {
            lines: [
                Line::new((2.5 * sr) as usize),
                Line::new((2.5 * sr) as usize),
            ],
            time: 0.3 * sr,
            lp: [0.0; 2],
            hp: [0.0; 2],
            wobble: Turn::new(0.43, sr),
            feedback: 0.45,
        }
    }

    /// `input` goes to the left head; `samples` is the time between repeats.
    #[inline]
    pub fn process(&mut self, input: f32, samples: f32) -> (f32, f32) {
        // the motor takes a moment to reach a new speed, so the pitch slides
        self.time += (samples - self.time) * 0.00012;
        self.wobble.next();
        let d = (self.time * (1.0 + 0.0014 * self.wobble.sin)).max(1.0);
        let mut out = [0.0f32; 2];
        for h in 0..2 {
            let y = self.lines[h].tap_frac(d);
            // every pass through the tape loses top and bottom
            self.lp[h] += 0.32 * (y - self.lp[h]);
            self.hp[h] += 0.02 * (self.lp[h] - self.hp[h]);
            out[h] = self.lp[h] - self.hp[h];
        }
        let fb = self.feedback;
        let soft = |x: f32| x / (1.0 + x.abs());
        self.lines[0].write(soft(input + out[1] * fb));
        self.lines[1].write(soft(out[0] * fb));
        (out[0], out[1])
    }
}

/// The room every track sends to.
pub struct Space {
    pub plate: Plate,
    pub echo: Echo,
}

impl Space {
    pub fn new(sr: f32) -> Self {
        Space {
            plate: Plate::new(sr),
            echo: Echo::new(sr),
        }
    }

    /// Set the size of the room from the master's `space` knob.
    pub fn set_size(&mut self, space: f32) {
        let s = space.clamp(0.0, 1.0);
        self.plate.decay = 0.55 + 0.4 * s;
        self.plate.damping = 0.62 - 0.3 * s;
    }

    /// The wet signal for what was sent to the reverb and to the echo.
    #[inline]
    pub fn process(&mut self, to_reverb: f32, to_echo: f32, echo_samples: f32) -> (f32, f32) {
        let (el, er) = self.echo.process(to_echo, echo_samples);
        // the echoes also sound in the room, a little
        let (rl, rr) = self.plate.process(to_reverb + (el + er) * 0.12);
        (el + rl, er + rr)
    }
}

/// Makes room for the kick: follows the low end of the drums and turns
/// everything else down for a moment.
pub struct Pump {
    low: f32,
    env: f32,
    attack: f32,
    release: f32,
    lp: f32,
}

impl Pump {
    pub fn new(sr: f32) -> Self {
        Pump {
            low: 0.0,
            env: 0.0,
            attack: 1.0 - (-1.0 / (0.003 * sr)).exp(),
            release: 1.0 - (-1.0 / (0.19 * sr)).exp(),
            lp: 1.0 - (-TAU * 140.0 / sr).exp(),
        }
    }

    /// The gain for everything that is not drums, given this sample of drums.
    #[inline]
    pub fn gain(&mut self, drums: f32, amount: f32) -> f32 {
        self.low += self.lp * (drums - self.low);
        let level = (self.low.abs() * 3.2).min(1.0);
        let k = if level > self.env {
            self.attack
        } else {
            self.release
        };
        self.env += k * (level - self.env);
        1.0 - amount.clamp(0.0, 1.0) * 0.85 * self.env
    }
}

/// The punch effects: held, never latched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PunchKind {
    LowPass,
    HighPass,
    Crush,
    Tape,
    LoopBeat,
    LoopStep,
    Dub,
    Freeze,
}

impl PunchKind {
    pub const ALL: [PunchKind; 8] = [
        PunchKind::LoopBeat,
        PunchKind::LoopStep,
        PunchKind::Tape,
        PunchKind::LowPass,
        PunchKind::HighPass,
        PunchKind::Crush,
        PunchKind::Dub,
        PunchKind::Freeze,
    ];

    pub fn id(self) -> &'static str {
        match self {
            PunchKind::LowPass => "lpf",
            PunchKind::HighPass => "hpf",
            PunchKind::Crush => "crush",
            PunchKind::Tape => "tape",
            PunchKind::LoopBeat => "loop",
            PunchKind::LoopStep => "stutter",
            PunchKind::Dub => "dub",
            PunchKind::Freeze => "freeze",
        }
    }

    pub fn from_id(id: &str) -> Option<PunchKind> {
        PunchKind::ALL.into_iter().find(|k| k.id() == id)
    }

    fn index(self) -> usize {
        self as usize
    }
}

const RING_SECONDS: f32 = 4.0;

pub struct Punch {
    held: [bool; 8],
    sr: f32,
    lp: [Svf; 2],
    hp: [Svf; 2],
    lp_cut: f32,
    hp_cut: f32,
    tick: u32,
    crush_hold: (f32, f32),
    crush_count: u32,
    ring: Vec<(f32, f32)>,
    wpos: usize,
    // a held loop: where it starts in the ring, its length, where we are
    loop_start: usize,
    loop_len: usize,
    loop_pos: usize,
    looping: bool,
    // tape stop: the speed and the place the head reads from
    tape_rate: f32,
    tape_pos: f32,
    taping: bool,
    /// Fades between the live signal and an effect, so nothing clicks.
    blend: f32,
}

impl Punch {
    pub fn new(sr: f32) -> Self {
        Punch {
            held: [false; 8],
            sr,
            lp: [Svf::default(), Svf::default()],
            hp: [Svf::default(), Svf::default()],
            lp_cut: 20000.0,
            hp_cut: 10.0,
            tick: 0,
            crush_hold: (0.0, 0.0),
            crush_count: 0,
            ring: vec![(0.0, 0.0); (RING_SECONDS * sr) as usize],
            wpos: 0,
            loop_start: 0,
            loop_len: 1,
            loop_pos: 0,
            looping: false,
            tape_rate: 1.0,
            tape_pos: 0.0,
            taping: false,
            blend: 0.0,
        }
    }

    pub fn set(&mut self, kind: PunchKind, on: bool) {
        self.held[kind.index()] = on;
    }

    pub fn held(&self, kind: PunchKind) -> bool {
        self.held[kind.index()]
    }

    pub fn any(&self) -> bool {
        self.held.iter().any(|h| *h)
            || self.blend > 0.0005
            || self.lp_cut < 19000.0
            || self.hp_cut > 12.0
    }

    /// One frame of the mix through whatever is held. `beat` is a quarter note in samples.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32, beat: f32) -> (f32, f32) {
        let n = self.ring.len();
        let want_loop =
            self.held[PunchKind::LoopBeat.index()] || self.held[PunchKind::LoopStep.index()];
        let want_tape = self.held[PunchKind::Tape.index()];

        // ---- the loop: grab what just played and turn it
        if want_loop {
            let len = if self.held[PunchKind::LoopStep.index()] {
                beat / 4.0
            } else {
                beat
            };
            let len = (len as usize).clamp(64, n / 2);
            if !self.looping || len != self.loop_len {
                self.looping = true;
                self.loop_len = len;
                self.loop_start = (self.wpos + n - len) % n;
                self.loop_pos = 0;
            }
        } else {
            self.looping = false;
        }
        // ---- the tape: the motor stops
        if want_tape && !self.taping {
            self.taping = true;
            self.tape_rate = 1.0;
            self.tape_pos = self.wpos as f32;
        } else if !want_tape {
            self.taping = false;
        }

        let (mut l, mut r) = (l, r);
        let effect = if self.looping {
            let i = (self.loop_start + self.loop_pos) % n;
            // a short fade at each turn of the loop
            let edge = self.loop_pos.min(self.loop_len - 1 - self.loop_pos) as f32 / 96.0;
            self.loop_pos = (self.loop_pos + 1) % self.loop_len;
            let (a, b) = self.ring[i];
            Some((a * edge.min(1.0), b * edge.min(1.0)))
        } else if self.taping {
            self.tape_rate = (self.tape_rate - 1.6 / self.sr).max(0.0);
            self.tape_pos += self.tape_rate;
            let i = self.tape_pos as usize;
            let f = self.tape_pos - i as f32;
            let (a0, b0) = self.ring[i % n];
            let (a1, b1) = self.ring[(i + 1) % n];
            let g = self.tape_rate.min(1.0).sqrt();
            Some(((a0 + (a1 - a0) * f) * g, (b0 + (b1 - b0) * f) * g))
        } else {
            None
        };
        // the ring keeps recording the live mix unless a loop is holding it
        if !self.looping {
            self.ring[self.wpos] = (l, r);
            self.wpos = (self.wpos + 1) % n;
        }
        let target = if effect.is_some() { 1.0 } else { 0.0 };
        self.blend += (target - self.blend) * 0.01;
        if let Some((a, b)) = effect {
            l += (a - l) * self.blend;
            r += (b - r) * self.blend;
        } else if self.blend > 0.0005 {
            // coming back: the live mix fades up from nothing rather than
            // jumping in
            l *= 1.0 - self.blend;
            r *= 1.0 - self.blend;
        }

        // ---- the filters glide to where they are held and back
        let lp_target: f32 = if self.held[PunchKind::LowPass.index()] {
            320.0
        } else {
            20000.0
        };
        let hp_target: f32 = if self.held[PunchKind::HighPass.index()] {
            1900.0
        } else {
            10.0
        };
        // a glide in pitch, worked out only while a filter is on its way
        if (self.lp_cut - lp_target).abs() > 1.0 {
            self.lp_cut *= (lp_target / self.lp_cut).powf(0.00035);
        }
        if (self.hp_cut - hp_target).abs() > 0.05 {
            self.hp_cut *= (hp_target / self.hp_cut).powf(0.00035);
        }
        self.tick = self.tick.wrapping_add(1);
        if self.tick % 16 == 0 {
            let top = self.sr * 0.45;
            for f in self.lp.iter_mut() {
                f.set(self.lp_cut.min(top), 0.9, self.sr);
            }
            for f in self.hp.iter_mut() {
                f.set(self.hp_cut.min(top), 0.8, self.sr);
            }
        }
        if self.lp_cut < 19000.0 {
            l = self.lp[0].process(l).low;
            r = self.lp[1].process(r).low;
        }
        if self.hp_cut > 12.0 {
            l = self.hp[0].process(l).high;
            r = self.hp[1].process(r).high;
        }

        // ---- crush: few bits, few samples
        if self.held[PunchKind::Crush.index()] {
            if self.crush_count == 0 {
                let q = |x: f32| (x * 24.0).round() / 24.0;
                self.crush_hold = (q(l), q(r));
                self.crush_count = (self.sr / 5500.0) as u32;
            } else {
                self.crush_count -= 1;
            }
            l = self.crush_hold.0;
            r = self.crush_hold.1;
        }
        (l, r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plate_rings_then_dies_and_never_blows_up() {
        let mut p = Plate::new(48000.0);
        p.decay = 0.95;
        let mut peak = 0.0f32;
        let mut late = 0.0f32;
        for i in 0..48000 * 12 {
            let x = if i < 4800 {
                ((i * 7919) % 200) as f32 / 100.0 - 1.0
            } else {
                0.0
            };
            let (l, r) = p.process(x);
            assert!(l.is_finite() && r.is_finite());
            peak = peak.max(l.abs());
            if i > 48000 * 11 {
                late = late.max(l.abs());
            }
        }
        assert!(peak > 0.05, "the plate is silent");
        assert!(peak < 4.0, "the plate is too loud: {peak}");
        assert!(late < peak * 0.05, "the tail never dies: {late} of {peak}");
    }

    #[test]
    fn frozen_plate_holds_its_tail() {
        let mut p = Plate::new(48000.0);
        for i in 0..24000 {
            p.process(((i * 31) % 100) as f32 / 50.0 - 1.0);
        }
        p.freeze(true);
        let mut level = |p: &mut Plate| {
            (0..24000)
                .map(|_| p.process(0.0).0.abs())
                .fold(0.0f32, f32::max)
        };
        let early = level(&mut p);
        for _ in 0..48000 * 5 {
            p.process(0.0);
        }
        let late = level(&mut p);
        assert!(late > early * 0.4, "a frozen tail faded: {early} to {late}");
        assert!(late < early * 2.5, "a frozen tail grew: {early} to {late}");
    }

    #[test]
    fn echo_repeats_and_fades() {
        let mut e = Echo::new(48000.0);
        e.time = 4800.0;
        let mut energy = [0.0f32; 6];
        for i in 0..4800 * 6 {
            let x = if i < 200 { 1.0 } else { 0.0 };
            let (l, r) = e.process(x, 4800.0);
            energy[i / 4800] += l * l + r * r;
        }
        assert!(energy[1] > 0.001, "no first repeat");
        assert!(energy[4] < energy[1], "repeats do not fade");
    }

    #[test]
    fn pump_ducks_on_a_kick_and_recovers() {
        let mut p = Pump::new(48000.0);
        let mut low = 1.0f32;
        for i in 0..4800 {
            let kick = (TAU * 55.0 * i as f32 / 48000.0).sin() * 0.7;
            low = low.min(p.gain(kick, 1.0));
        }
        assert!(low < 0.5, "the pump barely ducks: {low}");
        let mut g = 0.0;
        for _ in 0..48000 {
            g = p.gain(0.0, 1.0);
        }
        assert!(g > 0.97, "the pump never lets go: {g}");
        assert_eq!(Pump::new(48000.0).gain(0.9, 0.0), 1.0);
    }

    #[test]
    fn punch_passes_through_at_rest_and_loops_when_held() {
        let mut p = Punch::new(48000.0);
        for i in 0..48000 {
            let x = (i as f32 * 0.01).sin();
            let (l, _) = p.process(x, x, 24000.0);
            assert!((l - x).abs() < 1e-4, "punch colours the mix at rest");
        }
        p.set(PunchKind::LoopStep, true);
        let mut out = Vec::new();
        for i in 0..48000 {
            let x = ((48000 + i) as f32 * 0.01).sin();
            out.push(p.process(x, x, 24000.0).0);
        }
        // one sixteenth is 6000 samples: the held loop repeats with that period
        let a = &out[24000..30000];
        let b = &out[30000..36000];
        let diff: f32 = a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum::<f32>() / 6000.0;
        assert!(diff < 0.02, "a held loop does not repeat: {diff}");
        assert!(out.iter().all(|x| x.is_finite()));
    }
}
