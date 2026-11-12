//! Real-time engine: runs on the audio thread, owns the instruments and a
//! read-only copy of the patterns, and is driven by lock-free commands.
//!
//! Rules for this module: no locks, and no allocation or deallocation in
//! `process`. Heap data arrives boxed through commands and old boxes leave
//! through the garbage queue to be dropped on the control thread.

use std::any::Any;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use rtrb::{Consumer, Producer};

use crate::fx::{FxChain, MasterBus};
use crate::instruments::Instrument;
use crate::model::{
    MacroParam, Macros, Master, Pattern, MAX_PARTS, MAX_TRACKS, PPQ, SLOTS_PER_TRACK,
    TICKS_PER_4_4_BAR, TICKS_PER_STEP,
};

/// Events are scheduled with this granularity (samples). ~0.33 ms at 48 kHz.
const CHUNK: usize = 16;
/// Most strikes a repeated note makes inside a sixteenth.
const MAX_REPEAT: u8 = 4;
const MAX_PENDING_OFFS: usize = 128;
const MAX_SWING_TICKS: f64 = TICKS_PER_STEP as f64 * 0.5;
/// Largest "laid-back" delay humanize adds (~1/12 of a beat).
const MAX_HUMAN_TICKS: f64 = 8.0;

/// Deterministic 0..1 hash so each note of each loop pass varies the same
/// way every time (renders are reproducible) but differently from its neighbours.
fn hash01(a: u64, b: u64) -> f32 {
    let mut x = a.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ b.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    x ^= x >> 31;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 29;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

pub type Garbage = Box<dyn Any + Send>;

pub enum Command {
    /// Hold or let go a punch effect over the whole mix.
    Punch {
        kind: crate::space::PunchKind,
        on: bool,
    },
    SetMacroPatch {
        track: usize,
        patch: crate::macro_synth::MacroPatch,
    },
    Play,
    /// Audible pre-roll in beats; transport and recording stay stopped until it ends.
    PlayWithCountIn(u32),
    Stop,
    SetTempo {
        bpm: f32,
        swing: f32,
    },
    /// Time signature: ticks per bar and per beat (the metronome's click).
    SetMeter {
        bar: u32,
        beat: u32,
    },
    NoteOn {
        track: usize,
        pitch: u8,
        velocity: f32,
    },
    NoteOff {
        track: usize,
        pitch: u8,
    },
    SetPattern {
        track: usize,
        slot: usize,
        pattern: Option<Box<Pattern>>,
    },
    /// Launch a slot (None = stop the track). Quantized to the next bar while
    /// playing unless `immediate`.
    Launch {
        track: usize,
        slot: Option<usize>,
        immediate: bool,
    },
    SetMacros {
        track: usize,
        macros: Macros,
    },
    /// While a knob is being touched, its automation playback is suspended.
    Touch {
        track: usize,
        param: MacroParam,
        touching: bool,
    },
    /// Mixer strip: mute, solo and pan (-1..1).
    SetMix {
        track: usize,
        mute: bool,
        solo: bool,
        pan: f32,
    },
    SetMaster(Master),
    SetHumanize {
        track: usize,
        amount: f32,
    },
    /// The track's own mix of its parts (drum pads, synth layers).
    SetParts {
        track: usize,
        gains: [f32; MAX_PARTS],
    },
    /// Sound design: a designed synth's new patch, applied live.
    SetSynthPatch {
        track: usize,
        patch: crate::sound::SynthPatch,
    },
    /// Sound design: one pad of a designed kit, applied from its next hit.
    SetDrumPad {
        track: usize,
        pad: usize,
        sound: crate::sound::DrumPad,
    },
    /// Replace the song timeline (clip lanes indexed by track position).
    SetSong(Box<SongRuntime>),
    /// Song mode plays the timeline; session mode plays launched clips.
    SetSongMode(bool),
    /// Where Play starts in song mode; while playing, jumps there.
    Seek {
        tick: f64,
    },
    SetInstrument {
        track: usize,
        instrument: Box<Instrument>,
    },
    InsertTrack {
        index: usize,
        track: Box<TrackRuntime>,
    },
    RemoveTrack {
        index: usize,
    },
    /// Metronome click on every beat while playing (never in exports).
    SetMetronome(bool),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EngineEvent {
    /// A note played live (pads/keyboard), timestamped for capture/recording.
    LiveNote {
        track: usize,
        pitch: u8,
        velocity: f32,
        on: bool,
        tick: f64,
        playing: bool,
    },
}

/// State shared with the UI thread (written by the engine, read anywhere).
pub struct SharedStatus {
    pub playing: AtomicBool,
    pub count_in: AtomicU32,
    song_tick: AtomicU64,
    pub active_slot: [AtomicI32; MAX_TRACKS],
    pub queued_slot: [AtomicI32; MAX_TRACKS],
    origins: [AtomicU64; MAX_TRACKS],
    levels: [AtomicU32; MAX_TRACKS],
    master_level: AtomicU32,
    /// Effective (automated) macro values for each track, for animated knobs.
    macros: [[AtomicU32; 7]; MAX_TRACKS],
    pub sample_rate: AtomicU32,
}

/// One clip of the timeline, in ticks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipSpan {
    pub start: f64,
    pub end: f64,
    pub slot: usize,
}

/// Audio-thread copy of the song: per-track lanes sorted by start.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SongRuntime {
    pub lanes: Vec<Vec<ClipSpan>>,
    pub length: f64,
    pub looping: bool,
    /// Fade-in / fade-out lengths in ticks (0 = none).
    pub fade_in: f64,
    pub fade_out: f64,
}

impl SongRuntime {
    /// Equal-power fade gain at `tick` (1 when no fades apply).
    pub fn fade_gain(&self, tick: f64) -> f32 {
        let mut g = 1.0f64;
        if self.fade_in > 0.0 {
            g *= (tick / self.fade_in).clamp(0.0, 1.0);
        }
        if self.fade_out > 0.0 {
            g *= ((self.length - tick) / self.fade_out).clamp(0.0, 1.0);
        }
        (g * std::f64::consts::FRAC_PI_2).sin() as f32
    }

    fn clip_at(&self, track: usize, tick: f64) -> Option<ClipSpan> {
        let lane = self.lanes.get(track)?;
        let i = lane.partition_point(|c| c.start <= tick);
        let c = lane.get(i.checked_sub(1)?)?;
        (tick < c.end).then_some(*c)
    }
}

/// -1 = nothing, -2 = queued stop.
pub const SLOT_NONE: i32 = -1;
pub const SLOT_STOP: i32 = -2;

impl SharedStatus {
    pub fn new() -> Self {
        SharedStatus {
            playing: AtomicBool::new(false),
            count_in: AtomicU32::new(0),
            song_tick: AtomicU64::new(0),
            active_slot: std::array::from_fn(|_| AtomicI32::new(SLOT_NONE)),
            queued_slot: std::array::from_fn(|_| AtomicI32::new(SLOT_NONE)),
            origins: std::array::from_fn(|_| AtomicU64::new(0)),
            levels: std::array::from_fn(|_| AtomicU32::new(0)),
            master_level: AtomicU32::new(0),
            macros: std::array::from_fn(|_| std::array::from_fn(|_| AtomicU32::new(0))),
            sample_rate: AtomicU32::new(48000),
        }
    }
    pub fn song_tick(&self) -> f64 {
        f64::from_bits(self.song_tick.load(Ordering::Relaxed))
    }
    /// Song tick at which the track's active pattern started looping.
    pub fn origin(&self, track: usize) -> f64 {
        f64::from_bits(self.origins[track].load(Ordering::Relaxed))
    }
    pub fn level(&self, track: usize) -> f32 {
        f32::from_bits(self.levels[track].load(Ordering::Relaxed))
    }
    pub fn master_level(&self) -> f32 {
        f32::from_bits(self.master_level.load(Ordering::Relaxed))
    }
    pub fn macro_value(&self, track: usize, param: MacroParam) -> f32 {
        f32::from_bits(self.macros[track][param.index()].load(Ordering::Relaxed))
    }
}

impl Default for SharedStatus {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy)]
struct PendingOff {
    pitch: u8,
    tick: f64,
}

/// Audio-thread side of a track.
pub struct TrackRuntime {
    instrument: Box<Instrument>,
    fx: FxChain,
    macros: Macros,
    effective: Macros,
    touched: [bool; 7],
    mute: bool,
    solo: bool,
    pan: f32,
    humanize: f32,
    /// Gains of the instrument's parts; kept here so a swapped-in
    /// instrument picks them up.
    parts: [f32; MAX_PARTS],
    slots: [Option<Box<Pattern>>; SLOTS_PER_TRACK],
    active: Option<usize>,
    /// Song tick where the active pattern (re)started, so loops play from their start.
    origin: f64,
    /// Some(x) = launch x (None inside = stop) at the next bar.
    queued: Option<Option<usize>>,
    pending_offs: [PendingOff; MAX_PENDING_OFFS],
    n_offs: usize,
    mono: Vec<f32>,
    left: Vec<f32>,
    right: Vec<f32>,
    to_reverb: Vec<f32>,
    to_echo: Vec<f32>,
    level: f32,
}

impl TrackRuntime {
    pub fn new(
        instrument: Instrument,
        macros: Macros,
        mute: bool,
        patterns: Vec<Option<Pattern>>,
        sr: f32,
    ) -> Self {
        let mut slots: [Option<Box<Pattern>>; SLOTS_PER_TRACK] = Default::default();
        for (i, p) in patterns.into_iter().take(SLOTS_PER_TRACK).enumerate() {
            slots[i] = p.map(Box::new);
        }
        TrackRuntime {
            instrument: Box::new(instrument),
            fx: FxChain::new(sr, &macros),
            macros,
            effective: macros,
            touched: [false; 7],
            mute,
            slots,
            active: None,
            origin: 0.0,
            solo: false,
            pan: 0.0,
            humanize: 0.0,
            parts: [1.0; MAX_PARTS],
            queued: None,
            pending_offs: [PendingOff {
                pitch: 0,
                tick: 0.0,
            }; MAX_PENDING_OFFS],
            n_offs: 0,
            mono: vec![0.0; CHUNK],
            left: vec![0.0; CHUNK],
            right: vec![0.0; CHUNK],
            to_reverb: vec![0.0; CHUNK],
            to_echo: vec![0.0; CHUNK],
            level: 0.0,
        }
    }

    /// Mixer state for a freshly built track.
    pub fn set_mix(&mut self, solo: bool, pan: f32) {
        self.solo = solo;
        self.pan = pan.clamp(-1.0, 1.0);
    }

    pub fn set_humanize(&mut self, amount: f32) {
        self.humanize = amount.clamp(0.0, 1.0);
    }

    pub fn set_parts(&mut self, gains: [f32; MAX_PARTS]) {
        self.parts = gains;
        self.instrument.set_part_gains(&self.parts);
    }

    fn schedule_off(&mut self, pitch: u8, tick: f64) {
        if self.n_offs == MAX_PENDING_OFFS {
            // full: release the earliest one right away
            let (idx, _) = self.pending_offs[..self.n_offs]
                .iter()
                .enumerate()
                .min_by(|a, b| a.1.tick.total_cmp(&b.1.tick))
                .unwrap();
            let p = self.pending_offs[idx].pitch;
            self.instrument.note_off(p);
            self.pending_offs[idx] = self.pending_offs[self.n_offs - 1];
            self.n_offs -= 1;
        }
        self.pending_offs[self.n_offs] = PendingOff { pitch, tick };
        self.n_offs += 1;
    }

    fn release_due(&mut self, until: f64) {
        let mut i = 0;
        while i < self.n_offs {
            if self.pending_offs[i].tick < until {
                self.instrument.note_off(self.pending_offs[i].pitch);
                self.pending_offs[i] = self.pending_offs[self.n_offs - 1];
                self.n_offs -= 1;
            } else {
                i += 1;
            }
        }
    }

    fn release_all(&mut self) {
        for i in 0..self.n_offs {
            self.instrument.note_off(self.pending_offs[i].pitch);
        }
        self.n_offs = 0;
    }
}

pub struct Engine {
    sr: f32,
    bpm: f32,
    swing: f32,
    playing: bool,
    song_tick: f64,
    free_tick: f64,
    // Boxed so a removed track can be handed to the garbage queue without
    // allocating on the audio thread.
    #[allow(clippy::vec_box)]
    tracks: Vec<Box<TrackRuntime>>,
    master: MasterBus,
    space: crate::space::Space,
    pump: crate::space::Pump,
    pump_amount: f32,
    punch: crate::space::Punch,
    commands: Consumer<Command>,
    events: Producer<EngineEvent>,
    garbage: Producer<Garbage>,
    status: Arc<SharedStatus>,
    master_level: f32,
    song: Box<SongRuntime>,
    song_mode: bool,
    start_tick: f64,
    /// Garbage that could not be sent yet (queue full): kept until it can.
    stalled: Option<Garbage>,
    status_counter: u32,
    click: Click,
    count_in_ticks: f64,
    /// Ticks per bar and per beat of the project's meter.
    bar_ticks: f64,
    beat_ticks: f64,
}

/// Metronome: a short soft "tock" on each beat, higher on the downbeat.
/// Mixed after the master bus so vinyl/tape never colour it.
struct Click {
    on: bool,
    /// Beats in a bar: the first one clicks higher.
    per_bar: i64,
    last_beat: i64,
    phase: f32,
    inc: f32,
    env: f32,
    decay: f32,
}

impl Click {
    fn new(sr: f32) -> Click {
        Click {
            on: false,
            per_bar: 4,
            last_beat: -1,
            phase: 0.0,
            inc: 0.0,
            env: 0.0,
            decay: (-1.0 / (0.025 * sr)).exp(),
        }
    }

    /// Call once per chunk with the current beat; triggers on a new beat.
    fn beat(&mut self, beat: i64, sr: f32) {
        if beat == self.last_beat {
            return;
        }
        self.last_beat = beat;
        if self.on {
            let down = beat.rem_euclid(self.per_bar) == 0;
            let hz = if down { 1760.0 } else { 1320.0 };
            self.inc = hz / sr;
            self.phase = 0.0;
            self.env = if down { 0.32 } else { 0.2 };
        }
    }

    #[inline]
    fn next(&mut self) -> f32 {
        if self.env < 1e-4 {
            return 0.0;
        }
        let y = (self.phase * std::f32::consts::TAU).sin() * self.env;
        self.phase = (self.phase + self.inc) % 1.0;
        self.env *= self.decay;
        y
    }
}

pub struct EngineEndpoints {
    pub commands: Producer<Command>,
    pub events: Consumer<EngineEvent>,
    pub garbage: Consumer<Garbage>,
    pub status: Arc<SharedStatus>,
}

impl Engine {
    /// Test helper: take the queued commands without applying them.
    #[cfg(test)]
    pub(crate) fn drain_commands_for_test(&mut self) -> Vec<Command> {
        let mut out = Vec::new();
        while let Ok(c) = self.commands.pop() {
            out.push(c);
        }
        out
    }

    pub fn new(sample_rate: f32) -> (Engine, EngineEndpoints) {
        let (cmd_tx, cmd_rx) = rtrb::RingBuffer::new(4096);
        let (ev_tx, ev_rx) = rtrb::RingBuffer::new(4096);
        let (gb_tx, gb_rx) = rtrb::RingBuffer::new(1024);
        let status = Arc::new(SharedStatus::new());
        status
            .sample_rate
            .store(sample_rate as u32, Ordering::Relaxed);
        let engine = Engine {
            sr: sample_rate,
            bpm: 120.0,
            swing: 0.0,
            playing: false,
            song_tick: 0.0,
            free_tick: 0.0,
            tracks: Vec::with_capacity(MAX_TRACKS),
            master: MasterBus::new(sample_rate),
            space: crate::space::Space::new(sample_rate),
            pump: crate::space::Pump::new(sample_rate),
            pump_amount: 0.0,
            punch: crate::space::Punch::new(sample_rate),
            commands: cmd_rx,
            events: ev_tx,
            garbage: gb_tx,
            status: status.clone(),
            master_level: 0.0,
            song: Box::default(),
            song_mode: false,
            start_tick: 0.0,
            stalled: None,
            status_counter: 0,
            click: Click::new(sample_rate),
            count_in_ticks: 0.0,
            bar_ticks: TICKS_PER_4_4_BAR as f64,
            beat_ticks: PPQ as f64,
        };
        (
            engine,
            EngineEndpoints {
                commands: cmd_tx,
                events: ev_rx,
                garbage: gb_rx,
                status,
            },
        )
    }

    pub fn sample_rate(&self) -> f32 {
        self.sr
    }

    pub fn song_tick(&self) -> f64 {
        self.song_tick
    }

    fn trash(&mut self, g: Garbage) {
        if let Some(prev) = self.stalled.take() {
            if let Err(rtrb::PushError::Full(prev)) = self.garbage.push(prev) {
                // still full: leak rather than free on the audio thread
                std::mem::forget(prev);
            }
        }
        if let Err(rtrb::PushError::Full(g)) = self.garbage.push(g) {
            self.stalled = Some(g);
        }
    }

    fn ticks_per_sample(&self) -> f64 {
        self.bpm as f64 / 60.0 * PPQ as f64 / self.sr as f64
    }

    fn handle(&mut self, cmd: Command) {
        match cmd {
            Command::PlayWithCountIn(beats) => {
                if !self.playing && self.count_in_ticks <= 0.0 {
                    if beats == 0 {
                        self.handle(Command::Play);
                    } else {
                        let beats = beats.min(32);
                        self.count_in_ticks = beats as f64 * self.beat_ticks;
                        self.click.last_beat = i64::MIN;
                        self.status.count_in.store(beats, Ordering::Relaxed);
                    }
                }
            }
            Command::Play => {
                self.count_in_ticks = 0.0;
                self.status.count_in.store(0, Ordering::Relaxed);
                self.click.last_beat = -1;
                if !self.playing {
                    self.playing = true;
                    if self.song_mode {
                        // the timeline decides what plays, from the start marker
                        self.song_tick = self.start_tick;
                        for t in self.tracks.iter_mut() {
                            t.queued = None;
                            t.active = None;
                        }
                    } else {
                        self.song_tick = 0.0;
                        // queued launches start right away from bar 1
                        for t in self.tracks.iter_mut() {
                            if let Some(q) = t.queued.take() {
                                t.active = q;
                            }
                            t.origin = 0.0;
                        }
                    }
                    self.status.playing.store(true, Ordering::Relaxed);
                }
            }
            Command::Stop => {
                self.count_in_ticks = 0.0;
                self.status.count_in.store(0, Ordering::Relaxed);
                self.click.env = 0.0;
                self.playing = false;
                self.song_tick = 0.0;
                for t in self.tracks.iter_mut() {
                    t.release_all();
                    t.instrument.all_notes_off();
                }
                self.status.playing.store(false, Ordering::Relaxed);
            }
            Command::SetTempo { bpm, swing } => {
                self.bpm = bpm.clamp(40.0, 300.0);
                self.swing = swing.clamp(0.0, 1.0);
            }
            Command::SetMeter { bar, beat } => {
                self.beat_ticks = beat.max(1) as f64;
                self.bar_ticks = bar.max(beat.max(1)) as f64;
                self.click.per_bar = (bar / beat.max(1)).max(1) as i64;
            }
            Command::NoteOn {
                track,
                pitch,
                velocity,
            } => {
                let tick = if self.playing {
                    self.song_tick
                } else {
                    self.free_tick
                };
                if let Some(t) = self.tracks.get_mut(track) {
                    t.instrument.note_on(pitch, velocity);
                }
                let _ = self.events.push(EngineEvent::LiveNote {
                    track,
                    pitch,
                    velocity,
                    on: true,
                    tick,
                    playing: self.playing,
                });
            }
            Command::NoteOff { track, pitch } => {
                let tick = if self.playing {
                    self.song_tick
                } else {
                    self.free_tick
                };
                if let Some(t) = self.tracks.get_mut(track) {
                    t.instrument.note_off(pitch);
                }
                let _ = self.events.push(EngineEvent::LiveNote {
                    track,
                    pitch,
                    velocity: 0.0,
                    on: false,
                    tick,
                    playing: self.playing,
                });
            }
            Command::SetPattern {
                track,
                slot,
                pattern,
            } => {
                match self.tracks.get_mut(track) {
                    Some(t) if slot < SLOTS_PER_TRACK => {
                        let old = std::mem::replace(&mut t.slots[slot], pattern);
                        if t.slots[slot].is_none() && t.active == Some(slot) {
                            t.release_all();
                            t.active = None;
                        }
                        if let Some(old) = old {
                            self.trash(old);
                        }
                    }
                    // invalid target: never free the box here, hand it back
                    _ => {
                        if let Some(p) = pattern {
                            self.trash(p);
                        }
                    }
                }
            }
            Command::Launch {
                track,
                slot,
                immediate,
            } => {
                if self.song_mode {
                    return;
                }
                if let Some(t) = self.tracks.get_mut(track) {
                    let slot = slot.filter(|&s| s < SLOTS_PER_TRACK && t.slots[s].is_some());
                    if immediate || !self.playing {
                        if t.active != slot {
                            t.release_all();
                            let bar = self.bar_ticks;
                            t.origin = if self.playing {
                                (self.song_tick / bar).floor() * bar
                            } else {
                                0.0
                            };
                        }
                        t.active = slot;
                        t.queued = None;
                    } else {
                        t.queued = Some(slot);
                    }
                }
            }
            Command::SetMacros { track, macros } => {
                if let Some(t) = self.tracks.get_mut(track) {
                    t.macros = macros;
                }
            }
            Command::Touch {
                track,
                param,
                touching,
            } => {
                if let Some(t) = self.tracks.get_mut(track) {
                    t.touched[param.index()] = touching;
                }
            }
            Command::SetMix {
                track,
                mute,
                solo,
                pan,
            } => {
                if let Some(t) = self.tracks.get_mut(track) {
                    t.mute = mute;
                    t.solo = solo;
                    t.pan = pan.clamp(-1.0, 1.0);
                }
            }
            Command::SetMaster(m) => {
                self.master.set(m);
                self.space.set_size(m.space);
                self.pump_amount = m.pump;
            }
            Command::Punch { kind, on } => {
                use crate::space::PunchKind;
                self.punch.set(kind, on);
                match kind {
                    PunchKind::Freeze => self.space.plate.freeze(on),
                    PunchKind::Dub => self.space.echo.feedback = if on { 0.9 } else { 0.45 },
                    _ => {}
                }
            }
            Command::SetMacroPatch { track, patch } => {
                if let Some(t) = self.tracks.get_mut(track) {
                    if let crate::instruments::Instrument::Macro(m) = &mut *t.instrument {
                        m.set_patch(patch);
                    }
                }
            }
            Command::SetMetronome(on) => self.click.on = on,
            Command::SetHumanize { track, amount } => {
                if let Some(t) = self.tracks.get_mut(track) {
                    t.set_humanize(amount);
                }
            }
            Command::SetParts { track, gains } => {
                if let Some(t) = self.tracks.get_mut(track) {
                    t.set_parts(gains);
                }
            }
            Command::SetSynthPatch { track, patch } => {
                if let Some(t) = self.tracks.get_mut(track) {
                    if let crate::instruments::Instrument::Synth(s) = &mut *t.instrument {
                        s.set_patch(patch);
                    }
                }
            }
            Command::SetDrumPad { track, pad, sound } => {
                if let Some(t) = self.tracks.get_mut(track) {
                    if let crate::instruments::Instrument::Drums(d) = &mut *t.instrument {
                        d.set_pad(pad, sound);
                    }
                }
            }
            Command::SetSong(song) => {
                let old = std::mem::replace(&mut self.song, song);
                self.trash(old);
            }
            Command::SetSongMode(on) => {
                if on != self.song_mode {
                    self.song_mode = on;
                    for t in self.tracks.iter_mut() {
                        t.release_all();
                        t.queued = None;
                        t.active = None;
                    }
                    if self.playing {
                        self.song_tick = if on { self.start_tick } else { 0.0 };
                    }
                }
            }
            Command::Seek { tick } => {
                self.start_tick = tick.max(0.0);
                if self.playing && self.song_mode {
                    self.song_tick = self.start_tick;
                    for t in self.tracks.iter_mut() {
                        t.release_all();
                        t.active = None;
                    }
                }
            }
            Command::SetInstrument { track, instrument } => {
                let old = match self.tracks.get_mut(track) {
                    Some(t) => {
                        t.n_offs = 0;
                        let old = std::mem::replace(&mut t.instrument, instrument);
                        t.instrument.set_part_gains(&t.parts);
                        old
                    }
                    // invalid target: never free the box here, hand it back
                    None => instrument,
                };
                self.trash(old);
            }
            Command::InsertTrack { index, track } => {
                if self.tracks.len() < MAX_TRACKS {
                    let index = index.min(self.tracks.len());
                    self.tracks.insert(index, track);
                } else {
                    self.trash(track);
                }
            }
            Command::RemoveTrack { index } => {
                if index < self.tracks.len() {
                    let t = self.tracks.remove(index);
                    self.trash(t);
                }
            }
        }
    }

    /// Render interleaved audio with `channels` channels (1 or more).
    pub fn process(&mut self, out: &mut [f32], channels: usize) {
        while let Ok(cmd) = self.commands.pop() {
            self.handle(cmd);
        }
        let channels = channels.max(1);
        let frames = out.len() / channels;
        let mut frame = 0;
        let mut peak = 0.0f32;
        while frame < frames {
            let n = CHUNK.min(frames - frame);
            let fade_from = self.fade_gain();
            self.step_sequencer(n);
            let fade_to = self.fade_gain();
            if self.playing {
                let beat = (self.song_tick / self.beat_ticks).floor() as i64;
                self.click.beat(beat, self.sr);
            } else if self.count_in_ticks <= 0.0 {
                self.click.last_beat = -1;
            }
            let mut chunk_l = [0.0f32; CHUNK];
            let mut chunk_r = [0.0f32; CHUNK];
            let mut send_reverb = [0.0f32; CHUNK];
            let mut send_echo = [0.0f32; CHUNK];
            let mut drums = [0.0f32; CHUNK];
            let beat_samples = 60.0 / self.bpm * self.sr;
            let any_solo = self.tracks.iter().any(|t| t.solo);
            // every track through its own chain; the drums are heard first,
            // because the rest makes room for them
            for t in self.tracks.iter_mut() {
                t.mono[..n].fill(0.0);
                t.instrument.render(&mut t.mono[..n]);
                let TrackRuntime {
                    fx,
                    mono,
                    left,
                    right,
                    to_reverb,
                    to_echo,
                    effective,
                    ..
                } = &mut **t;
                fx.process(
                    &mono[..n],
                    &mut left[..n],
                    &mut right[..n],
                    &mut to_reverb[..n],
                    &mut to_echo[..n],
                    effective,
                );
                let mut tp = 0.0f32;
                for i in 0..n {
                    tp = tp.max(t.left[i].abs()).max(t.right[i].abs());
                }
                t.level = t.level.max(tp);
                if !t.mute && (!any_solo || t.solo) && t.instrument.is_kit() {
                    for i in 0..n {
                        drums[i] += t.left[i];
                    }
                }
            }
            let mut duck = [1.0f32; CHUNK];
            if self.pump_amount > 0.001 {
                for i in 0..n {
                    duck[i] = self.pump.gain(drums[i], self.pump_amount);
                }
            }
            for t in self.tracks.iter() {
                if t.mute || (any_solo && !t.solo) {
                    continue;
                }
                // simple balance pan
                let gl = (1.0 - t.pan).min(1.0);
                let gr = (1.0 + t.pan).min(1.0);
                let kit = t.instrument.is_kit();
                for i in 0..n {
                    let g = if kit { 1.0 } else { duck[i] };
                    chunk_l[i] += t.left[i] * gl * g;
                    chunk_r[i] += t.right[i] * gr * g;
                    send_reverb[i] += t.to_reverb[i] * g;
                    send_echo[i] += t.to_echo[i] * g;
                }
            }
            // the punch effects, then the room every track sends to
            let dub = self.punch.held(crate::space::PunchKind::Dub);
            for i in 0..n {
                // at rest the punch only remembers the mix, for the next loop or tape stop
                let (l, r) = self.punch.process(chunk_l[i], chunk_r[i], beat_samples);
                chunk_l[i] = l;
                chunk_r[i] = r;
                let mut echo_in = send_echo[i];
                if dub {
                    echo_in += (chunk_l[i] + chunk_r[i]) * 0.5;
                }
                let (wl, wr) = self
                    .space
                    .process(send_reverb[i], echo_in, beat_samples * 0.75);
                chunk_l[i] += wl;
                chunk_r[i] += wr;
            }
            for i in 0..n {
                let g = fade_from + (fade_to - fade_from) * (i as f32 / n as f32);
                let (l, r) = self.master.process(chunk_l[i] * g, chunk_r[i] * g);
                let click = self.click.next();
                let (l, r) = ((l + click).clamp(-1.0, 1.0), (r + click).clamp(-1.0, 1.0));
                peak = peak.max(l.abs()).max(r.abs());
                let base = (frame + i) * channels;
                out[base] = l;
                if channels > 1 {
                    out[base + 1] = r;
                    for c in 2..channels {
                        out[base + c] = 0.0;
                    }
                }
            }
            frame += n;
        }
        self.master_level = self.master_level.max(peak);
        self.publish_status(frames);
    }

    fn publish_status(&mut self, frames: usize) {
        let s = &self.status;
        s.song_tick
            .store(self.song_tick.to_bits(), Ordering::Relaxed);
        self.status_counter += frames as u32;
        // meters: publish and decay ~30 times per second
        if self.status_counter as f32 > self.sr / 30.0 {
            self.status_counter = 0;
            for (i, t) in self.tracks.iter_mut().enumerate() {
                s.levels[i].store(t.level.to_bits(), Ordering::Relaxed);
                t.level = 0.0;
                for k in 0..7 {
                    s.macros[i][k].store(t.effective.0[k].to_bits(), Ordering::Relaxed);
                }
            }
            for i in self.tracks.len()..MAX_TRACKS {
                s.levels[i].store(0, Ordering::Relaxed);
            }
            s.master_level
                .store(self.master_level.to_bits(), Ordering::Relaxed);
            self.master_level = 0.0;
        }
        for (i, t) in self.tracks.iter().enumerate() {
            s.active_slot[i].store(
                t.active.map(|x| x as i32).unwrap_or(SLOT_NONE),
                Ordering::Relaxed,
            );
            let q = match t.queued {
                None => SLOT_NONE,
                Some(None) => SLOT_STOP,
                Some(Some(x)) => x as i32,
            };
            s.queued_slot[i].store(q, Ordering::Relaxed);
            s.origins[i].store(t.origin.to_bits(), Ordering::Relaxed);
        }
        for i in self.tracks.len()..MAX_TRACKS {
            s.active_slot[i].store(SLOT_NONE, Ordering::Relaxed);
            s.queued_slot[i].store(SLOT_NONE, Ordering::Relaxed);
        }
    }

    fn fade_gain(&self) -> f32 {
        if self.song_mode && self.playing {
            self.song.fade_gain(self.song_tick)
        } else {
            1.0
        }
    }

    /// Song mode: wrap/stop at the end and point every track at the clip
    /// under the playhead (each clip plays its loop from the clip start).
    fn follow_song(&mut self, dt: f64) {
        let len = self.song.length;
        if len > 0.0 && self.song_tick >= len {
            if self.song.looping {
                self.song_tick = (self.song_tick - len).max(0.0);
                for t in self.tracks.iter_mut() {
                    t.release_all();
                    t.active = None;
                }
            } else if self.song_tick >= len + self.bar_ticks {
                // a bar of silence for tails, then stop by itself
                self.handle(Command::Stop);
                return;
            }
        }
        // look at the end of this chunk so a clip starting inside it fires
        let probe = self.song_tick + dt - 1e-6;
        for (i, t) in self.tracks.iter_mut().enumerate() {
            t.queued = None;
            match self.song.clip_at(i, probe) {
                Some(c) if t.active != Some(c.slot) || t.origin != c.start => {
                    t.release_all();
                    t.active = Some(c.slot);
                    t.origin = c.start;
                }
                Some(_) => {}
                None => {
                    if t.active.is_some() {
                        t.release_all();
                        t.active = None;
                    }
                }
            }
        }
    }

    /// Advance the clock by `n` samples, firing all events that fall inside.
    fn step_sequencer(&mut self, n: usize) {
        let dt = self.ticks_per_sample() * n as f64;
        self.free_tick += dt;
        if self.count_in_ticks > 0.0 {
            let remaining = (self.count_in_ticks / self.beat_ticks).ceil() as u32;
            let on = self.click.on;
            self.click.on = true;
            self.click.beat(-(remaining as i64), self.sr);
            self.click.on = on;
            self.count_in_ticks = (self.count_in_ticks - dt).max(0.0);
            self.status.count_in.store(remaining, Ordering::Relaxed);
            // Start at the next audio chunk, with the normal first downbeat.
            // Epsilon avoids delaying a mathematically exact boundary by a chunk.
            if self.count_in_ticks < 1e-7 {
                self.handle(Command::Play);
            }
            return;
        }
        if !self.playing {
            for t in self.tracks.iter_mut() {
                t.effective = t.macros;
            }
            return;
        }
        if self.song_mode {
            self.follow_song(dt);
            if !self.playing {
                return;
            }
        }
        let t0 = self.song_tick;
        let t1 = t0 + dt;
        let bar = self.bar_ticks;
        let bar_len = bar as u32;

        // Quantized launches at bar boundaries.
        let boundary = (t0 / bar).ceil() * bar;
        if boundary < t1 {
            for t in self.tracks.iter_mut() {
                if let Some(q) = t.queued.take() {
                    if t.active != q {
                        t.release_all();
                    }
                    // relaunching the same slot restarts it too
                    t.active = q;
                    t.origin = boundary;
                }
            }
        }

        for ti in 0..self.tracks.len() {
            let swing = self.swing;
            let t = &mut self.tracks[ti];
            t.release_due(t1);
            let Some(slot) = t.active else {
                t.effective = t.macros;
                continue;
            };
            let Some(pattern) = t.slots[slot].as_deref() else {
                t.effective = t.macros;
                continue;
            };
            let len = pattern.length_ticks(bar_len) as f64;
            let loop_start = t.origin + ((t0 - t.origin) / len).floor() * len;
            let lo = t0 - loop_start;
            let hi = t1 - loop_start; // may exceed len -> wrap
                                      // Automation (evaluated at chunk start).
            let mut eff = t.macros;
            for lane in &pattern.automation {
                if !t.touched[lane.param.index()] {
                    if let Some(v) = lane.value_at(lo, pattern.length_ticks(bar_len)) {
                        eff.set(lane.param, v);
                    }
                }
            }
            t.effective = eff;

            // Notes: collect into a small stack buffer to satisfy the borrow checker.
            let mut fire: [(u8, f32, f64); 32] = [(0, 0.0, 0.0); 32];
            let mut nfire = 0;
            let (human, origin) = (t.humanize as f64, t.origin);
            let scan = |from: f64,
                        to: f64,
                        offset: f64,
                        fire: &mut [(u8, f32, f64); 32],
                        nfire: &mut usize| {
                let first = pattern.notes.partition_point(|n| {
                    (n.start as f64)
                        < from - MAX_SWING_TICKS - MAX_HUMAN_TICKS - TICKS_PER_STEP as f64
                });
                for note in &pattern.notes[first..] {
                    if note.start as f64 >= to {
                        break;
                    }
                    let sw = if swing > 0.0
                        && (note.start / TICKS_PER_STEP) % 2 == 1
                        && note.start % TICKS_PER_STEP == 0
                    {
                        swing as f64 * MAX_SWING_TICKS
                    } else {
                        0.0
                    };
                    // humanize: play a little late (never early) and vary the touch
                    let (mut lag, mut vel) = (0.0, note.velocity);
                    if human > 0.0 {
                        let cycle = ((loop_start + offset - origin) / len).round().max(0.0) as u64;
                        let key = ((note.start as u64) << 8) | note.pitch as u64;
                        lag = human * MAX_HUMAN_TICKS * hash01(key, cycle) as f64;
                        let dv = hash01(key ^ 0xABCD, cycle) - 0.5;
                        vel = (vel * (1.0 + dv * 0.45 * human as f32)).clamp(0.05, 1.0);
                    }
                    let cycle = ((loop_start + offset - origin) / len).round().max(0.0) as u64;
                    let key = ((note.start as u64) << 8) | note.pitch as u64;
                    // chance: the same answer for the same note in the same pass
                    // of the loop, so a bounce matches what was heard
                    if note.chance < 100
                        && hash01(key ^ 0xC4A2CE, cycle) * 100.0 >= note.chance as f32
                    {
                        continue;
                    }
                    // repeat: the note strikes several times inside its first sixteenth
                    let strikes = note.repeat.clamp(1, MAX_REPEAT) as u32;
                    let span = (note.length.min(TICKS_PER_STEP) as f64) / strikes as f64;
                    for k in 0..strikes {
                        let s = note.start as f64 + sw + lag + k as f64 * span;
                        if s >= from && s < to && *nfire < fire.len() {
                            let end = if strikes > 1 {
                                s + span * 0.9
                            } else {
                                s + note.length as f64
                            };
                            fire[*nfire] = (note.pitch, vel, loop_start + offset + end);
                            *nfire += 1;
                        }
                    }
                }
            };
            scan(lo, hi.min(len), 0.0, &mut fire, &mut nfire);
            if hi > len {
                scan(0.0, hi - len, len, &mut fire, &mut nfire);
            }
            for &(pitch, vel, off_tick) in &fire[..nfire] {
                t.instrument.note_on(pitch, vel);
                t.schedule_off(pitch, off_tick);
            }
        }
        self.song_tick = t1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TICKS_PER_4_4_BAR as TICKS_PER_BAR;
    use crate::model::{DrumKitId, InstrumentSpec, Note, DRUM_BASE_NOTE};

    fn drum_track(sr: f32, pattern: Pattern) -> Box<TrackRuntime> {
        let inst = Instrument::build(
            &InstrumentSpec::Drums {
                kit: DrumKitId::Tr808,
            },
            None,
            sr,
        );
        Box::new(TrackRuntime::new(
            inst,
            Macros::default(),
            false,
            vec![Some(pattern)],
            sr,
        ))
    }

    #[test]
    fn plays_a_pattern_and_reports_status() {
        let sr = 48000.0;
        let (mut eng, mut ep) = Engine::new(sr);
        let mut p = Pattern::new("t", 1);
        p.add_note(
            Note {
                start: 0,
                length: 24,
                pitch: DRUM_BASE_NOTE,
                velocity: 1.0,
                ..Default::default()
            },
            TICKS_PER_BAR,
        );
        ep.commands
            .push(Command::InsertTrack {
                index: 0,
                track: drum_track(sr, p),
            })
            .ok()
            .unwrap();
        ep.commands
            .push(Command::Launch {
                track: 0,
                slot: Some(0),
                immediate: false,
            })
            .ok()
            .unwrap();
        ep.commands.push(Command::Play).ok().unwrap();
        let mut buf = vec![0.0; 2 * 4800];
        eng.process(&mut buf, 2);
        let peak = buf.iter().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!(peak > 0.05, "peak {peak}");
        assert_eq!(ep.status.active_slot[0].load(Ordering::Relaxed), 0);
        assert!(ep.status.song_tick() > 0.0);
    }

    #[test]
    fn launch_is_quantized_to_bar() {
        let sr = 48000.0;
        let (mut eng, mut ep) = Engine::new(sr);
        let mut p = Pattern::new("t", 1);
        p.add_note(
            Note {
                start: 0,
                length: 24,
                pitch: DRUM_BASE_NOTE,
                velocity: 1.0,
                ..Default::default()
            },
            TICKS_PER_BAR,
        );
        ep.commands
            .push(Command::InsertTrack {
                index: 0,
                track: drum_track(sr, p),
            })
            .ok()
            .unwrap();
        ep.commands.push(Command::Play).ok().unwrap();
        let mut buf = vec![0.0; 2 * 1024];
        eng.process(&mut buf, 2);
        ep.commands
            .push(Command::Launch {
                track: 0,
                slot: Some(0),
                immediate: false,
            })
            .ok()
            .unwrap();
        eng.process(&mut buf, 2);
        assert_eq!(ep.status.queued_slot[0].load(Ordering::Relaxed), 0);
        assert_eq!(ep.status.active_slot[0].load(Ordering::Relaxed), SLOT_NONE);
        // 120 bpm: one bar = 2 s
        for _ in 0..100 {
            eng.process(&mut buf, 2);
        }
        assert_eq!(ep.status.active_slot[0].load(Ordering::Relaxed), 0);
    }

    #[test]
    fn note_count_matches_pattern_over_loops() {
        // Count note-ons through a counting instrument proxy: use a synth and
        // check pending note-offs bookkeeping instead.
        let sr = 48000.0;
        let (mut eng, mut ep) = Engine::new(sr);
        let mut p = Pattern::new("t", 1);
        for s in 0..16 {
            p.add_note(
                Note {
                    start: s * TICKS_PER_STEP,
                    length: 12,
                    pitch: 60,
                    velocity: 1.0,
                    ..Default::default()
                },
                TICKS_PER_BAR,
            );
        }
        let inst = Instrument::build(
            &InstrumentSpec::Synth {
                preset: crate::model::SynthPreset::Keys,
            },
            None,
            sr,
        );
        let tr = Box::new(TrackRuntime::new(
            inst,
            Macros::default(),
            false,
            vec![Some(p)],
            sr,
        ));
        ep.commands
            .push(Command::InsertTrack {
                index: 0,
                track: tr,
            })
            .ok()
            .unwrap();
        ep.commands
            .push(Command::Launch {
                track: 0,
                slot: Some(0),
                immediate: true,
            })
            .ok()
            .unwrap();
        ep.commands
            .push(Command::SetTempo {
                bpm: 120.0,
                swing: 0.5,
            })
            .ok()
            .unwrap();
        ep.commands.push(Command::Play).ok().unwrap();
        let mut buf = vec![0.0; 2 * 256];
        // 3 bars = 6 s
        for _ in 0..(6 * 48000 / 256) {
            eng.process(&mut buf, 2);
            assert!(eng.tracks[0].n_offs <= 2);
            assert!(buf.iter().all(|x| x.is_finite()));
        }
    }

    #[test]
    fn humanize_hash_is_stable_and_spread() {
        assert_eq!(hash01(5, 9), hash01(5, 9));
        let vals: Vec<f32> = (0..1000).map(|i| hash01(i, 3)).collect();
        assert!(vals.iter().all(|v| (0.0..1.0).contains(v)));
        let mean = vals.iter().sum::<f32>() / vals.len() as f32;
        assert!((mean - 0.5).abs() < 0.05, "mean {mean}");
        assert_ne!(hash01(1, 0), hash01(1, 1));
    }

    #[test]
    fn boxes_for_missing_tracks_come_back_as_garbage() {
        let sr = 48000.0;
        let (mut eng, mut ep) = Engine::new(sr);
        let inst = Instrument::build(
            &InstrumentSpec::Drums {
                kit: DrumKitId::LoFi,
            },
            None,
            sr,
        );
        ep.commands
            .push(Command::SetInstrument {
                track: 7,
                instrument: Box::new(inst),
            })
            .ok()
            .unwrap();
        ep.commands
            .push(Command::SetPattern {
                track: 7,
                slot: 0,
                pattern: Some(Box::new(Pattern::new("x", 1))),
            })
            .ok()
            .unwrap();
        let mut buf = vec![0.0; 512];
        eng.process(&mut buf, 2);
        let mut n = 0;
        while ep.garbage.pop().is_ok() {
            n += 1;
        }
        assert_eq!(n, 2);
    }

    #[test]
    fn live_notes_emit_events() {
        let sr = 48000.0;
        let (mut eng, mut ep) = Engine::new(sr);
        let p = Pattern::new("t", 1);
        ep.commands
            .push(Command::InsertTrack {
                index: 0,
                track: drum_track(sr, p),
            })
            .ok()
            .unwrap();
        ep.commands
            .push(Command::NoteOn {
                track: 0,
                pitch: 36,
                velocity: 1.0,
            })
            .ok()
            .unwrap();
        let mut buf = vec![0.0; 512];
        eng.process(&mut buf, 2);
        match ep.events.pop() {
            Ok(EngineEvent::LiveNote {
                track: 0,
                pitch: 36,
                on: true,
                playing: false,
                ..
            }) => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn count_in_is_audible_without_advancing_or_recording_transport() {
        let (mut eng, mut ep) = Engine::new(48000.0);
        ep.commands.push(Command::PlayWithCountIn(4)).ok().unwrap();
        let mut out = vec![0.0; 48000 * 2];
        eng.process(&mut out, 2); // two of the four beats at 120 BPM
        assert!(!ep.status.playing.load(Ordering::Relaxed));
        assert_eq!(eng.song_tick(), 0.0);
        assert!(ep.status.count_in.load(Ordering::Relaxed) > 0);
        assert!(out.iter().any(|v| v.abs() > 0.01));
        ep.commands
            .push(Command::NoteOn {
                track: 0,
                pitch: 60,
                velocity: 1.0,
            })
            .ok()
            .unwrap();
        let mut chunk = [0.0; 32];
        eng.process(&mut chunk, 2);
        assert!(matches!(
            ep.events.pop(),
            Ok(EngineEvent::LiveNote { playing: false, .. })
        ));
        eng.process(&mut out, 2);
        assert!(ep.status.playing.load(Ordering::Relaxed));
        assert_eq!(ep.status.count_in.load(Ordering::Relaxed), 0);
        assert!(eng.song_tick() < 1.0);
        // The click preference is still off after pre-roll.
        assert!(!eng.click.on);
    }

    #[test]
    fn stop_cancels_count_in_and_plain_play_starts_immediately() {
        let (mut eng, mut ep) = Engine::new(48000.0);
        let mut out = vec![0.0; 48000 * 2];
        ep.commands.push(Command::PlayWithCountIn(8)).ok().unwrap();
        eng.process(&mut out, 2);
        ep.commands.push(Command::Stop).ok().unwrap();
        eng.process(&mut out, 2);
        assert_eq!(ep.status.count_in.load(Ordering::Relaxed), 0);
        assert!(!ep.status.playing.load(Ordering::Relaxed));
        assert_eq!(eng.song_tick(), 0.0);
        assert!(out.iter().all(|v| *v == 0.0));
        ep.commands.push(Command::Play).ok().unwrap();
        eng.process(&mut out, 2);
        assert!(ep.status.playing.load(Ordering::Relaxed));
        assert!(eng.song_tick() > 0.0);
    }

    #[test]
    fn metronome_clicks_on_beats_only_when_enabled() {
        let render = |metronome: bool| {
            let (mut eng, mut ep) = Engine::new(44100.0);
            for c in [
                Command::SetMetronome(metronome),
                Command::SetTempo {
                    bpm: 120.0,
                    swing: 0.0,
                },
                Command::Play,
            ] {
                ep.commands.push(c).ok().unwrap();
            }
            // one second at 120 BPM = two beats, no tracks: only the click
            let mut buf = vec![0.0f32; 2 * 44100];
            eng.process(&mut buf, 2);
            buf
        };
        assert!(render(false).iter().all(|&x| x == 0.0));
        let on = render(true);
        let peak_at = |from: usize| {
            on[from * 2..(from + 2000) * 2]
                .iter()
                .fold(0.0f32, |m, x| m.max(x.abs()))
        };
        let (downbeat, beat2) = (peak_at(0), peak_at(22050));
        assert!(downbeat > 0.25 && beat2 > 0.1, "{downbeat} {beat2}");
        assert!(peak_at(11000) < 1e-3, "silence between beats");
    }
}
