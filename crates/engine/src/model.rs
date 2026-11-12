//! Project model: tracks, patterns (loops), scenes, automation and arrangement.
//! This is the authoritative, serializable state owned by the control thread.

use serde::{Deserialize, Serialize};

use crate::theory::{ChordKind, Scale};

/// Ticks per quarter note.
pub const PPQ: u32 = 96;
pub const TICKS_PER_STEP: u32 = PPQ / 4; // 1/16 note
/// Ticks in a 4/4 bar (the default meter).
pub const TICKS_PER_4_4_BAR: u32 = PPQ * 4;
pub const MAX_TRACKS: usize = 16;
/// Loop slots per track: the most sections (grid rows) a project can have.
/// How many rows are shown is the number of scenes.
pub const SLOTS_PER_TRACK: usize = 16;
pub const DRUM_BASE_NOTE: u8 = 36;
pub const DRUM_PADS: usize = 16;
/// Parts of an instrument mixed on their own: drum pads or synth layers.
pub const MAX_PARTS: usize = 16;
/// The level of a part that plays it as designed (the knob's 0..1 range
/// goes a little above it, for a boost).
pub const PART_UNITY: f32 = 0.8;

/// Gain of a part from its level (0..1, `PART_UNITY` = as designed), on a
/// gentle curve: half the knob is about -12 dB, the top about +4 dB.
pub fn part_gain(level: f32) -> f32 {
    let x = level.clamp(0.0, 1.0) / PART_UNITY;
    x * x
}

pub type SampleId = u32;

/// Time signature of the project: `beats` per bar of `unit` notes (4 = quarter,
/// 8 = eighth). Every bar is a whole number of 1/16 steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meter {
    pub beats: u32,
    pub unit: u32,
}

impl Default for Meter {
    fn default() -> Self {
        Meter { beats: 4, unit: 4 }
    }
}

impl Meter {
    /// The meters offered in the interface.
    pub const COMMON: [Meter; 9] = [
        Meter { beats: 4, unit: 4 },
        Meter { beats: 3, unit: 4 },
        Meter { beats: 2, unit: 4 },
        Meter { beats: 5, unit: 4 },
        Meter { beats: 6, unit: 4 },
        Meter { beats: 7, unit: 4 },
        Meter { beats: 6, unit: 8 },
        Meter { beats: 7, unit: 8 },
        Meter { beats: 12, unit: 8 },
    ];

    /// Bring any meter into range: 1..=16 beats of quarters, eighths or sixteenths.
    pub fn valid(self) -> Meter {
        let unit = match self.unit {
            8 => 8,
            16 => 16,
            _ => 4,
        };
        Meter {
            beats: self.beats.clamp(1, 16),
            unit,
        }
    }
    /// Ticks in one beat (one `unit` note).
    pub fn beat_ticks(self) -> u32 {
        PPQ * 4 / self.unit
    }
    pub fn bar_ticks(self) -> u32 {
        self.beats * self.beat_ticks()
    }
    /// 1/16 steps in a bar.
    pub fn steps(self) -> u32 {
        self.bar_ticks() / TICKS_PER_STEP
    }
    pub fn is_default(&self) -> bool {
        *self == Meter::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// Start in ticks from the pattern start.
    pub start: u32,
    /// Length in ticks.
    pub length: u32,
    pub pitch: u8,
    /// 0..1
    pub velocity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MacroParam {
    Cutoff,
    Resonance,
    Drive,
    Saturation,
    Reverb,
    Delay,
    Volume,
}

impl MacroParam {
    pub const ALL: [MacroParam; 7] = [
        MacroParam::Cutoff,
        MacroParam::Resonance,
        MacroParam::Drive,
        MacroParam::Saturation,
        MacroParam::Reverb,
        MacroParam::Delay,
        MacroParam::Volume,
    ];
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn label(self) -> &'static str {
        match self {
            MacroParam::Cutoff => "Filter",
            MacroParam::Resonance => "Resonance",
            MacroParam::Drive => "Drive",
            MacroParam::Saturation => "Saturation",
            MacroParam::Reverb => "Reverb",
            MacroParam::Delay => "Delay",
            MacroParam::Volume => "Volume",
        }
    }
}

/// Values 0..1 for each macro, indexed by `MacroParam::index`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Macros(pub [f32; 7]);

impl Default for Macros {
    fn default() -> Self {
        // cutoff open, little resonance, dry, unity-ish volume
        Macros([1.0, 0.1, 0.0, 0.0, 0.0, 0.0, 0.8])
    }
}

impl Macros {
    pub fn get(&self, p: MacroParam) -> f32 {
        self.0[p.index()]
    }
    pub fn set(&mut self, p: MacroParam, v: f32) {
        self.0[p.index()] = v.clamp(0.0, 1.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AutoPoint {
    pub tick: u32,
    pub value: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutomationLane {
    pub param: MacroParam,
    /// Sorted by tick.
    pub points: Vec<AutoPoint>,
}

impl AutomationLane {
    pub fn new(param: MacroParam) -> Self {
        AutomationLane {
            param,
            points: Vec::new(),
        }
    }

    /// Linear interpolation, holding first/last values at the edges and
    /// wrapping smoothly from the last point back to the first (loops).
    pub fn value_at(&self, tick: f64, loop_len: u32) -> Option<f32> {
        let pts = &self.points;
        match pts.len() {
            0 => None,
            1 => Some(pts[0].value),
            _ => {
                let first = pts[0];
                let last = pts[pts.len() - 1];
                if tick < first.tick as f64 || tick >= last.tick as f64 {
                    // wrap segment last -> first (+loop_len)
                    let span = (loop_len as f64 - last.tick as f64) + first.tick as f64;
                    if span <= 0.0 {
                        return Some(last.value);
                    }
                    let pos = if tick >= last.tick as f64 {
                        tick - last.tick as f64
                    } else {
                        tick + loop_len as f64 - last.tick as f64
                    };
                    let t = (pos / span).clamp(0.0, 1.0) as f32;
                    return Some(last.value + (first.value - last.value) * t);
                }
                // binary search the segment
                let idx = pts.partition_point(|p| (p.tick as f64) <= tick);
                let a = pts[idx - 1];
                let b = pts[idx];
                let span = (b.tick - a.tick) as f64;
                if span <= 0.0 {
                    return Some(b.value);
                }
                let t = ((tick - a.tick as f64) / span) as f32;
                Some(a.value + (b.value - a.value) * t)
            }
        }
    }

    /// Record a value at `tick`, overwriting any points recorded in
    /// (`prev_tick`, `tick`] during the same pass ("latch/overwrite" mode).
    pub fn write(&mut self, prev_tick: Option<u32>, tick: u32, value: f32) {
        if let Some(prev) = prev_tick {
            if prev < tick {
                self.points.retain(|p| p.tick <= prev || p.tick > tick);
            } else if prev > tick {
                // wrapped around the loop end
                self.points.retain(|p| p.tick <= prev && p.tick > tick);
            }
        }
        self.points.retain(|p| p.tick != tick);
        let idx = self.points.partition_point(|p| p.tick < tick);
        self.points.insert(
            idx,
            AutoPoint {
                tick,
                value: value.clamp(0.0, 1.0),
            },
        );
    }

    pub fn set_points(&mut self, mut points: Vec<AutoPoint>) {
        points.sort_by_key(|p| p.tick);
        points.dedup_by_key(|p| p.tick);
        for p in points.iter_mut() {
            p.value = p.value.clamp(0.0, 1.0);
        }
        self.points = points;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pattern {
    pub name: String,
    /// Loop length in bars: 1, 2, 4 or 8.
    pub bars: u32,
    /// Sorted by start tick.
    pub notes: Vec<Note>,
    pub automation: Vec<AutomationLane>,
}

impl Pattern {
    pub fn new(name: impl Into<String>, bars: u32) -> Self {
        Pattern {
            name: name.into(),
            bars: valid_bars(bars),
            notes: Vec::new(),
            automation: Vec::new(),
        }
    }

    /// Loop length for bars of `bar` ticks (`Meter::bar_ticks`).
    pub fn length_ticks(&self, bar: u32) -> u32 {
        self.bars * bar
    }

    pub fn sort(&mut self) {
        self.notes
            .sort_by(|a, b| a.start.cmp(&b.start).then(a.pitch.cmp(&b.pitch)));
    }

    pub fn add_note(&mut self, note: Note, bar: u32) {
        let len = self.length_ticks(bar);
        let mut n = note;
        n.start %= len;
        n.length = n.length.max(1);
        n.velocity = n.velocity.clamp(0.0, 1.0);
        self.notes
            .retain(|x| !(x.start == n.start && x.pitch == n.pitch));
        self.notes.push(n);
        self.sort();
    }

    pub fn remove_note(&mut self, start: u32, pitch: u8) -> bool {
        let before = self.notes.len();
        self.notes
            .retain(|x| !(x.start == start && x.pitch == pitch));
        before != self.notes.len()
    }

    /// Move/resize a note: the one at (`start`, `pitch`) becomes `note`.
    pub fn replace_note(&mut self, start: u32, pitch: u8, note: Note, bar: u32) -> bool {
        if !self.remove_note(start, pitch) {
            return false;
        }
        self.add_note(note, bar);
        true
    }

    /// Set the velocity of every note starting in `from..to` (a step: one
    /// note, or a whole chord / several drum hits).
    pub fn set_velocity(&mut self, from: u32, to: u32, velocity: f32) -> bool {
        let v = velocity.clamp(0.05, 1.0);
        let mut any = false;
        for n in self
            .notes
            .iter_mut()
            .filter(|n| n.start >= from && n.start < to)
        {
            n.velocity = v;
            any = true;
        }
        any
    }

    /// Toggle a note on the step grid. Returns true if the note now exists.
    /// A step holding several hits (a hi-hat roll) is cleared in one go.
    pub fn toggle_step(
        &mut self,
        step: u32,
        pitch: u8,
        velocity: f32,
        length: u32,
        bar: u32,
    ) -> bool {
        let start = step * TICKS_PER_STEP;
        let in_step = |n: &Note| n.pitch == pitch && n.start / TICKS_PER_STEP == step;
        if self.notes.iter().any(in_step) {
            self.notes.retain(|n| !in_step(n));
            false
        } else {
            self.add_note(
                Note {
                    start,
                    length,
                    pitch,
                    velocity,
                },
                bar,
            );
            true
        }
    }

    /// Change loop length; notes beyond the new end are dropped, shrinking
    /// keeps them, growing duplicates the content to fill the new length.
    pub fn set_bars(&mut self, bars: u32, bar: u32) {
        let bars = valid_bars(bars);
        if bars == self.bars {
            return;
        }
        let old_len = self.length_ticks(bar);
        if bars > self.bars {
            let copies = bars / self.bars;
            let orig = self.notes.clone();
            for c in 1..copies {
                for n in &orig {
                    self.notes.push(Note {
                        start: n.start + c * old_len,
                        ..*n
                    });
                }
            }
            for lane in &mut self.automation {
                let orig = lane.points.clone();
                for c in 1..copies {
                    for p in &orig {
                        lane.points.push(AutoPoint {
                            tick: p.tick + c * old_len,
                            value: p.value,
                        });
                    }
                }
            }
        }
        self.bars = bars;
        let len = self.length_ticks(bar);
        self.notes.retain(|n| n.start < len);
        for lane in &mut self.automation {
            lane.points.retain(|p| p.tick < len);
        }
        self.sort();
    }

    /// Quantize note starts to `grid` ticks with `strength` 0..1.
    pub fn quantize(&mut self, grid: u32, strength: f32, bar: u32) {
        let len = self.length_ticks(bar);
        for n in &mut self.notes {
            n.start = quantize_tick(n.start, grid, strength) % len;
        }
        // remove duplicates created by quantization
        self.sort();
        self.notes
            .dedup_by(|a, b| a.start == b.start && a.pitch == b.pitch);
    }

    pub fn lane_mut(&mut self, param: MacroParam) -> &mut AutomationLane {
        if let Some(i) = self.automation.iter().position(|l| l.param == param) {
            &mut self.automation[i]
        } else {
            self.automation.push(AutomationLane::new(param));
            self.automation.last_mut().unwrap()
        }
    }

    pub fn lane(&self, param: MacroParam) -> Option<&AutomationLane> {
        self.automation.iter().find(|l| l.param == param)
    }
}

pub fn quantize_tick(tick: u32, grid: u32, strength: f32) -> u32 {
    if grid == 0 {
        return tick;
    }
    let target = ((tick as f64 / grid as f64).round() * grid as f64) as i64;
    let moved = tick as f64 + (target as f64 - tick as f64) * strength.clamp(0.0, 1.0) as f64;
    moved.round().max(0.0) as u32
}

pub fn valid_bars(bars: u32) -> u32 {
    match bars {
        0 | 1 => 1,
        2 | 3 => 2,
        4..=7 => 4,
        _ => 8,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackKind {
    Drums,
    Melodic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrumKitId {
    Acoustic,
    Tr808,
    Tr909,
    LoFi,
    Modular,
    /// Sampled acoustic kit (CC0 recordings, see SAMPLES.md). Older project
    /// files call it "polvo".
    #[serde(alias = "polvo")]
    Dust,
    /// Trap drums through a dusty sampler: long 808 kick, snappy snare and
    /// clap, tight hats made for rolls.
    Trap,
    /// Sampled from lofisons' own dark tracks: sub kicks, hall snares, quiet hats.
    Shadow,
    /// Sampled from lofisons' modular sessions: tight kicks, snaps, ticks.
    Session,
}

impl DrumKitId {
    pub const ALL: [DrumKitId; 9] = [
        DrumKitId::Shadow,
        DrumKitId::Session,
        DrumKitId::Dust,
        DrumKitId::LoFi,
        DrumKitId::Trap,
        DrumKitId::Modular,
        DrumKitId::Acoustic,
        DrumKitId::Tr808,
        DrumKitId::Tr909,
    ];
    pub fn label(self) -> &'static str {
        match self {
            DrumKitId::Acoustic => "Acoustic",
            DrumKitId::Tr808 => "Boom",
            DrumKitId::Tr909 => "Club",
            DrumKitId::LoFi => "Lo-Fi",
            DrumKitId::Modular => "Modular",
            DrumKitId::Dust => "Dust (sampled)",
            DrumKitId::Trap => "Trap Lo-Fi",
            DrumKitId::Shadow => "Shadow",
            DrumKitId::Session => "Session",
        }
    }

    /// Kits played from recordings; the others are synthesized.
    pub fn sampled(self) -> bool {
        matches!(
            self,
            DrumKitId::Dust | DrumKitId::Shadow | DrumKitId::Session
        )
    }

    /// Pad names for the play surface and editor.
    pub fn pad_names(self) -> [&'static str; DRUM_PADS] {
        match self {
            DrumKitId::Dust => [
                "Kick",
                "Old snare",
                "Hand",
                "Tambourine",
                "Open tambourine",
                "Tumba",
                "Conga",
                "Quinto",
                "Rim",
                "Cowbell",
                "Cymbal",
                "Ride",
                "Guiro",
                "Kick 2",
                "Soft snare",
                "Claves",
            ],
            DrumKitId::Shadow => [
                "Kick",
                "Snare",
                "Hall snare",
                "Hat",
                "Open hat",
                "Deep kick",
                "Boom",
                "Thud",
                "Rim",
                "Knock",
                "Wash",
                "Tick",
                "Shaker",
                "Kick 2",
                "Snare 2",
                "Clack",
            ],
            DrumKitId::Session => [
                "Kick", "Snare", "Snap", "Hat", "Open hat", "Low tom", "Mid tom", "Knock", "Rim",
                "Blip", "Wash", "Tick", "Shaker", "Kick 2", "Snare 2", "Boom",
            ],
            DrumKitId::Trap => [
                "Kick",
                "Snare",
                "Clap",
                "Hat",
                "Open hat",
                "Low perc",
                "Mid perc",
                "High perc",
                "Rim",
                "Snap",
                "Crash",
                "Ride",
                "Shaker",
                "808 boom",
                "Rim snare",
                "Tick",
            ],
            DrumKitId::Modular => [
                "Kick",
                "Snare",
                "Clap",
                "Tick",
                "Open tick",
                "Low bongo",
                "Mid bongo",
                "High bongo",
                "Ping",
                "Ping FM",
                "Burst",
                "Metal",
                "Shaker",
                "Sub",
                "Wood",
                "Drop",
            ],
            _ => [
                "Kick",
                "Snare",
                "Clap",
                "Closed hat",
                "Open hat",
                "Low tom",
                "Mid tom",
                "High tom",
                "Rim",
                "Cowbell",
                "Crash",
                "Ride",
                "Shaker",
                "Kick 2",
                "Snare 2",
                "Clave",
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SynthPreset {
    Keys,
    Pluck,
    Bass,
    Pad,
    Lead,
    SubBass,
    Rhodes,
    FmBell,
    Mallet,
    /// Procedural felt piano (modal synthesis, no samples).
    Piano,
    /// Trap 808: a long sine boom with a punchy pitch drop, saturated so it
    /// carries on small speakers, sliding between overlapping notes.
    Bass808,
    /// One sub note sampled from lofisons' Shadow Ambient.
    ShadowSub,
    /// One sub note sampled from lofisons' Silent Focus.
    SilentSub,
    /// One bass note sampled from a lofisons modular session.
    SessionBass,
}

impl SynthPreset {
    /// Presets played from a recording of one note, with the note it holds.
    pub fn sampled(self) -> Option<(&'static str, u8)> {
        match self {
            SynthPreset::ShadowSub => Some(("bass_shadow", 32)),
            SynthPreset::SilentSub => Some(("bass_silent", 31)),
            SynthPreset::SessionBass => Some(("bass_session", 37)),
            _ => None,
        }
    }

    pub const ALL: [SynthPreset; 14] = [
        SynthPreset::ShadowSub,
        SynthPreset::SilentSub,
        SynthPreset::SessionBass,
        SynthPreset::Piano,
        SynthPreset::Rhodes,
        SynthPreset::FmBell,
        SynthPreset::Mallet,
        SynthPreset::Keys,
        SynthPreset::Pluck,
        SynthPreset::Bass,
        SynthPreset::Bass808,
        SynthPreset::Pad,
        SynthPreset::Lead,
        SynthPreset::SubBass,
    ];
    pub fn label(self) -> &'static str {
        match self {
            SynthPreset::ShadowSub => "Shadow sub",
            SynthPreset::SilentSub => "Silent sub",
            SynthPreset::SessionBass => "Session bass",
            SynthPreset::Keys => "Dusty keys",
            SynthPreset::Pluck => "Soft pluck",
            SynthPreset::Bass => "Round bass",
            SynthPreset::Pad => "Tape pad",
            SynthPreset::Lead => "Soft lead",
            SynthPreset::SubBass => "Sub bass",
            SynthPreset::Rhodes => "FM electric piano",
            SynthPreset::FmBell => "FM bell",
            SynthPreset::Mallet => "Mallet / Bongo",
            SynthPreset::Piano => "Felt piano",
            SynthPreset::Bass808 => "808 bass",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InstrumentSpec {
    /// Synthesized drum kit.
    Drums { kit: DrumKitId },
    /// Subtractive polysynth.
    Synth { preset: SynthPreset },
    /// Recorded sample played chromatically, tuned so `root` plays it as is.
    SampleMelodic { sample: SampleId, root: u8 },
    /// Recorded sample sliced into pads (start, end) in frames.
    SampleKit {
        sample: SampleId,
        slices: Vec<(u32, u32)>,
    },
    /// Granular texture: a built-in source, or a recorded sample (`sample`
    /// takes precedence when set), sprayed into overlapping grains.
    Granular {
        texture: TextureKind,
        #[serde(default)]
        sample: Option<SampleId>,
    },
    /// A synth designed in the sound editor (or loaded from a `.yggsound`).
    CustomSynth {
        name: String,
        patch: crate::sound::SynthPatch,
    },
    /// A kit of designed pads, each with its name.
    CustomKit {
        name: String,
        pads: Vec<crate::sound::DrumPad>,
        names: Vec<String>,
    },
}

/// Built-in sources for the granular texture module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextureKind {
    /// Soft pitched cloud (good for pads under chords). Old projects that
    /// used the removed "rain" texture open with the cloud instead.
    #[serde(alias = "rain")]
    Cloud,
    /// Glassy FM bell shimmer.
    Glass,
    /// Four seconds of lofisons' Shadow Ambient pads, as a cloud (G sharp minor).
    ShadowAir,
    /// Four seconds of lofisons' Silent Focus pads, as a cloud (G minor).
    SilentAir,
    /// Four seconds of a lofisons modular session, as a cloud.
    SessionAir,
}

impl TextureKind {
    pub const ALL: [TextureKind; 5] = [
        TextureKind::ShadowAir,
        TextureKind::SilentAir,
        TextureKind::SessionAir,
        TextureKind::Cloud,
        TextureKind::Glass,
    ];
    pub fn label(self) -> &'static str {
        match self {
            TextureKind::Cloud => "Cloud",
            TextureKind::Glass => "Glass",
            TextureKind::ShadowAir => "Shadow air",
            TextureKind::SilentAir => "Silent air",
            TextureKind::SessionAir => "Session air",
        }
    }

    /// Textures read from a recording: its name and the note at which it
    /// plays at its own pitch (the root of the chord it holds, near middle C).
    pub fn sampled(self) -> Option<(&'static str, u8)> {
        match self {
            TextureKind::ShadowAir => Some(("air_shadow", 68)),
            TextureKind::SilentAir => Some(("air_silent", 67)),
            TextureKind::SessionAir => Some(("air_session", 68)),
            _ => None,
        }
    }
}

impl InstrumentSpec {
    pub fn label(&self) -> String {
        match self {
            InstrumentSpec::Drums { kit } => format!("Kit {}", kit.label()),
            InstrumentSpec::Synth { preset } => preset.label().to_string(),
            InstrumentSpec::SampleMelodic { sample, .. } => format!("Sample #{sample}"),
            InstrumentSpec::SampleKit { sample, .. } => format!("Kit sample #{sample}"),
            InstrumentSpec::Granular {
                sample: Some(id), ..
            } => format!("Granular #{id}"),
            InstrumentSpec::Granular { texture, .. } => format!("Texture {}", texture.label()),
            InstrumentSpec::CustomSynth { name, .. } | InstrumentSpec::CustomKit { name, .. } => {
                name.clone()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: u32,
    pub name: String,
    pub kind: TrackKind,
    pub instrument: InstrumentSpec,
    pub macros: Macros,
    pub mute: bool,
    /// Per-track scale override (None = follow the project scale).
    pub scale: Option<Scale>,
    /// Octave of the first pad on the play surface.
    pub octave: i32,
    pub chord_mode: Option<ChordKind>,
    pub slots: Vec<Option<Pattern>>,
    /// Stereo balance -1 (left) .. 1 (right).
    #[serde(default)]
    pub pan: f32,
    #[serde(default)]
    pub solo: bool,
    /// 0..1: laid-back timing and velocity variation on sequenced notes.
    #[serde(default = "default_humanize")]
    pub humanize: f32,
    /// Level of each part (drum pad or synth layer), 0..1; missing entries
    /// play at `PART_UNITY`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<f32>,
}

fn default_humanize() -> f32 {
    0.3
}

impl Track {
    pub fn new(
        id: u32,
        name: impl Into<String>,
        kind: TrackKind,
        instrument: InstrumentSpec,
    ) -> Self {
        let octave = match (&instrument, kind) {
            (_, TrackKind::Drums) => 0,
            (
                InstrumentSpec::Synth {
                    preset:
                        SynthPreset::Bass
                        | SynthPreset::SubBass
                        | SynthPreset::Bass808
                        | SynthPreset::ShadowSub
                        | SynthPreset::SilentSub
                        | SynthPreset::SessionBass,
                },
                _,
            ) => 2,
            (InstrumentSpec::SampleMelodic { root, .. }, _) => (*root as i32 / 12 - 1).clamp(1, 6),
            // a designed bass (sub-heavy, mono) starts low
            (InstrumentSpec::CustomSynth { patch, .. }, _)
                if patch.mono > 0.5 && patch.cutoff < 600.0 =>
            {
                2
            }
            _ => 4,
        };
        Track {
            id,
            name: name.into(),
            kind,
            instrument,
            macros: Macros::default(),
            mute: false,
            scale: None,
            octave,
            chord_mode: None,
            slots: vec![None; SLOTS_PER_TRACK],
            pan: 0.0,
            solo: false,
            humanize: default_humanize(),
            parts: Vec::new(),
        }
    }

    pub fn part_level(&self, part: usize) -> f32 {
        self.parts.get(part).copied().unwrap_or(PART_UNITY)
    }

    pub fn set_part_level(&mut self, part: usize, level: f32) {
        if part < MAX_PARTS {
            if self.parts.len() <= part {
                self.parts.resize(part + 1, PART_UNITY);
            }
            self.parts[part] = level.clamp(0.0, 1.0);
        }
    }

    /// Gains for the audio thread, one per part.
    pub fn part_gains(&self) -> [f32; MAX_PARTS] {
        std::array::from_fn(|i| part_gain(self.part_level(i)))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub name: String,
    /// Index = track position, value = slot to launch (None = stop that track).
    pub slots: Vec<Option<usize>>,
}

/// A loop placed on the song timeline: plays pattern `slot` of the track
/// with id `track` from bar `start` for `bars` bars (looping the pattern).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SongClip {
    pub track: u32,
    pub slot: usize,
    pub start: u32,
    pub bars: u32,
}

impl SongClip {
    pub fn end(&self) -> u32 {
        self.start + self.bars
    }
}

/// The mini timeline: clips per track lane, bar resolution.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Song {
    pub clips: Vec<SongClip>,
    /// Loop the whole song while playing it.
    #[serde(default)]
    pub looping: bool,
    /// Fade in from silence over this many bars at the start (0 = off).
    #[serde(default)]
    pub fade_in: u32,
    /// Fade out to silence over the last bars of the song (0 = off).
    #[serde(default)]
    pub fade_out: u32,
}

pub const MAX_SONG_BARS: u32 = 512;

impl Song {
    pub fn length_bars(&self) -> u32 {
        self.clips.iter().map(|c| c.end()).max().unwrap_or(0)
    }

    /// Clear `[start, end)` on a lane, trimming or splitting clips in the way.
    pub fn carve(&mut self, track: u32, start: u32, end: u32) {
        let mut out = Vec::with_capacity(self.clips.len() + 1);
        for c in self.clips.drain(..) {
            if c.track != track || c.end() <= start || c.start >= end {
                out.push(c);
                continue;
            }
            if c.start < start {
                out.push(SongClip {
                    bars: start - c.start,
                    ..c
                });
            }
            if c.end() > end {
                out.push(SongClip {
                    start: end,
                    bars: c.end() - end,
                    ..c
                });
            }
        }
        self.clips = out;
    }

    /// Insert a clip, replacing whatever was under it on its lane.
    pub fn insert(&mut self, clip: SongClip) -> usize {
        let clip = SongClip {
            bars: clip.bars.clamp(1, MAX_SONG_BARS),
            start: clip.start.min(MAX_SONG_BARS - 1),
            ..clip
        };
        self.carve(clip.track, clip.start, clip.end());
        self.clips.push(clip);
        self.sort();
        self.clips.iter().position(|c| *c == clip).unwrap()
    }

    pub fn sort(&mut self) {
        self.clips.sort_by_key(|c| (c.track, c.start));
    }

    /// Song recording: the given slot played during `bar` on a lane.
    /// Extends the clip that ends right there, or starts a new one.
    pub fn record_bar(&mut self, track: u32, slot: usize, bar: u32) {
        if let Some(i) = self
            .clips
            .iter()
            .position(|c| c.track == track && c.slot == slot && c.end() == bar)
        {
            let mut c = self.clips.remove(i);
            c.bars += 1;
            self.insert(c);
        } else {
            self.insert(SongClip {
                track,
                slot,
                start: bar,
                bars: 1,
            });
        }
    }
}

/// Master bus: volume and the lo-fi character knobs (all 0..1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Master {
    pub volume: f32,
    /// Vinyl crackle and hiss.
    pub vinyl: f32,
    /// Tape wow & flutter.
    pub tape: f32,
    /// Low-pass warmth and gentle saturation.
    pub warmth: f32,
}

impl Default for Master {
    fn default() -> Self {
        Master {
            volume: 0.8,
            vinyl: 0.0,
            tape: 0.0,
            warmth: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub bpm: f32,
    /// 0..1, delays every second 1/16 by up to half a step.
    pub swing: f32,
    pub scale: Scale,
    pub tracks: Vec<Track>,
    pub scenes: Vec<Scene>,
    #[serde(default)]
    pub song: Song,
    #[serde(default)]
    pub master: Master,
    pub next_track_id: u32,
    /// Time signature; older projects (without it) are 4/4.
    #[serde(default, skip_serializing_if = "Meter::is_default")]
    pub meter: Meter,
}

impl Default for Project {
    fn default() -> Self {
        Project {
            name: "Untitled".into(),
            bpm: 120.0,
            swing: 0.0,
            scale: Scale::default(),
            tracks: Vec::new(),
            scenes: Vec::new(),
            song: Song::default(),
            master: Master::default(),
            next_track_id: 1,
            meter: Meter::default(),
        }
    }
}

impl Project {
    /// An empty workspace in the house mood: slow, minor, a little tape on the
    /// master, with the Shadow drums, sub and air and the felt piano ready.
    pub fn blank() -> Self {
        let mut p = Project {
            bpm: 84.0,
            swing: 0.12,
            scale: Scale::new(9, crate::theory::ScaleKind::Minor),
            master: Master {
                volume: 0.8,
                vinyl: 0.15,
                tape: 0.3,
                warmth: 0.45,
            },
            ..Project::default()
        };
        for name in ["A", "B", "C", "D"] {
            p.add_scene(name);
        }
        p.add_track(
            TrackKind::Drums,
            InstrumentSpec::Drums {
                kit: DrumKitId::Shadow,
            },
        );
        let melodic = [
            (
                InstrumentSpec::Synth {
                    preset: SynthPreset::ShadowSub,
                },
                1,
            ),
            (
                InstrumentSpec::Granular {
                    texture: TextureKind::ShadowAir,
                    sample: None,
                },
                4,
            ),
            (
                InstrumentSpec::Synth {
                    preset: SynthPreset::Piano,
                },
                4,
            ),
        ];
        for (instrument, octave) in melodic {
            p.add_track(TrackKind::Melodic, instrument);
            p.tracks.last_mut().unwrap().octave = octave;
        }
        p.tracks[2].name = "Shadow air".into();
        let set = |t: &mut Track, m: &[(MacroParam, f32)]| {
            m.iter().for_each(|&(k, v)| t.macros.set(k, v))
        };
        set(
            &mut p.tracks[2],
            &[(MacroParam::Reverb, 0.45), (MacroParam::Cutoff, 0.7)],
        );
        set(
            &mut p.tracks[3],
            &[(MacroParam::Reverb, 0.3), (MacroParam::Delay, 0.15)],
        );
        p
    }

    /// A ready-to-jam demo with dusty drums, piano, bass, pluck and a short song.
    pub fn starter() -> Self {
        let mut p = Project {
            name: "My lo-fi beat".into(),
            bpm: 82.0,
            swing: 0.4,
            master: Master {
                volume: 0.8,
                vinyl: 0.3,
                tape: 0.35,
                warmth: 0.45,
            },
            ..Project::default()
        };
        // four rows to start with: room to jam without a wall of empty slots
        for name in ["Intro", "Verse", "Outro", "Chorus"] {
            p.add_scene(name);
        }
        p.add_track(
            TrackKind::Drums,
            InstrumentSpec::Drums {
                kit: DrumKitId::Dust,
            },
        );
        p.add_track(
            TrackKind::Melodic,
            InstrumentSpec::Synth {
                preset: SynthPreset::Bass,
            },
        );
        p.add_track(
            TrackKind::Melodic,
            InstrumentSpec::Synth {
                preset: SynthPreset::Piano,
            },
        );
        p.add_track(
            TrackKind::Melodic,
            InstrumentSpec::Synth {
                preset: SynthPreset::Pluck,
            },
        );
        p.tracks[3].octave = 5;
        let set = |t: &mut Track, m: &[(MacroParam, f32)]| {
            m.iter().for_each(|&(k, v)| t.macros.set(k, v))
        };
        set(
            &mut p.tracks[0],
            &[(MacroParam::Cutoff, 0.85), (MacroParam::Saturation, 0.2)],
        );
        set(&mut p.tracks[1], &[(MacroParam::Volume, 0.75)]);
        set(
            &mut p.tracks[2],
            &[
                (MacroParam::Reverb, 0.25),
                (MacroParam::Delay, 0.12),
                (MacroParam::Volume, 0.75),
            ],
        );
        set(
            &mut p.tracks[3],
            &[
                (MacroParam::Reverb, 0.3),
                (MacroParam::Delay, 0.35),
                (MacroParam::Cutoff, 0.85),
                (MacroParam::Volume, 0.6),
            ],
        );

        let step = |s: u32, pitch: u8, velocity: f32| Note {
            start: s * TICKS_PER_STEP,
            length: TICKS_PER_STEP,
            pitch,
            velocity,
        };
        let (kick, snare, hat, open_hat) = (
            DRUM_BASE_NOTE,
            DRUM_BASE_NOTE + 1,
            DRUM_BASE_NOTE + 3,
            DRUM_BASE_NOTE + 4,
        );

        // the demo is in 4/4
        const BAR: u32 = TICKS_PER_4_4_BAR;
        // Drums: slot 1 = full groove (verse), slot 2 = sparse (outro).
        let mut sparse = Pattern::new("Beat", 1);
        for (s, v) in [(0u32, 1.0), (7, 0.5), (10, 0.85)] {
            sparse.add_note(step(s, kick, v), BAR);
        }
        for s in [4u32, 12] {
            sparse.add_note(step(s, snare, 0.85), BAR);
        }
        for s in (0..16u32).step_by(2) {
            sparse.add_note(step(s, hat, if s % 4 == 0 { 0.55 } else { 0.35 }), BAR);
        }
        let mut full = sparse.clone();
        full.name = "Groove".into();
        full.add_note(step(15, snare, 0.25), BAR); // ghost note
        for s in (1..14u32).step_by(2) {
            full.add_note(step(s, hat, 0.22), BAR);
        }
        full.add_note(step(14, open_hat, 0.4), BAR);
        p.tracks[0].slots[1] = Some(full);
        p.tracks[0].slots[2] = Some(sparse);

        // Am9 - Dm9 - Fmaj7 - Em7, one chord per bar, slightly strummed.
        let mut chords = Pattern::new("Chords", 4);
        let voicings: [&[u8]; 4] = [
            &[57, 60, 64, 67, 71],
            &[53, 57, 60, 62, 64],
            &[53, 57, 60, 64],
            &[52, 55, 59, 62],
        ];
        for (bar, notes) in voicings.iter().enumerate() {
            for (k, &pitch) in notes.iter().enumerate() {
                chords.add_note(
                    Note {
                        start: bar as u32 * BAR + k as u32 * 4,
                        length: BAR - TICKS_PER_STEP * 3,
                        pitch,
                        velocity: 0.62 + 0.04 * k as f32,
                    },
                    BAR,
                );
            }
        }
        for slot in 0..3 {
            p.tracks[2].slots[slot] = Some(chords.clone());
        }

        // Bass follows the roots: A, D, F, E.
        let mut bass = Pattern::new("Bass", 4);
        for (bar, root) in [45u8, 38, 41, 40].into_iter().enumerate() {
            let at = bar as u32 * BAR;
            bass.add_note(
                Note {
                    start: at,
                    length: TICKS_PER_STEP * 6,
                    pitch: root,
                    velocity: 0.9,
                },
                BAR,
            );
            bass.add_note(
                Note {
                    start: at + TICKS_PER_STEP * 10,
                    length: TICKS_PER_STEP * 4,
                    pitch: root,
                    velocity: 0.7,
                },
                BAR,
            );
            if bar % 2 == 1 {
                bass.add_note(
                    Note {
                        start: at + TICKS_PER_STEP * 14,
                        length: TICKS_PER_STEP * 2,
                        pitch: root + 7,
                        velocity: 0.6,
                    },
                    BAR,
                );
            }
        }
        p.tracks[1].slots[1] = Some(bass);

        // Soft pluck: a lazy A minor pentatonic phrase over two bars.
        let mut melody = Pattern::new("Melody", 2);
        for (s, pitch, v) in [
            (0u32, 76u8, 0.8),
            (3, 72, 0.6),
            (6, 74, 0.7),
            (10, 69, 0.6),
            (14, 72, 0.5),
            (16, 79, 0.75),
            (19, 76, 0.6),
            (22, 74, 0.6),
            (26, 72, 0.55),
            (28, 69, 0.5),
        ] {
            melody.add_note(
                Note {
                    start: s * TICKS_PER_STEP,
                    length: TICKS_PER_STEP * 2,
                    pitch,
                    velocity: v,
                },
                BAR,
            );
        }
        p.tracks[3].slots[0] = Some(melody.clone());
        p.tracks[3].slots[1] = Some(melody);

        let ids: Vec<u32> = p.tracks.iter().map(|t| t.id).collect();
        // (track, slot, start bar, bars): Intro 4, Verse 8, Outro 4
        for (track, slot, start, bars) in [
            (2, 0, 0, 4),
            (3, 0, 0, 4),
            (0, 1, 4, 8),
            (1, 1, 4, 8),
            (2, 1, 4, 8),
            (3, 1, 4, 8),
            (0, 2, 12, 4),
            (2, 2, 12, 4),
        ] {
            p.song.insert(SongClip {
                track: ids[track],
                slot,
                start,
                bars,
            });
        }
        p.song.looping = true;
        p.song.fade_out = 4;
        // The written demo was composed in A minor. Keep its scale degrees
        // consistent with the C major project and the performance pads.
        let from = Scale::new(9, crate::theory::ScaleKind::Minor);
        for track in p.tracks.iter_mut().filter(|t| t.kind == TrackKind::Melodic) {
            for pattern in track.slots.iter_mut().flatten() {
                for note in &mut pattern.notes {
                    note.pitch = p.scale.remap_from(&from, note.pitch);
                }
            }
        }
        p
    }

    pub fn add_track(&mut self, kind: TrackKind, instrument: InstrumentSpec) -> Option<usize> {
        if self.tracks.len() >= MAX_TRACKS {
            return None;
        }
        let id = self.next_track_id;
        self.next_track_id += 1;
        let name = match &instrument {
            InstrumentSpec::Drums { .. }
            | InstrumentSpec::SampleKit { .. }
            | InstrumentSpec::CustomKit { .. } => "Drums".to_string(),
            other => other.label(),
        };
        self.tracks.push(Track::new(id, name, kind, instrument));
        for (i, scene) in self.scenes.iter_mut().enumerate() {
            scene.slots.push((i < SLOTS_PER_TRACK).then_some(i));
        }
        Some(self.tracks.len() - 1)
    }

    pub fn remove_track(&mut self, index: usize) -> Option<Track> {
        if index >= self.tracks.len() {
            return None;
        }
        for scene in &mut self.scenes {
            if index < scene.slots.len() {
                scene.slots.remove(index);
            }
        }
        let id = self.tracks[index].id;
        self.song.clips.retain(|c| c.track != id);
        Some(self.tracks.remove(index))
    }

    /// Bring a project from disk (or an older version) to the current shape:
    /// every track has `SLOTS_PER_TRACK` slots, every scene one entry per
    /// track, and there is at least one section.
    pub fn normalize(&mut self) {
        for t in &mut self.tracks {
            t.slots.resize(SLOTS_PER_TRACK, None);
        }
        self.scenes.truncate(SLOTS_PER_TRACK);
        if self.scenes.is_empty() {
            self.add_scene("Section 1");
        }
        let n = self.tracks.len();
        for s in &mut self.scenes {
            s.slots.resize(n, None);
        }
    }

    /// Remove section `row`: its loops go, every row below moves up one,
    /// and scenes and song clips follow their loops.
    pub fn remove_section(&mut self, row: usize) -> bool {
        if row >= self.scenes.len() || self.scenes.len() <= 1 {
            return false;
        }
        for t in &mut self.tracks {
            if row < t.slots.len() {
                t.slots.remove(row);
                t.slots.push(None);
            }
        }
        self.scenes.remove(row);
        let shift = |s: usize| match s.cmp(&row) {
            std::cmp::Ordering::Less => Some(s),
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => Some(s - 1),
        };
        for scene in &mut self.scenes {
            for slot in &mut scene.slots {
                *slot = slot.and_then(shift);
            }
        }
        self.song.clips.retain(|c| c.slot != row);
        for c in &mut self.song.clips {
            if c.slot > row {
                c.slot -= 1;
            }
        }
        true
    }

    /// Scenes are grid rows: scene N launches slot N of every track.
    pub fn add_scene(&mut self, name: impl Into<String>) -> usize {
        let row = self.scenes.len();
        let slot = (row < SLOTS_PER_TRACK).then_some(row);
        self.scenes.push(Scene {
            name: name.into(),
            slots: vec![slot; self.tracks.len()],
        });
        self.scenes.len() - 1
    }

    pub fn track_scale(&self, track: usize) -> Scale {
        self.tracks
            .get(track)
            .and_then(|t| t.scale)
            .unwrap_or(self.scale)
    }

    pub fn pattern(&self, track: usize, slot: usize) -> Option<&Pattern> {
        self.tracks.get(track)?.slots.get(slot)?.as_ref()
    }

    pub fn pattern_mut(&mut self, track: usize, slot: usize) -> Option<&mut Pattern> {
        self.tracks.get_mut(track)?.slots.get_mut(slot)?.as_mut()
    }

    /// Change the time signature. Loops keep their length in bars; every note
    /// and automation point keeps its place inside its bar, and whatever no
    /// longer fits a shorter bar is dropped. The timeline is in bars: it stays.
    pub fn set_meter(&mut self, meter: Meter) {
        let meter = meter.valid();
        let (old, new) = (self.meter.bar_ticks(), meter.bar_ticks());
        self.meter = meter;
        if old == new {
            return;
        }
        let place = |t: u32| -> Option<u32> {
            let (bar, at) = (t / old, t % old);
            (at < new).then_some(bar * new + at)
        };
        for p in self
            .tracks
            .iter_mut()
            .flat_map(|t| t.slots.iter_mut().flatten())
        {
            p.notes = p
                .notes
                .iter()
                .filter_map(|n| {
                    let start = place(n.start)?;
                    let room = new - start % new;
                    Some(Note {
                        start,
                        // a note no longer runs past the end of its (shorter) bar
                        length: if new < old {
                            n.length.min(room)
                        } else {
                            n.length
                        },
                        ..*n
                    })
                })
                .collect();
            for lane in &mut p.automation {
                lane.points = lane
                    .points
                    .iter()
                    .filter_map(|pt| {
                        Some(AutoPoint {
                            tick: place(pt.tick)?,
                            value: pt.value,
                        })
                    })
                    .collect();
            }
            p.sort();
        }
    }

    /// Ticks in one bar of the project's meter.
    pub fn bar_ticks(&self) -> u32 {
        self.meter.bar_ticks()
    }

    /// Ticks per second at the current tempo.
    pub fn ticks_per_second(&self) -> f64 {
        self.bpm as f64 / 60.0 * PPQ as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const TICKS_PER_BAR: u32 = TICKS_PER_4_4_BAR;

    #[test]
    fn meters_are_whole_steps() {
        for m in Meter::COMMON {
            assert_eq!(m.bar_ticks() % TICKS_PER_STEP, 0, "{m:?}");
        }
        assert_eq!(Meter { beats: 3, unit: 4 }.steps(), 12);
        assert_eq!(Meter { beats: 6, unit: 8 }.steps(), 12);
        assert_eq!(Meter { beats: 7, unit: 8 }.steps(), 14);
        assert_eq!(
            Meter { beats: 0, unit: 3 }.valid(),
            Meter { beats: 1, unit: 4 }
        );
    }

    #[test]
    fn changing_the_meter_keeps_notes_in_their_bars() {
        let mut p = Project::starter();
        let before = p.pattern(0, 1).unwrap().clone();
        p.set_meter(Meter { beats: 3, unit: 4 });
        let bar = p.bar_ticks();
        assert_eq!(bar, 288);
        let after = p.pattern(0, 1).unwrap();
        assert_eq!(after.bars, before.bars);
        // beats 1-3 stay, beat 4 is gone, and nothing overflows the loop
        assert!(after
            .notes
            .iter()
            .all(|n| n.start < after.length_ticks(bar)));
        let kept = before.notes.iter().filter(|n| n.start % 384 < 288).count();
        assert_eq!(after.notes.len(), kept);
        // back to 4/4: the remaining notes are where they were
        p.set_meter(Meter::default());
        let back = p.pattern(0, 1).unwrap();
        assert!(back.notes.iter().all(|n| before
            .notes
            .iter()
            .any(|b| b.start == n.start && b.pitch == n.pitch)));
        // the meter is saved only when it is not 4/4
        assert!(!serde_json::to_string(&p).unwrap().contains("meter"));
        p.set_meter(Meter { beats: 7, unit: 8 });
        let json = serde_json::to_string(&p).unwrap();
        let loaded: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.meter, Meter { beats: 7, unit: 8 });
    }

    #[test]
    fn automation_interp_and_wrap() {
        let mut lane = AutomationLane::new(MacroParam::Cutoff);
        lane.set_points(vec![
            AutoPoint {
                tick: 0,
                value: 0.0,
            },
            AutoPoint {
                tick: 100,
                value: 1.0,
            },
        ]);
        assert_eq!(lane.value_at(50.0, 200), Some(0.5));
        assert_eq!(lane.value_at(150.0, 200), Some(0.5)); // wrapping back down
        assert_eq!(lane.value_at(0.0, 200), Some(0.0));
    }

    #[test]
    fn automation_write_overwrites() {
        let mut lane = AutomationLane::new(MacroParam::Cutoff);
        lane.set_points(
            (0..10)
                .map(|i| AutoPoint {
                    tick: i * 10,
                    value: 0.5,
                })
                .collect(),
        );
        lane.write(Some(15), 45, 1.0);
        let ticks: Vec<u32> = lane.points.iter().map(|p| p.tick).collect();
        assert_eq!(ticks, vec![0, 10, 45, 50, 60, 70, 80, 90]);
    }

    #[test]
    fn pattern_grow_duplicates() {
        let mut p = Pattern::new("x", 1);
        p.add_note(
            Note {
                start: 0,
                length: 24,
                pitch: 36,
                velocity: 1.0,
            },
            TICKS_PER_BAR,
        );
        p.set_bars(4, TICKS_PER_BAR);
        assert_eq!(p.notes.len(), 4);
        assert_eq!(p.notes[3].start, 3 * TICKS_PER_BAR);
        p.set_bars(2, TICKS_PER_BAR);
        assert_eq!(p.notes.len(), 2);
    }

    #[test]
    fn toggle_step() {
        let mut p = Pattern::new("x", 1);
        assert!(p.toggle_step(3, 40, 1.0, 24, TICKS_PER_BAR));
        assert_eq!(p.notes[0].start, 72);
        assert!(!p.toggle_step(3, 40, 1.0, 24, TICKS_PER_BAR));
        assert!(p.notes.is_empty());
    }

    #[test]
    fn toggle_step_clears_a_roll() {
        let mut p = Pattern::new("x", 1);
        for k in 0..4 {
            p.add_note(
                Note {
                    start: 2 * TICKS_PER_STEP + k * 6,
                    length: 6,
                    pitch: 39,
                    velocity: 0.5,
                },
                TICKS_PER_BAR,
            );
        }
        assert!(!p.toggle_step(2, 39, 1.0, 24, TICKS_PER_BAR));
        assert!(p.notes.is_empty());
    }

    #[test]
    fn quantize() {
        assert_eq!(quantize_tick(30, 24, 1.0), 24);
        assert_eq!(quantize_tick(40, 24, 1.0), 48);
        assert_eq!(quantize_tick(30, 24, 0.5), 27);
    }

    #[test]
    fn song_insert_carves_lane() {
        let mut song = Song::default();
        song.insert(SongClip {
            track: 1,
            slot: 0,
            start: 0,
            bars: 8,
        });
        song.insert(SongClip {
            track: 2,
            slot: 0,
            start: 0,
            bars: 8,
        });
        // split the first lane in two around the new clip
        song.insert(SongClip {
            track: 1,
            slot: 1,
            start: 2,
            bars: 2,
        });
        let lane: Vec<(u32, u32, usize)> = song
            .clips
            .iter()
            .filter(|c| c.track == 1)
            .map(|c| (c.start, c.bars, c.slot))
            .collect();
        assert_eq!(lane, vec![(0, 2, 0), (2, 2, 1), (4, 4, 0)]);
        assert_eq!(song.clips.iter().filter(|c| c.track == 2).count(), 1);
        assert_eq!(song.length_bars(), 8);
    }

    #[test]
    fn song_record_bar_extends() {
        let mut song = Song::default();
        for bar in 0..4 {
            song.record_bar(1, 0, bar);
        }
        song.record_bar(1, 1, 4);
        song.record_bar(1, 0, 5);
        let lane: Vec<(u32, u32, usize)> = song
            .clips
            .iter()
            .map(|c| (c.start, c.bars, c.slot))
            .collect();
        assert_eq!(lane, vec![(0, 4, 0), (4, 1, 1), (5, 1, 0)]);
    }

    #[test]
    fn old_project_files_still_load() {
        let mut v = serde_json::to_value(Project::starter()).unwrap();
        let obj = v.as_object_mut().unwrap();
        obj.remove("song");
        obj.remove("master");
        for t in obj["tracks"].as_array_mut().unwrap() {
            t.as_object_mut().unwrap().remove("pan");
        }
        let p: Project = serde_json::from_value(v).unwrap();
        assert!(p.song.clips.is_empty());
        assert_eq!(p.master, Master::default());
    }

    #[test]
    fn old_polvo_kit_id_opens_as_dust() {
        let spec: InstrumentSpec =
            serde_json::from_str(r#"{"type":"drums","kit":"polvo"}"#).unwrap();
        assert_eq!(
            spec,
            InstrumentSpec::Drums {
                kit: DrumKitId::Dust
            }
        );
    }

    #[test]
    fn removed_rain_texture_opens_as_cloud() {
        let spec: InstrumentSpec =
            serde_json::from_str(r#"{"type":"granular","texture":"rain","sample":null}"#).unwrap();
        assert_eq!(
            spec,
            InstrumentSpec::Granular {
                texture: TextureKind::Cloud,
                sample: None
            }
        );
    }

    #[test]
    fn project_serde_roundtrip() {
        let p = Project::starter();
        let s = serde_json::to_string(&p).unwrap();
        let q: Project = serde_json::from_str(&s).unwrap();
        assert_eq!(p, q);
    }
}
