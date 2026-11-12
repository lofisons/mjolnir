//! Control-thread API. Owns the authoritative `Project`, translates user
//! actions into engine commands, handles capture, recording, scale changes,
//! sampling and persistence. The UI talks only to this.

use std::collections::HashMap;
use std::sync::Arc;

use base64::Engine as _;
use rtrb::{Consumer, Producer};
use serde::{Deserialize, Serialize};

use crate::capture::{CaptureBuffer, LiveEvent};
use crate::engine::{
    Command, EngineEndpoints, EngineEvent, Garbage, SharedStatus, TrackRuntime, SLOT_STOP,
};
use crate::instruments::{Instrument, SampleData};
use crate::model::*;
use crate::sampling;
use crate::theory::{note_name, ChordKind, Scale};

#[derive(Debug, Clone, Serialize)]
pub struct PadInfo {
    pub label: String,
    pub pitches: Vec<u8>,
    pub root: bool,
}

/// One part of an instrument in the part mix.
#[derive(Debug, Clone, Serialize)]
pub struct PartInfo {
    pub index: usize,
    pub label: String,
    pub level: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrackStatus {
    pub active: Option<usize>,
    /// Slot that will start at the next bar.
    pub queued: Option<usize>,
    /// The track will stop at the next bar.
    pub queued_stop: bool,
    pub level: f32,
    pub macros: [f32; 7],
    /// 0..1 position inside the active loop.
    pub progress: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub playing: bool,
    pub count_in: u32,
    pub note_onset: u64,
    pub tick: f64,
    pub bar: u32,
    pub beat: u32,
    pub master_level: f32,
    pub tracks: Vec<TrackStatus>,
    pub song_mode: bool,
    pub metronome: bool,
    /// Bumped whenever the project changed from the engine side (recording).
    pub revision: u64,
}

/// Project + embedded samples, as stored on disk (`.yggdrasil` JSON).
#[derive(Serialize, Deserialize)]
pub struct ProjectFile {
    pub format: String,
    pub project: Project,
    /// Sample id -> base64 encoded 16-bit WAV.
    pub samples: HashMap<SampleId, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SampleMode {
    Melodic,
    Kit,
    /// Granular texture made from the recording.
    Granular,
}

pub struct Controller {
    pub project: Project,
    samples: HashMap<SampleId, Arc<SampleData>>,
    next_sample_id: SampleId,
    commands: Producer<Command>,
    events: Consumer<EngineEvent>,
    garbage: Consumer<Garbage>,
    status: Arc<SharedStatus>,
    sr: f32,
    capture: HashMap<u32, CaptureBuffer>,
    held_pads: HashMap<(usize, usize), Vec<u8>>,
    /// Overdub live notes into the active pattern while playing.
    pub record_notes: bool,
    /// Record macro movements into the active pattern while playing.
    pub record_automation: bool,
    pub quantize: bool,
    recording_notes: HashMap<(usize, u8), (usize, f32, f64)>,
    last_auto_tick: HashMap<(usize, MacroParam), u32>,
    revision: u64,
    /// Play the song timeline instead of the session grid.
    song_mode: bool,
    /// Write what the session plays into the song timeline.
    pub record_song: bool,
    /// Session preference: 0, 1 or 2 bars before an armed take.
    pub count_in_bars: u32,
    /// Bar where playback (song mode) and song recording start.
    start_bar: u32,
    last_recorded_bar: Option<u32>,
    /// Groove mode: the timeline is a tape. Recording appends after what is
    /// there, and editing a loop the tape already holds keeps the recorded
    /// bars as they sounded (a take).
    pub groove: bool,
    /// Groove mode: timeline bar = playback bar + this offset.
    tape_offset: i64,
    /// Timeline bar being recorded right now (None when not recording).
    recording_at: Option<u32>,
    undo_stack: Vec<Project>,
    redo_stack: Vec<Project>,
    pub(crate) midi: crate::midi::MidiState,
    metronome: bool,
    note_onset: u64,
    /// A take recorded into a new loop: it grows (1, 2, 4, 8 bars) while
    /// the recording goes on, and keeps its length when it stops.
    growing: Option<(usize, usize)>,
    /// The section shown in the interface: a new take goes there if free.
    pub focus_slot: Option<usize>,
}

const UNDO_DEPTH: usize = 100;

/// Build the audio-thread representation of a track.
pub fn track_runtime(
    track: &Track,
    samples: &HashMap<SampleId, Arc<SampleData>>,
    sr: f32,
) -> TrackRuntime {
    let instrument = build_instrument(&track.instrument, samples, sr);
    let mut rt = TrackRuntime::new(
        instrument,
        track.macros,
        track.mute,
        track.slots.clone(),
        sr,
    );
    rt.set_mix(track.solo, track.pan);
    rt.set_humanize(track.humanize);
    rt.set_parts(track.part_gains());
    rt
}

pub fn build_instrument(
    spec: &InstrumentSpec,
    samples: &HashMap<SampleId, Arc<SampleData>>,
    sr: f32,
) -> Instrument {
    let sample = match spec {
        InstrumentSpec::SampleMelodic { sample, .. } | InstrumentSpec::SampleKit { sample, .. } => {
            samples.get(sample).cloned()
        }
        InstrumentSpec::Granular {
            sample: Some(sample),
            ..
        } => samples.get(sample).cloned(),
        _ => None,
    };
    Instrument::build(spec, sample, sr)
}

impl Controller {
    pub fn new(endpoints: EngineEndpoints, sample_rate: f32) -> Self {
        let EngineEndpoints {
            commands,
            events,
            garbage,
            status,
        } = endpoints;
        let mut c = Controller {
            project: Project::default(),
            samples: HashMap::new(),
            next_sample_id: 1,
            commands,
            events,
            garbage,
            status,
            sr: sample_rate,
            capture: HashMap::new(),
            held_pads: HashMap::new(),
            record_notes: false,
            record_automation: false,
            quantize: true,
            recording_notes: HashMap::new(),
            last_auto_tick: HashMap::new(),
            revision: 0,
            song_mode: false,
            record_song: false,
            count_in_bars: 0,
            start_bar: 0,
            last_recorded_bar: None,
            groove: false,
            tape_offset: 0,
            recording_at: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            midi: Default::default(),
            metronome: false,
            note_onset: 0,
            growing: None,
            focus_slot: None,
        };
        c.load(Project::starter(), HashMap::new());
        c
    }

    fn send(&mut self, cmd: Command) {
        let mut cmd = cmd;
        // The queue is large; if the audio thread is stalled, wait briefly.
        for _ in 0..200 {
            match self.commands.push(cmd) {
                Ok(()) => return,
                Err(rtrb::PushError::Full(c)) => {
                    cmd = c;
                    self.collect_garbage();
                    // no threads in a browser: there the engine drains the queue
                    // between commands, so a full queue is not waited on
                    #[cfg(not(target_arch = "wasm32"))]
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
        }
        eprintln!("mjolnir: engine command queue is full, dropping a command");
    }

    fn collect_garbage(&mut self) {
        while let Ok(g) = self.garbage.pop() {
            drop(g);
        }
    }

    pub fn samples(&self) -> &HashMap<SampleId, Arc<SampleData>> {
        &self.samples
    }

    pub fn sample_rate(&self) -> f32 {
        self.sr
    }

    /// Replace the whole project (new/open).
    pub fn load(&mut self, project: Project, samples: HashMap<SampleId, Arc<SampleData>>) {
        self.send(Command::Stop);
        for _ in 0..self.project.tracks.len() {
            self.send(Command::RemoveTrack { index: 0 });
        }
        self.project = project;
        self.samples = samples;
        self.next_sample_id = self.samples.keys().max().map(|m| m + 1).unwrap_or(1);
        self.record_notes = false;
        self.record_automation = false;
        self.record_song = false;
        self.song_mode = false;
        self.send(Command::SetSongMode(false));
        self.capture.clear();
        self.held_pads.clear();
        self.recording_notes.clear();
        self.midi_reset();
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.start_bar = 0;
        self.send(Command::Seek { tick: 0.0 });
        self.insert_all_tracks();
        self.revision += 1;
    }

    fn insert_all_tracks(&mut self) {
        for i in 0..self.project.tracks.len() {
            let rt = track_runtime(&self.project.tracks[i], &self.samples, self.sr);
            self.send(Command::InsertTrack {
                index: i,
                track: Box::new(rt),
            });
        }
        self.sync_tempo();
        self.sync_song();
        let master = self.project.master;
        self.send(Command::SetMaster(master));
    }

    // ------------------------------------------------------------ undo/redo

    /// Remember the current project before a change, for undo.
    pub fn checkpoint(&mut self) {
        if self.undo_stack.last() == Some(&self.project) {
            return;
        }
        self.undo_stack.push(self.project.clone());
        if self.undo_stack.len() > UNDO_DEPTH {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo(&mut self) -> Result<(), String> {
        let prev = self.undo_stack.pop().ok_or("nothing to undo")?;
        let current = std::mem::replace(&mut self.project, prev);
        self.redo_stack.push(current.clone());
        self.restore(current);
        Ok(())
    }

    pub fn redo(&mut self) -> Result<(), String> {
        let next = self.redo_stack.pop().ok_or("nothing to redo")?;
        let current = std::mem::replace(&mut self.project, next);
        self.undo_stack.push(current.clone());
        self.restore(current);
        Ok(())
    }

    /// Bring the engine in line with `self.project` after undo/redo, without
    /// stopping playback when the track layout did not change.
    fn restore(&mut self, before: Project) {
        let same_layout = before.tracks.len() == self.project.tracks.len()
            && before
                .tracks
                .iter()
                .zip(&self.project.tracks)
                .all(|(a, b)| a.id == b.id);
        if same_layout {
            for i in 0..self.project.tracks.len() {
                let (a, b) = (&before.tracks[i], &self.project.tracks[i]);
                let instrument = (a.instrument != b.instrument)
                    .then(|| build_instrument(&b.instrument, &self.samples, self.sr));
                let (macros, slots_changed) = (b.macros, a.slots != b.slots);
                if let Some(inst) = instrument {
                    self.send(Command::SetInstrument {
                        track: i,
                        instrument: Box::new(inst),
                    });
                }
                self.send(Command::SetMacros { track: i, macros });
                self.sync_mix(i);
                let amount = self.project.tracks[i].humanize;
                self.send(Command::SetHumanize { track: i, amount });
                self.sync_parts(i);
                if slots_changed {
                    self.sync_track_patterns(i);
                }
            }
            self.sync_tempo();
            self.sync_song();
            let master = self.project.master;
            self.send(Command::SetMaster(master));
        } else {
            for _ in 0..before.tracks.len() {
                self.send(Command::RemoveTrack { index: 0 });
            }
            self.insert_all_tracks();
            self.held_pads.clear();
            self.recording_notes.clear();
            self.midi_reset();
        }
        self.revision += 1;
    }

    /// Change the time signature (see `Project::set_meter`).
    pub fn set_meter(&mut self, meter: crate::model::Meter) {
        self.project.set_meter(meter);
        for i in 0..self.project.tracks.len() {
            self.sync_track_patterns(i);
        }
        self.sync_tempo();
        self.sync_song();
        self.revision += 1;
    }

    fn sync_tempo(&mut self) {
        let (bpm, swing) = (self.project.bpm, self.project.swing);
        self.send(Command::SetTempo { bpm, swing });
        let (bar, beat) = (self.project.bar_ticks(), self.project.meter.beat_ticks());
        self.send(Command::SetMeter { bar, beat });
    }

    fn sync_pattern(&mut self, track: usize, slot: usize) {
        let pattern = self.project.pattern(track, slot).cloned().map(Box::new);
        self.send(Command::SetPattern {
            track,
            slot,
            pattern,
        });
    }

    fn sync_track_patterns(&mut self, track: usize) {
        for slot in 0..SLOTS_PER_TRACK {
            self.sync_pattern(track, slot);
        }
    }

    fn check_track(&self, track: usize) -> Result<(), String> {
        if track < self.project.tracks.len() {
            Ok(())
        } else {
            Err(format!("track {track} does not exist"))
        }
    }

    // ------------------------------------------------------------ transport

    pub fn play(&mut self) {
        if self.record_notes || self.record_automation || self.record_song {
            let beats = self.count_in_bars.min(2) * self.project.meter.beats;
            self.send(Command::PlayWithCountIn(beats));
        } else {
            self.send(Command::Play);
        }
    }

    pub fn metronome(&self) -> bool {
        self.metronome
    }

    /// Click on every beat while playing. Not part of the project or exports.
    pub fn set_metronome(&mut self, on: bool) {
        self.metronome = on;
        self.send(Command::SetMetronome(on));
    }

    pub fn stop(&mut self) {
        self.poll();
        let tick = self.status.song_tick();
        let held: Vec<_> = self.recording_notes.keys().copied().collect();
        let mut changed = false;
        for (track, pitch) in held {
            changed |= self.record_live(track, pitch, 0.0, false, tick);
        }
        if changed {
            self.revision += 1;
        }
        self.send(Command::Stop);
        self.last_auto_tick.clear();
    }

    pub fn set_bpm(&mut self, bpm: f32) {
        self.project.bpm = bpm.clamp(40.0, 300.0);
        self.sync_tempo();
    }

    pub fn set_swing(&mut self, swing: f32) {
        self.project.swing = swing.clamp(0.0, 1.0);
        self.sync_tempo();
    }

    // --------------------------------------------------------------- tracks

    pub fn add_track(
        &mut self,
        kind: TrackKind,
        instrument: InstrumentSpec,
    ) -> Result<usize, String> {
        let idx = self
            .project
            .add_track(kind, instrument)
            .ok_or("track limit reached")?;
        let rt = track_runtime(&self.project.tracks[idx], &self.samples, self.sr);
        self.send(Command::InsertTrack {
            index: idx,
            track: Box::new(rt),
        });
        self.sync_song();
        Ok(idx)
    }

    pub fn remove_track(&mut self, track: usize) -> Result<(), String> {
        self.check_track(track)?;
        let removed = self.project.remove_track(track).unwrap();
        self.capture.remove(&removed.id);
        self.send(Command::RemoveTrack { index: track });
        self.sync_song();
        self.held_pads.clear();
        self.recording_notes.clear();
        self.last_auto_tick.clear();
        self.midi_reset();
        Ok(())
    }

    pub fn rename_track(&mut self, track: usize, name: String) -> Result<(), String> {
        self.check_track(track)?;
        self.project.tracks[track].name = name;
        Ok(())
    }

    fn sync_mix(&mut self, track: usize) {
        let t = &self.project.tracks[track];
        let (mute, solo, pan) = (t.mute, t.solo, t.pan);
        self.send(Command::SetMix {
            track,
            mute,
            solo,
            pan,
        });
    }

    pub fn set_mute(&mut self, track: usize, mute: bool) -> Result<(), String> {
        self.check_track(track)?;
        self.project.tracks[track].mute = mute;
        self.sync_mix(track);
        Ok(())
    }

    pub fn set_solo(&mut self, track: usize, solo: bool) -> Result<(), String> {
        self.check_track(track)?;
        self.project.tracks[track].solo = solo;
        self.sync_mix(track);
        Ok(())
    }

    pub fn set_humanize(&mut self, track: usize, amount: f32) -> Result<(), String> {
        self.check_track(track)?;
        let amount = amount.clamp(0.0, 1.0);
        self.project.tracks[track].humanize = amount;
        self.send(Command::SetHumanize { track, amount });
        Ok(())
    }

    pub fn set_pan(&mut self, track: usize, pan: f32) -> Result<(), String> {
        self.check_track(track)?;
        self.project.tracks[track].pan = pan.clamp(-1.0, 1.0);
        self.sync_mix(track);
        Ok(())
    }

    pub fn set_master(&mut self, master: Master) {
        let clamp = |v: f32| v.clamp(0.0, 1.0);
        self.project.master = Master {
            volume: clamp(master.volume),
            vinyl: clamp(master.vinyl),
            tape: clamp(master.tape),
            warmth: clamp(master.warmth),
        };
        let m = self.project.master;
        self.send(Command::SetMaster(m));
    }

    /// "Swap": change the sound source keeping every note.
    pub fn swap_instrument(&mut self, track: usize, spec: InstrumentSpec) -> Result<(), String> {
        self.check_track(track)?;
        if let InstrumentSpec::SampleKit { sample, .. }
        | InstrumentSpec::SampleMelodic { sample, .. }
        | InstrumentSpec::Granular {
            sample: Some(sample),
            ..
        } = &spec
        {
            if !self.samples.contains_key(sample) {
                return Err("unknown sample".into());
            }
        }
        let instrument = build_instrument(&spec, &self.samples, self.sr);
        let t = &mut self.project.tracks[track];
        // the part mix carries over only while the parts mean the same thing
        // (pad N of one kit is pad N of another); a new set of layers starts flat
        let names = |s: &InstrumentSpec| crate::instruments::part_names(s);
        let same_parts = t.kind == TrackKind::Drums || names(&t.instrument) == names(&spec);
        if !same_parts {
            t.parts.clear();
        }
        t.instrument = spec;
        self.send(Command::SetInstrument {
            track,
            instrument: Box::new(instrument),
        });
        self.sync_parts(track);
        Ok(())
    }

    /// The parts of a track's instrument that can be mixed on their own,
    /// with their current levels.
    pub fn parts(&self, track: usize) -> Result<Vec<PartInfo>, String> {
        self.check_track(track)?;
        let t = &self.project.tracks[track];
        Ok(crate::instruments::part_names(&t.instrument)
            .into_iter()
            .map(|(index, label)| PartInfo {
                index,
                label,
                level: t.part_level(index),
            })
            .collect())
    }

    /// Level of one part (0..1, `PART_UNITY` as designed). Live, like a knob:
    /// the UI sends `checkpoint` when a drag starts.
    pub fn set_part(&mut self, track: usize, part: usize, level: f32) -> Result<(), String> {
        self.check_track(track)?;
        if part >= MAX_PARTS {
            return Err("invalid part".into());
        }
        self.project.tracks[track].set_part_level(part, level);
        self.sync_parts(track);
        Ok(())
    }

    // --------------------------------------------------------- sound design

    /// Open the track's sound for design: a built-in synth preset or kit
    /// becomes an editable copy that sounds the same (the felt piano and soft
    /// pluck are physical models: their copy starts from a close FM patch,
    /// and the sampled Dust kit from the Lo-Fi kit). Designed sounds stay.
    pub fn design_sound(&mut self, track: usize) -> Result<(), String> {
        self.check_track(track)?;
        let spec = match &self.project.tracks[track].instrument {
            InstrumentSpec::CustomSynth { .. } | InstrumentSpec::CustomKit { .. } => return Ok(()),
            InstrumentSpec::Synth { preset } => InstrumentSpec::CustomSynth {
                name: format!("My {}", preset.label().to_lowercase()),
                patch: crate::instruments::patch_for(*preset),
            },
            InstrumentSpec::Drums { kit } => {
                let names_from = if *kit == DrumKitId::Dust {
                    DrumKitId::LoFi
                } else {
                    *kit
                };
                InstrumentSpec::CustomKit {
                    name: format!("My {} kit", names_from.label().to_lowercase()),
                    pads: crate::instruments::kit_pads(*kit),
                    names: names_from
                        .pad_names()
                        .iter()
                        .map(|n| n.to_string())
                        .collect(),
                }
            }
            _ => {
                return Err(
                    "Samples and textures can't be designed: pick a synth or a kit first".into(),
                )
            }
        };
        self.swap_instrument(track, spec)
    }

    /// Turn one knob of a designed sound (`pad` for a kit). Not an undo step
    /// by itself: the interface checkpoints when a knob is grabbed.
    pub fn set_sound_param(
        &mut self,
        track: usize,
        pad: Option<usize>,
        key: &str,
        value: f32,
    ) -> Result<(), String> {
        self.check_track(track)?;
        let cmd = match &mut self.project.tracks[track].instrument {
            InstrumentSpec::CustomSynth { patch, .. } => {
                patch.set(key, value)?;
                Command::SetSynthPatch {
                    track,
                    patch: *patch,
                }
            }
            InstrumentSpec::CustomKit { pads, .. } => {
                let pad = pad.ok_or("choose a pad")?;
                if pad >= DRUM_PADS {
                    return Err("no such pad".into());
                }
                if pads.len() <= pad {
                    pads.resize(pad + 1, crate::sound::DrumPad::default());
                }
                pads[pad].set(key, value)?;
                Command::SetDrumPad {
                    track,
                    pad,
                    sound: pads[pad],
                }
            }
            _ => return Err("design the sound first".into()),
        };
        self.send(cmd);
        Ok(())
    }

    /// Name a designed sound, or one pad of a designed kit.
    pub fn rename_sound(
        &mut self,
        track: usize,
        pad: Option<usize>,
        name: String,
    ) -> Result<(), String> {
        self.check_track(track)?;
        let name: String = name.trim().chars().take(40).collect();
        if name.is_empty() {
            return Err("a name can't be empty".into());
        }
        match (&mut self.project.tracks[track].instrument, pad) {
            (InstrumentSpec::CustomSynth { name: n, .. }, None)
            | (InstrumentSpec::CustomKit { name: n, .. }, None) => *n = name,
            (InstrumentSpec::CustomKit { names, .. }, Some(p)) if p < DRUM_PADS => {
                if names.len() <= p {
                    names.resize(p + 1, String::new());
                }
                names[p] = name;
            }
            _ => return Err("only designed sounds can be renamed".into()),
        }
        self.revision += 1;
        Ok(())
    }

    /// The track's sound as a `.yggsound` file (JSON).
    pub fn sound_file(&self, track: usize) -> Result<String, String> {
        self.check_track(track)?;
        let sound = self.project.tracks[track].instrument.clone();
        if matches!(
            sound,
            InstrumentSpec::SampleKit { .. }
                | InstrumentSpec::SampleMelodic { .. }
                | InstrumentSpec::Granular {
                    sample: Some(_),
                    ..
                }
        ) {
            return Err("Sounds made from a recording can't be saved as a sound file yet".into());
        }
        let file = crate::sound::SoundFile {
            format: "yggsound".into(),
            version: crate::sound::SOUND_FILE_VERSION,
            sound,
        };
        serde_json::to_string_pretty(&file).map_err(|e| e.to_string())
    }

    /// Load a `.yggsound` file into the track (a kit on a drum track, a
    /// synth on an instrument track). The notes stay.
    pub fn load_sound_file(&mut self, track: usize, json: &str) -> Result<(), String> {
        self.check_track(track)?;
        let file: crate::sound::SoundFile =
            serde_json::from_str(json).map_err(|e| format!("not a Yggdrasil sound: {e}"))?;
        // "tinysound": saved when the app was called TinyDaw
        if file.format != "yggsound" && file.format != "tinysound" {
            return Err("not a Yggdrasil sound file".into());
        }
        if file.version > crate::sound::SOUND_FILE_VERSION {
            return Err("this sound was made by a newer Yggdrasil: update to open it".into());
        }
        let spec = match file.sound {
            InstrumentSpec::CustomSynth { name, patch } => InstrumentSpec::CustomSynth {
                name,
                patch: patch.clamped(),
            },
            InstrumentSpec::CustomKit { name, pads, names } => InstrumentSpec::CustomKit {
                name,
                pads: pads
                    .into_iter()
                    .take(DRUM_PADS)
                    .map(|p| p.clamped())
                    .collect(),
                names: names.into_iter().take(DRUM_PADS).collect(),
            },
            InstrumentSpec::SampleKit { .. }
            | InstrumentSpec::SampleMelodic { .. }
            | InstrumentSpec::Granular {
                sample: Some(_), ..
            } => return Err("this sound needs a recording that is not in the file".into()),
            other => other,
        };
        let is_kit = matches!(
            spec,
            InstrumentSpec::Drums { .. } | InstrumentSpec::CustomKit { .. }
        );
        let wants_kit = self.project.tracks[track].kind == TrackKind::Drums;
        if is_kit != wants_kit {
            return Err(if is_kit {
                "This is a drum kit: load it on a drum track".into()
            } else {
                "This is an instrument: load it on an instrument track".into()
            });
        }
        self.swap_instrument(track, spec)
    }

    fn sync_parts(&mut self, track: usize) {
        let gains = self.project.tracks[track].part_gains();
        self.send(Command::SetParts { track, gains });
    }

    // --------------------------------------------------------- play surface

    /// The 16 pads of the play surface for a track: drum names, or scale
    /// notes (out-of-scale notes are never offered), or diatonic chords.
    pub fn pad_layout(&self, track: usize) -> Result<Vec<PadInfo>, String> {
        self.check_track(track)?;
        let t = &self.project.tracks[track];
        if t.kind == TrackKind::Drums {
            let names: Vec<String> = match &t.instrument {
                InstrumentSpec::Drums { kit } => {
                    kit.pad_names().iter().map(|n| n.to_string()).collect()
                }
                InstrumentSpec::SampleKit { .. } => {
                    (0..DRUM_PADS).map(|i| format!("Slice {}", i + 1)).collect()
                }
                InstrumentSpec::CustomKit { names, .. } => names.clone(),
                _ => DrumKitId::LoFi
                    .pad_names()
                    .iter()
                    .map(|n| n.to_string())
                    .collect(),
            };
            return Ok((0..DRUM_PADS)
                .map(|i| PadInfo {
                    label: names
                        .get(i)
                        .cloned()
                        .unwrap_or_else(|| format!("Pad {}", i + 1)),
                    pitches: vec![DRUM_BASE_NOTE + i as u8],
                    root: false,
                })
                .collect());
        }
        let scale = self.project.track_scale(track);
        let n = scale.len() as i32;
        Ok((0..16)
            .map(|i| {
                let pitches = match t.chord_mode {
                    Some(kind) => scale.chord(i, t.octave, kind),
                    None => vec![scale.degree_to_pitch(i, t.octave)],
                };
                let label = match t.chord_mode {
                    Some(_) => chord_label(&pitches),
                    None => note_name(pitches[0]),
                };
                PadInfo {
                    label,
                    pitches,
                    root: i.rem_euclid(n) == 0,
                }
            })
            .collect())
    }

    pub fn pad_on(&mut self, track: usize, pad: usize, velocity: f32) -> Result<(), String> {
        let layout = self.pad_layout(track)?;
        let info = layout.get(pad).ok_or("invalid pad")?;
        if let Some(prev) = self.held_pads.remove(&(track, pad)) {
            for p in prev {
                self.send(Command::NoteOff { track, pitch: p });
            }
        }
        for &p in &info.pitches {
            self.send(Command::NoteOn {
                track,
                pitch: p,
                velocity: velocity.clamp(0.05, 1.0),
            });
        }
        self.held_pads.insert((track, pad), info.pitches.clone());
        Ok(())
    }

    pub fn pad_off(&mut self, track: usize, pad: usize) -> Result<(), String> {
        if let Some(pitches) = self.held_pads.remove(&(track, pad)) {
            for p in pitches {
                self.send(Command::NoteOff { track, pitch: p });
            }
        }
        Ok(())
    }

    pub fn note_on(&mut self, track: usize, pitch: u8, velocity: f32) -> Result<(), String> {
        self.check_track(track)?;
        self.send(Command::NoteOn {
            track,
            pitch,
            velocity,
        });
        Ok(())
    }

    pub fn note_off(&mut self, track: usize, pitch: u8) -> Result<(), String> {
        self.check_track(track)?;
        self.send(Command::NoteOff { track, pitch });
        Ok(())
    }

    pub fn set_octave(&mut self, track: usize, octave: i32) -> Result<(), String> {
        self.check_track(track)?;
        self.project.tracks[track].octave = octave.clamp(0, 7);
        Ok(())
    }

    pub fn set_chord_mode(&mut self, track: usize, mode: Option<ChordKind>) -> Result<(), String> {
        self.check_track(track)?;
        self.project.tracks[track].chord_mode = mode;
        Ok(())
    }

    // ------------------------------------------------------------- patterns

    /// Grid rows in use: one per section.
    fn rows(&self) -> usize {
        self.project.scenes.len().clamp(1, SLOTS_PER_TRACK)
    }

    /// First empty slot of a track among the visible rows.
    fn free_slot(&self, track: usize) -> Option<usize> {
        self.project.tracks[track].slots[..self.rows()]
            .iter()
            .position(|s| s.is_none())
    }

    fn slot_ok(&self, track: usize, slot: usize) -> Result<(), String> {
        self.check_track(track)?;
        if slot < self.rows() {
            Ok(())
        } else {
            Err("invalid slot".into())
        }
    }

    pub fn create_pattern(&mut self, track: usize, slot: usize, bars: u32) -> Result<(), String> {
        self.slot_ok(track, slot)?;
        let name = format!("Pattern {}", slot + 1);
        self.project.tracks[track].slots[slot] = Some(Pattern::new(name, bars));
        self.sync_pattern(track, slot);
        Ok(())
    }

    pub fn delete_pattern(&mut self, track: usize, slot: usize) -> Result<(), String> {
        self.slot_ok(track, slot)?;
        self.project.tracks[track].slots[slot] = None;
        let id = self.project.tracks[track].id;
        self.project
            .song
            .clips
            .retain(|c| !(c.track == id && c.slot == slot));
        self.sync_pattern(track, slot);
        self.sync_song();
        Ok(())
    }

    pub fn duplicate_pattern(
        &mut self,
        track: usize,
        from: usize,
        to: usize,
    ) -> Result<(), String> {
        self.slot_ok(track, from)?;
        self.slot_ok(track, to)?;
        let mut p = self
            .project
            .pattern(track, from)
            .cloned()
            .ok_or("empty pattern")?;
        p.name = format!("{} (copy)", p.name);
        self.project.tracks[track].slots[to] = Some(p);
        self.sync_pattern(track, to);
        Ok(())
    }

    /// Load a ready-made drum loop into a slot (creating the loop if the
    /// slot is empty). Hat lines layer over the loop; beats replace it.
    pub fn load_groove(
        &mut self,
        track: usize,
        slot: usize,
        groove: crate::grooves::GrooveId,
    ) -> Result<(), String> {
        self.slot_ok(track, slot)?;
        if self.project.tracks[track].kind != TrackKind::Drums {
            return Err("drum loops go on drum tracks".into());
        }
        let bar = self.project.bar_ticks();
        let p = groove.apply(self.project.pattern(track, slot), bar);
        self.project.tracks[track].slots[slot] = Some(p);
        self.sync_pattern(track, slot);
        Ok(())
    }

    fn edit_pattern<R>(
        &mut self,
        track: usize,
        slot: usize,
        f: impl FnOnce(&mut Pattern) -> R,
    ) -> Result<R, String> {
        self.slot_ok(track, slot)?;
        if self.groove {
            self.keep_take(track, slot);
        }
        let p = self
            .project
            .pattern_mut(track, slot)
            .ok_or("empty pattern")?;
        let r = f(p);
        self.sync_pattern(track, slot);
        Ok(r)
    }

    pub fn toggle_step(
        &mut self,
        track: usize,
        slot: usize,
        step: u32,
        pitch: u8,
        velocity: f32,
    ) -> Result<bool, String> {
        let bar = self.project.bar_ticks();
        self.edit_pattern(track, slot, |p| {
            p.toggle_step(step, pitch, velocity, TICKS_PER_STEP, bar)
        })
    }

    pub fn add_note(&mut self, track: usize, slot: usize, note: Note) -> Result<(), String> {
        let bar = self.project.bar_ticks();
        self.edit_pattern(track, slot, |p| p.add_note(note, bar))
    }

    pub fn remove_note(
        &mut self,
        track: usize,
        slot: usize,
        start: u32,
        pitch: u8,
    ) -> Result<bool, String> {
        self.edit_pattern(track, slot, |p| p.remove_note(start, pitch))
    }

    /// Piano roll drag: move and/or resize the note at (`start`, `pitch`).
    pub fn move_note(
        &mut self,
        track: usize,
        slot: usize,
        start: u32,
        pitch: u8,
        note: Note,
    ) -> Result<bool, String> {
        let bar = self.project.bar_ticks();
        self.edit_pattern(track, slot, |p| p.replace_note(start, pitch, note, bar))
    }

    /// Velocity lane: set the velocity of the notes starting in `from..to`.
    pub fn set_velocity(
        &mut self,
        track: usize,
        slot: usize,
        from: u32,
        to: u32,
        velocity: f32,
    ) -> Result<bool, String> {
        self.edit_pattern(track, slot, |p| p.set_velocity(from, to, velocity))
    }

    pub fn set_pattern_bars(&mut self, track: usize, slot: usize, bars: u32) -> Result<(), String> {
        let bar = self.project.bar_ticks();
        self.edit_pattern(track, slot, |p| p.set_bars(bars, bar))
    }

    pub fn quantize_pattern(&mut self, track: usize, slot: usize) -> Result<(), String> {
        let bar = self.project.bar_ticks();
        self.edit_pattern(track, slot, |p| p.quantize(TICKS_PER_STEP, 1.0, bar))
    }

    pub fn clear_pattern(&mut self, track: usize, slot: usize) -> Result<(), String> {
        self.edit_pattern(track, slot, |p| {
            p.notes.clear();
            p.automation.clear();
        })
    }

    pub fn rename_pattern(
        &mut self,
        track: usize,
        slot: usize,
        name: String,
    ) -> Result<(), String> {
        self.edit_pattern(track, slot, |p| p.name = name)
    }

    // ------------------------------------------------------ session / scenes

    pub fn launch(&mut self, track: usize, slot: Option<usize>) -> Result<(), String> {
        self.check_track(track)?;
        self.send(Command::Launch {
            track,
            slot,
            immediate: false,
        });
        Ok(())
    }

    pub fn launch_scene(&mut self, scene: usize) -> Result<(), String> {
        let s = self
            .project
            .scenes
            .get(scene)
            .ok_or("invalid section")?
            .clone();
        for track in 0..self.project.tracks.len() {
            let slot = s.slots.get(track).copied().flatten();
            self.send(Command::Launch {
                track,
                slot,
                immediate: false,
            });
        }
        Ok(())
    }

    pub fn stop_all_clips(&mut self) {
        for track in 0..self.project.tracks.len() {
            self.send(Command::Launch {
                track,
                slot: None,
                immediate: false,
            });
        }
    }

    pub fn add_scene(&mut self, name: String) -> usize {
        self.project.add_scene(name)
    }

    /// Add a section (a new grid row) at the bottom. Returns its row.
    pub fn add_section(&mut self, name: Option<String>) -> Result<usize, String> {
        let row = self.project.scenes.len();
        if row >= SLOTS_PER_TRACK {
            return Err(format!("{SLOTS_PER_TRACK} sections at most"));
        }
        // takes kept by Groove mode live past the rows: make room for the new one
        for t in 0..self.project.tracks.len() {
            if self.project.tracks[t].slots[row].is_some() {
                let free =
                    (row + 1..SLOTS_PER_TRACK).find(|&i| self.project.tracks[t].slots[i].is_none());
                if let Some(to) = free {
                    self.move_slot(t, row, to);
                }
            }
        }
        self.sync_song();
        Ok(self
            .project
            .add_scene(name.unwrap_or_else(|| format!("Section {}", row + 1))))
    }

    /// Delete a section with its loops; the rows below move up. Loops that
    /// were playing keep playing in their new row.
    pub fn delete_section(&mut self, row: usize) -> Result<(), String> {
        use std::sync::atomic::Ordering::Relaxed;
        if row >= self.project.scenes.len() {
            return Err("invalid section".into());
        }
        if self.project.scenes.len() <= 1 {
            return Err("a song needs at least one section".into());
        }
        let playing = self.status.playing.load(Relaxed);
        let active: Vec<i32> = (0..self.project.tracks.len())
            .map(|t| self.status.active_slot[t].load(Relaxed))
            .collect();
        self.project.remove_section(row);
        for t in 0..self.project.tracks.len() {
            self.sync_track_patterns(t);
            let a = active[t];
            if a < 0 || (a as usize) < row {
                continue;
            }
            let slot = (a as usize > row).then(|| a as usize - 1);
            self.send(Command::Launch {
                track: t,
                slot,
                immediate: !playing || slot.is_none(),
            });
        }
        self.sync_song();
        Ok(())
    }

    /// Store whatever is currently playing as a new scene.
    pub fn capture_scene(&mut self) -> usize {
        if self.project.scenes.len() >= SLOTS_PER_TRACK {
            return self.project.scenes.len() - 1;
        }
        let slots = (0..self.project.tracks.len())
            .map(|t| {
                let a = self.status.active_slot[t].load(std::sync::atomic::Ordering::Relaxed);
                (a >= 0).then_some(a as usize)
            })
            .collect();
        let name = format!("Section {}", self.project.scenes.len() + 1);
        self.project.scenes.push(Scene { name, slots });
        self.project.scenes.len() - 1
    }

    pub fn rename_scene(&mut self, scene: usize, name: String) -> Result<(), String> {
        self.project
            .scenes
            .get_mut(scene)
            .ok_or("invalid section")?
            .name = name;
        Ok(())
    }

    pub fn delete_scene(&mut self, scene: usize) -> Result<(), String> {
        self.delete_section(scene)
    }

    pub fn set_scene_slot(
        &mut self,
        scene: usize,
        track: usize,
        slot: Option<usize>,
    ) -> Result<(), String> {
        self.check_track(track)?;
        let s = self
            .project
            .scenes
            .get_mut(scene)
            .ok_or("invalid section")?;
        s.slots.resize(self.project.tracks.len(), None);
        s.slots[track] = slot;
        Ok(())
    }

    // ----------------------------------------------------------------- song

    fn sync_song(&mut self) {
        let mut rt = crate::export::song_runtime(&self.project);
        rt.looping = self.project.song.looping;
        self.send(Command::SetSong(Box::new(rt)));
    }

    pub fn song_mode(&self) -> bool {
        self.song_mode
    }

    pub fn start_bar(&self) -> u32 {
        self.start_bar
    }

    pub fn set_song_mode(&mut self, on: bool) {
        self.song_mode = on;
        self.send(Command::SetSongMode(on));
    }

    /// Move the start marker (and the playhead, while playing the song).
    pub fn seek(&mut self, bar: u32) {
        self.start_bar = bar.min(MAX_SONG_BARS - 1);
        let tick = self.start_bar as f64 * self.project.bar_ticks() as f64;
        self.send(Command::Seek { tick });
    }

    fn track_id(&self, track: usize) -> Result<u32, String> {
        self.check_track(track)?;
        Ok(self.project.tracks[track].id)
    }

    /// Place a loop on the timeline. `bars` = None uses the loop's length.
    pub fn add_clip(
        &mut self,
        track: usize,
        slot: usize,
        start: u32,
        bars: Option<u32>,
    ) -> Result<usize, String> {
        let id = self.track_id(track)?;
        let p = self
            .project
            .pattern(track, slot)
            .ok_or("that slot is empty")?;
        let bars = bars.unwrap_or(p.bars);
        let i = self.project.song.insert(SongClip {
            track: id,
            slot,
            start,
            bars,
        });
        self.sync_song();
        Ok(i)
    }

    /// Move and/or resize a clip (also to another lane of a compatible track).
    pub fn edit_clip(
        &mut self,
        index: usize,
        track: usize,
        start: u32,
        bars: u32,
    ) -> Result<usize, String> {
        let id = self.track_id(track)?;
        if index >= self.project.song.clips.len() {
            return Err("invalid clip".into());
        }
        let mut clip = self.project.song.clips.remove(index);
        if clip.track != id {
            if self.project.pattern(track, clip.slot).is_none() {
                self.project.song.insert(clip);
                return Err("that track has no such loop".into());
            }
            clip.track = id;
        }
        clip.start = start;
        clip.bars = bars;
        let i = self.project.song.insert(clip);
        self.sync_song();
        Ok(i)
    }

    pub fn delete_clip(&mut self, index: usize) -> Result<(), String> {
        if index >= self.project.song.clips.len() {
            return Err("invalid clip".into());
        }
        self.project.song.clips.remove(index);
        self.sync_song();
        Ok(())
    }

    /// Append a scene at the end of the song: every loop of that row, for
    /// `bars` bars (default: the longest loop, at least 4 bars).
    pub fn append_scene(&mut self, scene: usize, bars: Option<u32>) -> Result<(), String> {
        let sc = self
            .project
            .scenes
            .get(scene)
            .ok_or("invalid section")?
            .clone();
        let parts: Vec<(u32, usize, u32)> = self
            .project
            .tracks
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                let slot = sc.slots.get(i).copied().flatten()?;
                let p = t.slots.get(slot)?.as_ref()?;
                Some((t.id, slot, p.bars))
            })
            .collect();
        if parts.is_empty() {
            return Err("that section has no loops".into());
        }
        let longest = parts.iter().map(|p| p.2).max().unwrap_or(1);
        let bars = bars.unwrap_or_else(|| longest.max(4).div_ceil(longest) * longest);
        let start = self.project.song.length_bars();
        for (track, slot, _) in parts {
            self.project.song.insert(SongClip {
                track,
                slot,
                start,
                bars,
            });
        }
        self.sync_song();
        Ok(())
    }

    pub fn clear_song(&mut self) {
        self.project.song.clips.clear();
        self.sync_song();
    }

    /// Fade in/out lengths in bars (0 = off).
    pub fn set_song_fades(&mut self, fade_in: u32, fade_out: u32) {
        self.project.song.fade_in = fade_in.min(64);
        self.project.song.fade_out = fade_out.min(64);
        self.sync_song();
    }

    pub fn set_song_looping(&mut self, looping: bool) {
        self.project.song.looping = looping;
        self.sync_song();
    }

    /// Song recording: each new bar, write what every track is playing.
    fn record_song_bar(&mut self) -> bool {
        use std::sync::atomic::Ordering::Relaxed;
        if !self.record_song || self.song_mode || !self.status.playing.load(Relaxed) {
            self.last_recorded_bar = None;
            self.recording_at = None;
            return false;
        }
        let bar = (self.status.song_tick() / self.project.bar_ticks() as f64).floor() as u32;
        if self.last_recorded_bar == Some(bar) {
            return false;
        }
        let first = self.groove && self.last_recorded_bar.is_none();
        if first {
            // a new take goes right after what the tape already holds
            self.tape_offset = self.project.song.length_bars() as i64 - bar as i64;
        }
        self.last_recorded_bar = Some(bar);
        let at = if self.groove {
            (bar as i64 + self.tape_offset).max(0) as u32
        } else {
            self.start_bar + bar
        };
        self.recording_at = Some(at);
        if at >= MAX_SONG_BARS {
            return false;
        }
        let mut changed = false;
        for i in 0..self.project.tracks.len() {
            let a = self.status.active_slot[i].load(Relaxed);
            if a >= 0 {
                let id = self.project.tracks[i].id;
                self.project.song.record_bar(id, a as usize, at);
                changed = true;
            }
        }
        if first && !changed {
            // nothing sounds yet: the take starts at the first bar that does
            self.last_recorded_bar = None;
            self.recording_at = None;
        }
        if changed {
            self.sync_song();
        }
        changed
    }

    /// Groove mode, before `slot` of `track` changes: if the timeline holds
    /// bars of it that were already heard (every bar when not recording, the
    /// bars before the one being recorded otherwise), move those bars to a
    /// copy of the loop as it is now, so the tape keeps sounding the same.
    /// The copy lives in a slot past the visible rows, reusing one that holds
    /// the same notes. With no slot left, the edit reaches the tape too.
    fn keep_take(&mut self, track: usize, slot: usize) {
        let now = self.recording_at.unwrap_or(u32::MAX);
        let id = self.project.tracks[track].id;
        let heard = |c: &SongClip| c.track == id && c.slot == slot && c.start < now;
        if !self.project.song.clips.iter().any(heard) {
            return;
        }
        let Some(pattern) = self.project.pattern(track, slot).cloned() else {
            return;
        };
        let rows = self.rows();
        let slots = &self.project.tracks[track].slots;
        let same = (rows..SLOTS_PER_TRACK).find(|&i| {
            slots[i].as_ref().is_some_and(|p| {
                p.notes == pattern.notes
                    && p.bars == pattern.bars
                    && p.automation == pattern.automation
            })
        });
        let Some(take) =
            same.or_else(|| (rows..SLOTS_PER_TRACK).rev().find(|&i| slots[i].is_none()))
        else {
            return;
        };
        if same.is_none() {
            let n = slots[rows..].iter().filter(|p| p.is_some()).count() + 1;
            let mut p = pattern;
            p.name = format!("{} · take {n}", p.name);
            self.project.tracks[track].slots[take] = Some(p);
            self.sync_pattern(track, take);
        }
        let mut rest = Vec::new();
        for c in self.project.song.clips.iter_mut().filter(|c| heard(c)) {
            if c.end() > now {
                // the bar being recorded onwards keeps following the edit
                rest.push(SongClip {
                    start: now,
                    bars: c.end() - now,
                    ..*c
                });
                c.bars = now - c.start;
            }
            c.slot = take;
        }
        for c in rest {
            self.project.song.insert(c);
        }
        self.sync_song();
    }

    /// Move a loop and its timeline clips to another (empty) slot.
    fn move_slot(&mut self, track: usize, from: usize, to: usize) {
        let id = self.project.tracks[track].id;
        let p = self.project.tracks[track].slots[from].take();
        self.project.tracks[track].slots[to] = p;
        for c in self.project.song.clips.iter_mut() {
            if c.track == id && c.slot == from {
                c.slot = to;
            }
        }
        self.sync_pattern(track, from);
        self.sync_pattern(track, to);
    }

    // -------------------------------------------------------------- capture

    /// Turn the last thing played on `track` into a loop in the first free
    /// slot (or the given one) and launch it. Returns the slot.
    pub fn capture(&mut self, track: usize, slot: Option<usize>) -> Result<usize, String> {
        self.check_track(track)?;
        self.poll();
        let id = self.project.tracks[track].id;
        let buf = self
            .capture
            .get(&id)
            .ok_or("nothing to capture yet: play something first")?;
        let mut pattern = buf
            .capture(8, self.quantize, self.project.bar_ticks())
            .ok_or("nothing to capture yet: play something first")?;
        let slot = match slot {
            Some(s) => s,
            None => self
                .free_slot(track)
                .ok_or("no free slots on this track: add a section")?,
        };
        self.slot_ok(track, slot)?;
        pattern.name = format!("Capture {}", slot + 1);
        self.project.tracks[track].slots[slot] = Some(pattern);
        self.sync_pattern(track, slot);
        self.capture.get_mut(&id).unwrap().clear();
        let playing = self
            .status
            .playing
            .load(std::sync::atomic::Ordering::Relaxed);
        self.send(Command::Launch {
            track,
            slot: Some(slot),
            immediate: !playing,
        });
        Ok(slot)
    }

    // ------------------------------------------------------- macros/automation

    pub fn set_macro(&mut self, track: usize, param: MacroParam, value: f32) -> Result<(), String> {
        self.check_track(track)?;
        self.project.tracks[track].macros.set(param, value);
        let macros = self.project.tracks[track].macros;
        self.send(Command::SetMacros { track, macros });
        if self.record_automation
            && self
                .status
                .playing
                .load(std::sync::atomic::Ordering::Relaxed)
        {
            let active = self.status.active_slot[track].load(std::sync::atomic::Ordering::Relaxed);
            if active >= 0 {
                let slot = active as usize;
                let origin = self.status.origin(track);
                let song_tick = self.status.song_tick();
                let bar = self.project.bar_ticks();
                if let Some(p) = self.project.pattern_mut(track, slot) {
                    let len = p.length_ticks(bar);
                    let local = ((song_tick - origin).max(0.0) as u64 % len as u64) as u32;
                    let prev = self.last_auto_tick.get(&(track, param)).copied();
                    p.lane_mut(param).write(prev, local, value);
                    self.last_auto_tick.insert((track, param), local);
                    self.sync_pattern(track, slot);
                    self.revision += 1;
                }
            }
        }
        Ok(())
    }

    /// The user grabbed/released a knob: suspend automation playback for it.
    pub fn touch(&mut self, track: usize, param: MacroParam, touching: bool) -> Result<(), String> {
        self.check_track(track)?;
        self.send(Command::Touch {
            track,
            param,
            touching,
        });
        self.last_auto_tick.remove(&(track, param));
        Ok(())
    }

    pub fn set_automation(
        &mut self,
        track: usize,
        slot: usize,
        param: MacroParam,
        points: Vec<AutoPoint>,
    ) -> Result<(), String> {
        let bar = self.project.bar_ticks();
        self.edit_pattern(track, slot, |p| {
            let len = p.length_ticks(bar);
            let pts = points.into_iter().filter(|pt| pt.tick < len).collect();
            p.lane_mut(param).set_points(pts);
            if p.lane(param).map(|l| l.points.is_empty()).unwrap_or(false) {
                p.automation.retain(|l| l.param != param);
            }
        })
    }

    pub fn clear_automation(
        &mut self,
        track: usize,
        slot: usize,
        param: Option<MacroParam>,
    ) -> Result<(), String> {
        self.edit_pattern(track, slot, |p| match param {
            Some(param) => p.automation.retain(|l| l.param != param),
            None => p.automation.clear(),
        })
    }

    // ---------------------------------------------------------------- scale

    /// Change the scale of the project (`track` = None) or of one track,
    /// moving every melodic note to the same degree of the new scale.
    pub fn set_scale(&mut self, scale: Scale, track: Option<usize>) -> Result<(), String> {
        let targets: Vec<(usize, Scale)> = match track {
            Some(t) => {
                self.check_track(t)?;
                vec![(t, self.project.track_scale(t))]
            }
            None => (0..self.project.tracks.len())
                .filter(|&t| self.project.tracks[t].scale.is_none())
                .map(|t| (t, self.project.scale))
                .collect(),
        };
        match track {
            Some(t) => self.project.tracks[t].scale = Some(scale),
            None => self.project.scale = scale,
        }
        for (t, old) in targets {
            if self.project.tracks[t].kind == TrackKind::Drums || old == scale {
                continue;
            }
            for slot in self.project.tracks[t].slots.iter_mut().flatten() {
                for n in &mut slot.notes {
                    n.pitch = scale.remap_from(&old, n.pitch);
                }
                slot.sort();
            }
            self.sync_track_patterns(t);
        }
        Ok(())
    }

    /// Make a track follow the project scale again.
    pub fn clear_track_scale(&mut self, track: usize) -> Result<(), String> {
        self.check_track(track)?;
        let project_scale = self.project.scale;
        self.set_scale(project_scale, Some(track))?;
        self.project.tracks[track].scale = None;
        Ok(())
    }

    // ------------------------------------------------------------- sampling

    /// Store a raw recording. Returns the new sample id and detected root.
    pub fn add_recording(
        &mut self,
        raw: &[f32],
        sample_rate: u32,
    ) -> Result<(SampleId, Option<u8>), String> {
        let sample =
            sampling::prepare(raw, sample_rate).ok_or("the recording is empty or too quiet")?;
        let pitch = sampling::detect_pitch(&sample);
        let id = self.next_sample_id;
        self.next_sample_id += 1;
        self.samples.insert(id, Arc::new(sample));
        Ok((id, pitch))
    }

    pub fn sample_spec(
        &self,
        sample: SampleId,
        mode: SampleMode,
    ) -> Result<InstrumentSpec, String> {
        let data = self.samples.get(&sample).ok_or("unknown sample")?;
        Ok(match mode {
            SampleMode::Melodic => InstrumentSpec::SampleMelodic {
                sample,
                root: sampling::detect_pitch(data).unwrap_or(60),
            },
            SampleMode::Granular => InstrumentSpec::Granular {
                texture: TextureKind::Cloud,
                sample: Some(sample),
            },
            SampleMode::Kit => InstrumentSpec::SampleKit {
                sample,
                slices: sampling::slice_onsets(data),
            },
        })
    }

    /// Create a new track that plays a recorded sample.
    pub fn add_sample_track(
        &mut self,
        sample: SampleId,
        mode: SampleMode,
    ) -> Result<usize, String> {
        let spec = self.sample_spec(sample, mode)?;
        let kind = if mode == SampleMode::Kit {
            TrackKind::Drums
        } else {
            TrackKind::Melodic
        };
        let idx = self.add_track(kind, spec)?;
        self.project.tracks[idx].name = match mode {
            SampleMode::Kit => format!("Kit mic #{sample}"),
            SampleMode::Melodic => format!("Mic #{sample}"),
            SampleMode::Granular => format!("Textura mic #{sample}"),
        };
        Ok(idx)
    }

    pub fn import_wav(&mut self, bytes: &[u8]) -> Result<(SampleId, Option<u8>), String> {
        let s = sampling::from_wav_bytes(bytes)?;
        self.add_recording(&s.frames, s.sample_rate)
    }

    // ---------------------------------------------------------- persistence

    pub fn to_file(&self) -> ProjectFile {
        let b64 = base64::engine::general_purpose::STANDARD;
        // only keep samples that are referenced
        let used: Vec<SampleId> = self
            .project
            .tracks
            .iter()
            .filter_map(|t| match &t.instrument {
                InstrumentSpec::SampleKit { sample, .. }
                | InstrumentSpec::SampleMelodic { sample, .. }
                | InstrumentSpec::Granular {
                    sample: Some(sample),
                    ..
                } => Some(*sample),
                _ => None,
            })
            .collect();
        let samples = self
            .samples
            .iter()
            .filter(|(id, _)| used.contains(id))
            .map(|(id, s)| (*id, b64.encode(sampling::to_wav_bytes(s))))
            .collect();
        ProjectFile {
            format: "yggdrasil/1".into(),
            project: self.project.clone(),
            samples,
        }
    }

    pub fn save_json(&self) -> Result<String, String> {
        serde_json::to_string(&self.to_file()).map_err(|e| e.to_string())
    }

    pub fn open_json(&mut self, json: &str) -> Result<(), String> {
        let file: ProjectFile =
            serde_json::from_str(json).map_err(|e| format!("invalid file: {e}"))?;
        let b64 = base64::engine::general_purpose::STANDARD;
        let mut samples = HashMap::new();
        for (id, data) in file.samples {
            let bytes = b64.decode(data).map_err(|e| e.to_string())?;
            samples.insert(id, Arc::new(sampling::from_wav_bytes(&bytes)?));
        }
        let mut project = file.project;
        project.normalize();
        self.load(project, samples);
        Ok(())
    }

    pub fn new_project(&mut self) {
        self.load(Project::blank(), HashMap::new());
    }

    pub fn open_demo(&mut self) {
        self.load(Project::starter(), HashMap::new());
    }

    /// Insert a diatonic chord as one project edit / undo transaction.
    pub fn add_chord(
        &mut self,
        track: usize,
        slot: usize,
        degree: i32,
        octave: i32,
        kind: ChordKind,
        start: u32,
        length: u32,
    ) -> Result<(), String> {
        self.slot_ok(track, slot)?;
        if self.project.tracks[track].kind != TrackKind::Melodic {
            return Err("Chords need an instrument track".into());
        }
        let scale = self.project.track_scale(track);
        let pitches = scale.chord(degree, octave.clamp(0, 7), kind);
        let bar = self.project.bar_ticks();
        self.edit_pattern(track, slot, |p| {
            let start = start.min(p.length_ticks(bar).saturating_sub(1));
            let length = length.max(1).min(p.length_ticks(bar) - start);
            for pitch in pitches {
                p.add_note(
                    Note {
                        start,
                        length,
                        pitch,
                        velocity: 0.8,
                    },
                    bar,
                );
            }
        })
    }

    // --------------------------------------------------------------- status

    /// Drain engine events (capture memory, overdub recording) and garbage.
    /// Call regularly (e.g. 30-60 Hz) from a non-audio thread.
    pub fn poll(&mut self) -> bool {
        self.collect_garbage();
        let mut changed = false;
        while let Ok(ev) = self.events.pop() {
            match ev {
                EngineEvent::LiveNote {
                    track,
                    pitch,
                    velocity,
                    on,
                    tick,
                    playing,
                } => {
                    let Some(t) = self.project.tracks.get(track) else {
                        continue;
                    };
                    let id = t.id;
                    self.capture.entry(id).or_default().push(LiveEvent {
                        pitch,
                        velocity,
                        on,
                        tick,
                        playing,
                    });
                    if on {
                        self.note_onset = self.note_onset.wrapping_add(1);
                    }
                    if self.record_notes && playing {
                        changed |= self.record_live(track, pitch, velocity, on, tick);
                    }
                }
            }
        }
        changed |= self.grow_take();
        changed |= self.record_song_bar();
        changed |= self.midi_tick();
        if changed {
            self.revision += 1;
        }
        changed
    }

    /// A growing take doubles its loop half a bar before the end, as long
    /// as the recording goes on (up to 8 bars). The new bars start empty.
    fn grow_take(&mut self) -> bool {
        use std::sync::atomic::Ordering::Relaxed;
        let Some((track, slot)) = self.growing else {
            return false;
        };
        let active = self
            .status
            .active_slot
            .get(track)
            .map_or(-1, |a| a.load(Relaxed));
        // (the engine may not have reported the launch of the new loop yet)
        let launched_elsewhere = active >= 0 && active != slot as i32;
        if !self.record_notes || !self.status.playing.load(Relaxed) || launched_elsewhere {
            self.growing = None;
            return false;
        }
        if active < 0 {
            return false;
        }
        let bar = self.project.bar_ticks();
        let elapsed = self.status.song_tick() - self.status.origin(track);
        let Some(p) = self.project.pattern_mut(track, slot) else {
            self.growing = None;
            return false;
        };
        if elapsed < (p.length_ticks(bar) as f64 - bar as f64 / 2.0) {
            return false;
        }
        if p.bars >= 8 {
            self.growing = None;
            return false;
        }
        p.bars *= 2;
        self.sync_pattern(track, slot);
        true
    }

    fn record_live(&mut self, track: usize, pitch: u8, velocity: f32, on: bool, tick: f64) -> bool {
        use std::sync::atomic::Ordering::Relaxed;
        let active = self.status.active_slot[track].load(Relaxed);
        if on {
            let slot = if active >= 0 {
                active as usize
            } else {
                // nothing playing: start a fresh loop in the section on screen
                // (or the first free one); it grows while the take goes on
                let shown = self
                    .focus_slot
                    .filter(|&s| s < self.rows() && self.project.tracks[track].slots[s].is_none());
                let Some(free) = shown.or_else(|| self.free_slot(track)) else {
                    return false;
                };
                self.growing = Some((track, free));
                self.project.tracks[track].slots[free] =
                    Some(Pattern::new(format!("Take {}", free + 1), 1));
                self.sync_pattern(track, free);
                self.send(Command::Launch {
                    track,
                    slot: Some(free),
                    immediate: true,
                });
                free
            };
            self.recording_notes
                .insert((track, pitch), (slot, velocity, tick));
            false
        } else if let Some((slot, note_velocity, start_tick)) =
            self.recording_notes.remove(&(track, pitch))
        {
            let origin = self.status.origin(track);
            let quantize = self.quantize;
            let bar = self.project.bar_ticks();
            let Some(p) = self.project.pattern_mut(track, slot) else {
                return false;
            };
            let len = p.length_ticks(bar) as f64;
            let rel = ((start_tick - origin).rem_euclid(len)).round() as u32;
            let mut start = rel;
            let mut length = (tick - start_tick).round().max(1.0) as u32;
            if quantize {
                start = quantize_tick(start, TICKS_PER_STEP, 1.0);
                length = quantize_tick(length, TICKS_PER_STEP, 1.0).max(TICKS_PER_STEP / 2);
            }
            p.add_note(
                Note {
                    start,
                    length,
                    pitch,
                    velocity: note_velocity,
                },
                bar,
            );
            self.sync_pattern(track, slot);
            true
        } else {
            false
        }
    }

    pub fn status(&self) -> Status {
        use std::sync::atomic::Ordering::Relaxed;
        let s = &self.status;
        let tick = s.song_tick();
        let bar = self.project.bar_ticks();
        let tracks = (0..self.project.tracks.len().min(MAX_TRACKS))
            .map(|i| {
                let a = s.active_slot[i].load(Relaxed);
                let q = s.queued_slot[i].load(Relaxed);
                let active = (a >= 0).then_some(a as usize);
                let progress = active
                    .and_then(|slot| self.project.pattern(i, slot))
                    .map(|p| {
                        let len = p.length_ticks(bar) as f64;
                        ((tick - s.origin(i)).rem_euclid(len) / len) as f32
                    })
                    .unwrap_or(0.0);
                TrackStatus {
                    active,
                    queued: (q >= 0).then_some(q as usize),
                    queued_stop: q == SLOT_STOP,
                    level: s.level(i),
                    macros: std::array::from_fn(|k| s.macro_value(i, MacroParam::ALL[k])),
                    progress,
                }
            })
            .collect();
        let meter = self.project.meter;
        let beats = (tick / meter.beat_ticks() as f64).floor() as u32;
        Status {
            playing: s.playing.load(Relaxed),
            count_in: s.count_in.load(Relaxed),
            note_onset: self.note_onset,
            tick,
            bar: beats / meter.beats + 1,
            beat: beats % meter.beats + 1,
            master_level: s.master_level(),
            tracks,
            song_mode: self.song_mode,
            metronome: self.metronome,
            revision: self.revision,
        }
    }
}

fn chord_label(pitches: &[u8]) -> String {
    let root = pitches[0];
    let name = crate::theory::NOTE_NAMES[(root % 12) as usize];
    let third = pitches.get(1).map(|p| p - root);
    let fifth = pitches.get(2).map(|p| p - root);
    let suffix = match (third, fifth) {
        (Some(4), Some(7)) => "",
        (Some(3), Some(7)) => "m",
        (Some(3), Some(6)) => "dim",
        (Some(4), Some(8)) => "aug",
        (Some(2), _) => "sus2",
        (Some(5), _) => "sus4",
        (Some(7), _) => "5",
        _ => "",
    };
    let seventh = if pitches.len() >= 4 { "7" } else { "" };
    format!("{name}{suffix}{seventh}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Engine;
    use crate::theory::ScaleKind;

    fn setup() -> (Controller, Engine) {
        let (engine, ep) = Engine::new(48000.0);
        let c = Controller::new(ep, 48000.0);
        (c, engine)
    }

    fn run(engine: &mut Engine, seconds: f32) {
        let mut buf = vec![0.0; 2 * 480];
        for _ in 0..((seconds * 100.0) as usize) {
            engine.process(&mut buf, 2);
        }
    }

    #[test]
    fn a_designed_copy_sounds_like_its_preset() {
        use crate::instruments::Instrument;
        let render = |spec: &InstrumentSpec| {
            let mut i = Instrument::build(spec, None, 48000.0);
            i.note_on(48, 0.8);
            let mut out = vec![0.0f32; 8192];
            i.render(&mut out);
            out
        };
        let (mut c, _e) = setup();
        let before = c.project.tracks[1].instrument.clone();
        c.design_sound(1).unwrap();
        let designed = c.project.tracks[1].instrument.clone();
        assert!(matches!(designed, InstrumentSpec::CustomSynth { .. }));
        assert_eq!(render(&before), render(&designed));
        // a synthesized kit too, pad for pad
        c.swap_instrument(
            0,
            InstrumentSpec::Drums {
                kit: DrumKitId::Tr808,
            },
        )
        .unwrap();
        let kit = c.project.tracks[0].instrument.clone();
        c.design_sound(0).unwrap();
        let InstrumentSpec::CustomKit { pads, names, .. } = c.project.tracks[0].instrument.clone()
        else {
            panic!("a kit")
        };
        assert_eq!((pads.len(), names.len()), (DRUM_PADS, DRUM_PADS));
        let hit = |spec: &InstrumentSpec| {
            let mut i = Instrument::build(spec, None, 48000.0);
            i.note_on(DRUM_BASE_NOTE, 1.0);
            let mut out = vec![0.0f32; 4096];
            i.render(&mut out);
            out
        };
        assert_eq!(hit(&kit), hit(&c.project.tracks[0].instrument));
    }

    #[test]
    fn sound_design_edits_live_undoes_and_saves_to_a_file() {
        let (mut c, _e) = setup();
        assert!(
            c.set_sound_param(1, None, "cutoff", 3000.0).is_err(),
            "design first"
        );
        c.design_sound(1).unwrap();
        c.checkpoint();
        c.set_sound_param(1, None, "cutoff", 99999.0).unwrap();
        c.set_sound_param(1, None, "saw", 0.8).unwrap();
        let InstrumentSpec::CustomSynth { patch, .. } = &c.project.tracks[1].instrument else {
            panic!()
        };
        assert_eq!((patch.cutoff, patch.saw), (16000.0, 0.8));
        c.undo().unwrap();
        let InstrumentSpec::CustomSynth { patch, .. } = &c.project.tracks[1].instrument else {
            panic!()
        };
        assert!(patch.cutoff < 16000.0);
        c.rename_sound(1, None, "  Wobble bass ".into()).unwrap();

        // a sound file goes from one track to another, but only to its kind
        let file = c.sound_file(1).unwrap();
        assert!(file.contains("\"yggsound\""));
        c.load_sound_file(3, &file).unwrap();
        assert_eq!(c.project.tracks[3].instrument.label(), "Wobble bass");
        assert!(c
            .load_sound_file(0, &file)
            .unwrap_err()
            .contains("instrument track"));
        assert!(c.load_sound_file(1, "{}").is_err());
        // a sound saved when the app was called TinyDaw still loads
        let old = file.replace("\"yggsound\"", "\"tinysound\"");
        c.load_sound_file(3, &old).unwrap();
        assert!(c
            .load_sound_file(
                1,
                r#"{"format":"yggsound","version":99,"sound":{"type":"synth","preset":"keys"}}"#
            )
            .unwrap_err()
            .contains("newer"));

        // kit pads: edit, rename, and the play surface shows the new name
        c.design_sound(0).unwrap();
        c.set_sound_param(0, Some(2), "noise_decay", 0.5).unwrap();
        c.rename_sound(0, Some(2), "Big snare".into()).unwrap();
        assert_eq!(c.pad_layout(0).unwrap()[2].label, "Big snare");
        let kit = c.sound_file(0).unwrap();
        assert!(c
            .load_sound_file(2, &kit)
            .unwrap_err()
            .contains("drum track"));
        // the project (with its designed sounds) survives a save and reload
        let json = c.save_json().unwrap();
        c.open_json(&json).unwrap();
        assert!(matches!(
            c.project.tracks[0].instrument,
            InstrumentSpec::CustomKit { .. }
        ));
    }

    #[test]
    fn a_new_take_grows_while_recording_and_keeps_its_length() {
        let (mut c, mut e) = setup();
        c.load(Project::blank(), HashMap::new());
        c.focus_slot = Some(0);
        c.record_notes = true;
        c.play();
        let mut step = |c: &mut Controller, e: &mut Engine, s: f32| {
            for _ in 0..(s * 20.0) as usize {
                run(e, 0.05);
                c.poll();
            }
        };
        step(&mut c, &mut e, 0.2);
        c.pad_on(1, 0, 0.8).unwrap();
        step(&mut c, &mut e, 0.3);
        c.pad_off(1, 0).unwrap();
        // 120 BPM: a bar is 2 s; after ~3 bars the take has grown to 4
        step(&mut c, &mut e, 5.6);
        assert_eq!(c.project.pattern(1, 0).unwrap().bars, 4);
        // punch out: the loop keeps its length
        c.record_notes = false;
        step(&mut c, &mut e, 4.0);
        assert_eq!(c.project.pattern(1, 0).unwrap().bars, 4);
        assert_eq!(c.project.pattern(1, 0).unwrap().notes.len(), 1);
    }

    #[test]
    fn stop_finishes_a_held_recorded_note() {
        let (mut c, mut e) = setup();
        c.record_notes = true;
        c.play();
        run(&mut e, 0.1);
        c.pad_on(1, 0, 0.8).unwrap();
        run(&mut e, 0.2);
        c.stop();
        run(&mut e, 0.01);
        let notes = &c.project.pattern(1, 0).unwrap().notes;
        assert_eq!(notes.len(), 1);
        assert!(notes[0].length > 0);
        c.pad_off(1, 0).unwrap();
        run(&mut e, 0.01);
        c.poll();
        assert_eq!(c.project.pattern(1, 0).unwrap().notes.len(), 1);
    }

    #[test]
    fn chord_insertion_preserves_notes_clamps_length_and_undoes_as_one_edit() {
        let (mut c, _e) = setup();
        c.create_pattern(2, 3, 1).unwrap();
        c.add_note(
            2,
            3,
            Note {
                start: 0,
                length: 24,
                pitch: 48,
                velocity: 0.7,
            },
        )
        .unwrap();
        let before = c.project.clone();
        c.checkpoint();
        c.add_chord(2, 3, 0, 4, ChordKind::Triad, 96, 999).unwrap();
        let notes = &c.project.pattern(2, 3).unwrap().notes;
        assert_eq!(notes.len(), 4);
        assert!(notes.iter().any(|n| n.pitch == 48));
        for pitch in [60, 64, 67] {
            assert!(notes
                .iter()
                .any(|n| n.pitch == pitch && n.start == 96 && n.length == 288));
        }
        c.undo().unwrap();
        assert_eq!(c.project, before);
        assert!(c.add_chord(0, 3, 0, 4, ChordKind::Triad, 0, 96).is_err());
        c.new_project();
        assert_eq!(c.project.scale, Scale::new(9, ScaleKind::Minor));
        assert!(c.project.song.clips.is_empty());
        assert!(c
            .project
            .tracks
            .iter()
            .all(|t| t.slots.iter().all(Option::is_none)));
        c.open_demo();
        assert!(!c.project.song.clips.is_empty());
    }

    #[test]
    fn count_in_only_applies_to_armed_recording() {
        let (mut c, mut e) = setup();
        c.count_in_bars = 1;
        c.play();
        run(&mut e, 0.1);
        assert!(c.status().playing);
        assert_eq!(c.status().count_in, 0);
        c.stop();
        run(&mut e, 0.01);
        c.record_notes = true;
        c.play();
        run(&mut e, 0.1);
        assert!(!c.status().playing);
        assert!(c.status().count_in > 0);
        c.stop();
        run(&mut e, 0.01);
        assert_eq!(c.status().count_in, 0);
    }

    #[test]
    fn new_project_is_a_minor_and_saved_scale_is_respected() {
        let (mut c, _e) = setup();
        assert_eq!(c.project.scale, Scale::new(0, ScaleKind::Major));
        let mut saved = c.project.clone();
        saved.scale = Scale::new(2, ScaleKind::Dorian);
        c.load(saved, HashMap::new());
        assert_eq!(c.project.scale, Scale::new(2, ScaleKind::Dorian));
        c.new_project();
        assert_eq!(c.project.scale, Scale::new(9, ScaleKind::Minor));
    }

    #[test]
    fn part_levels_follow_the_instrument() {
        let (mut c, _engine) = setup();
        assert_eq!(c.parts(0).unwrap().len(), DRUM_PADS);
        c.checkpoint();
        c.set_part(0, 3, 0.2).unwrap();
        assert_eq!(c.parts(0).unwrap()[3].level, 0.2);
        // pad 3 of one kit is pad 3 of another: the mix stays
        c.swap_instrument(
            0,
            InstrumentSpec::Drums {
                kit: DrumKitId::Trap,
            },
        )
        .unwrap();
        assert_eq!(c.project.tracks[0].part_level(3), 0.2);
        // a synth with other layers starts flat
        c.set_part(1, 2, 0.1).unwrap();
        c.swap_instrument(
            1,
            InstrumentSpec::Synth {
                preset: SynthPreset::Pad,
            },
        )
        .unwrap();
        assert!(c.project.tracks[1].parts.is_empty());
        assert!(c.set_part(0, MAX_PARTS, 0.5).is_err());
        c.undo().unwrap();
        assert_eq!(c.project.tracks[0].part_level(3), PART_UNITY);
        // saved with the project
        let json = serde_json::to_string(&c.project).unwrap();
        assert!(!json.contains("\"parts\""));
    }

    #[test]
    fn grooves_load_on_drum_tracks_only() {
        use crate::grooves::GrooveId;
        let (mut c, _engine) = setup();
        // the starter's last row is empty: a beat creates the loop
        let row = c.project.scenes.len() - 1;
        assert!(c.project.pattern(0, row).is_none());
        c.load_groove(0, row, GrooveId::TrapHalfTime).unwrap();
        assert_eq!(c.project.pattern(0, row).unwrap().name, "Trap half-time");
        // hats layer over it: the kick is still there
        c.load_groove(0, row, GrooveId::HatsTriplets).unwrap();
        let p = c.project.pattern(0, row).unwrap();
        assert!(p.notes.iter().any(|n| n.pitch == DRUM_BASE_NOTE));
        assert!(c.load_groove(1, row, GrooveId::BoomBap).is_err());
        assert!(c
            .load_groove(0, SLOTS_PER_TRACK, GrooveId::BoomBap)
            .is_err());
    }

    #[test]
    fn capture_end_to_end_while_stopped() {
        let (mut c, mut e) = setup();
        let bass = 1;
        for pad in [0usize, 2, 4, 2] {
            c.pad_on(bass, pad, 0.9).unwrap();
            run(&mut e, 0.1);
            c.pad_off(bass, pad).unwrap();
            run(&mut e, 0.15);
        }
        let slot = c.capture(bass, None).unwrap();
        assert_eq!(slot, 0);
        let p = c.project.pattern(bass, 0).unwrap();
        assert_eq!(p.notes.len(), 4);
        assert_eq!(p.bars, 1);
        // C major, bass plays from octave 2 -> C2 E2 G2 E2
        let pitches: Vec<u8> = p.notes.iter().map(|n| n.pitch).collect();
        assert_eq!(pitches, vec![36, 40, 43, 40]);
        // the captured loop was launched
        run(&mut e, 0.05);
        assert_eq!(c.status().tracks[bass].active, Some(0));
    }

    #[test]
    fn chord_pads() {
        let (mut c, _e) = setup();
        c.set_chord_mode(2, Some(ChordKind::Triad)).unwrap();
        let layout = c.pad_layout(2).unwrap();
        assert_eq!(layout[0].label, "C");
        assert_eq!(layout[0].pitches, vec![60, 64, 67]);
        assert_eq!(layout[2].label, "Em");
    }

    #[test]
    fn hot_scale_change_remaps_melodic_only() {
        let (mut c, _e) = setup();
        c.set_scale(Scale::new(9, ScaleKind::Minor), None).unwrap();
        c.create_pattern(2, 0, 1).unwrap();
        c.add_note(
            2,
            0,
            Note {
                start: 0,
                length: 24,
                pitch: 72,
                velocity: 1.0,
            },
        )
        .unwrap(); // C5 = b3 in A minor
        let drums_before = c.project.tracks[0].slots[0].clone();
        c.set_scale(Scale::new(9, ScaleKind::Major), None).unwrap();
        assert_eq!(c.project.pattern(2, 0).unwrap().notes[0].pitch, 73); // C#5
        assert_eq!(c.project.tracks[0].slots[0], drums_before);
        // pad layout follows the new scale
        assert_eq!(c.pad_layout(2).unwrap()[2].pitches, vec![73]);
    }

    #[test]
    fn records_automation_while_playing() {
        let (mut c, mut e) = setup();
        c.launch(0, Some(1)).unwrap();
        c.play();
        run(&mut e, 0.1);
        c.record_automation = true;
        c.touch(0, MacroParam::Cutoff, true).unwrap();
        for i in 0..10 {
            c.set_macro(0, MacroParam::Cutoff, i as f32 / 10.0).unwrap();
            run(&mut e, 0.05);
        }
        c.touch(0, MacroParam::Cutoff, false).unwrap();
        let lane = c
            .project
            .pattern(0, 1)
            .unwrap()
            .lane(MacroParam::Cutoff)
            .unwrap();
        assert!(lane.points.len() >= 8, "{:?}", lane.points);
        assert!(lane.points.windows(2).all(|w| w[0].tick < w[1].tick));
    }

    #[test]
    fn overdub_records_notes() {
        let (mut c, mut e) = setup();
        c.record_notes = true;
        c.play();
        run(&mut e, 0.05);
        c.pad_on(1, 0, 1.0).unwrap();
        run(&mut e, 0.2);
        c.pad_off(1, 0).unwrap();
        run(&mut e, 0.05);
        assert!(c.poll());
        let p = c
            .project
            .pattern(1, 0)
            .expect("a recording pattern was created");
        assert_eq!(p.notes.len(), 1);
        assert_eq!(p.notes[0].pitch, 36);
    }

    #[test]
    fn save_and_open_with_samples() {
        let (mut c, _e) = setup();
        let raw: Vec<f32> = (0..24000)
            .map(|i| (i as f32 * 2.0 * std::f32::consts::PI * 220.0 / 48000.0).sin() * 0.5)
            .collect();
        let (id, pitch) = c.add_recording(&raw, 48000).unwrap();
        assert_eq!(pitch, Some(57));
        let t = c.add_sample_track(id, SampleMode::Melodic).unwrap();
        assert!(matches!(
            c.project.tracks[t].instrument,
            InstrumentSpec::SampleMelodic { root: 57, .. }
        ));
        let json = c.save_json().unwrap();
        let (mut c2, _e2) = setup();
        c2.open_json(&json).unwrap();
        assert_eq!(c2.project, c.project);
        assert_eq!(c2.samples().len(), 1);
    }

    #[test]
    fn granular_from_mic_persists() {
        let (mut c, _e) = setup();
        let raw: Vec<f32> = (0..24000)
            .map(|i| (i as f32 * 2.0 * std::f32::consts::PI * 330.0 / 48000.0).sin() * 0.5)
            .collect();
        let (id, _) = c.add_recording(&raw, 48000).unwrap();
        let t = c.add_sample_track(id, SampleMode::Granular).unwrap();
        assert!(
            matches!(c.project.tracks[t].instrument, InstrumentSpec::Granular { sample: Some(s), .. } if s == id)
        );
        let json = c.save_json().unwrap();
        let (mut c2, _e2) = setup();
        c2.open_json(&json).unwrap();
        assert_eq!(c2.samples().len(), 1);
    }

    #[test]
    fn swap_keeps_notes() {
        let (mut c, _e) = setup();
        let before = c.project.tracks[0].slots.clone();
        c.swap_instrument(
            0,
            InstrumentSpec::Drums {
                kit: DrumKitId::Acoustic,
            },
        )
        .unwrap();
        assert_eq!(c.project.tracks[0].slots, before);
        assert!(c
            .swap_instrument(
                0,
                InstrumentSpec::SampleKit {
                    sample: 99,
                    slices: vec![]
                }
            )
            .is_err());
    }

    #[test]
    fn records_session_into_song() {
        let (mut c, mut e) = setup();
        c.clear_song();
        c.record_song = true;
        c.launch_scene(1).unwrap();
        c.play();
        // 84 bpm: a bar lasts 2.857 s; run ~2.5 bars
        for _ in 0..75 {
            run(&mut e, 0.1);
            c.poll();
        }
        c.stop();
        let lanes: Vec<(u32, u32, usize)> = c
            .project
            .song
            .clips
            .iter()
            .map(|k| (k.track, k.bars, k.slot))
            .collect();
        // every track recorded slot 1 for 3 bars (bars 0, 1, 2)
        assert_eq!(lanes.len(), 4, "{lanes:?}");
        assert!(lanes.iter().all(|l| l.1 == 3 && l.2 == 1), "{lanes:?}");
    }

    #[test]
    fn groove_edits_keep_the_recorded_take() {
        let (mut c, mut e) = setup();
        c.clear_song();
        c.groove = true;
        c.record_song = true;
        c.launch_scene(1).unwrap();
        c.play();
        // ~2.5 bars recorded (bars 0, 1, 2), then stop recording
        for _ in 0..75 {
            run(&mut e, 0.1);
            c.poll();
        }
        c.record_song = false;
        c.poll();
        let drums_before = c.project.pattern(0, 1).unwrap().clone();
        c.toggle_step(0, 1, 3, 42, 0.9).unwrap();
        // the tape still plays the loop as it was, from a take past the rows
        let id = c.project.tracks[0].id;
        let lane: Vec<_> = c
            .project
            .song
            .clips
            .iter()
            .filter(|k| k.track == id)
            .collect();
        assert_eq!(lane.len(), 1, "{lane:?}");
        let take = lane[0].slot;
        assert!(take >= c.project.scenes.len(), "{take}");
        assert_eq!(
            c.project.pattern(0, take).unwrap().notes,
            drums_before.notes
        );
        assert_ne!(c.project.pattern(0, 1).unwrap().notes, drums_before.notes);
        // a second edit with nothing new on tape makes no new take
        c.toggle_step(0, 1, 5, 42, 0.9).unwrap();
        let takes = c.project.tracks[0].slots[c.project.scenes.len()..]
            .iter()
            .filter(|p| p.is_some())
            .count();
        assert_eq!(takes, 1);
        // recording again appends after the tape
        c.record_song = true;
        for _ in 0..15 {
            run(&mut e, 0.1);
            c.poll();
        }
        c.stop();
        let lane: Vec<_> = c
            .project
            .song
            .clips
            .iter()
            .filter(|k| k.track == id)
            .map(|k| (k.start, k.slot))
            .collect();
        assert_eq!(lane, vec![(0, take), (3, 1)], "{lane:?}");
    }

    #[test]
    fn groove_edit_while_recording_splits_at_the_current_bar() {
        let (mut c, mut e) = setup();
        c.clear_song();
        c.groove = true;
        c.record_song = true;
        c.launch_scene(1).unwrap();
        c.play();
        for _ in 0..75 {
            run(&mut e, 0.1);
            c.poll();
        }
        // recording bar 2: bars 0-1 become the take, bar 2 follows the edit
        c.toggle_step(0, 1, 3, 42, 0.9).unwrap();
        let id = c.project.tracks[0].id;
        let lane: Vec<_> = c
            .project
            .song
            .clips
            .iter()
            .filter(|k| k.track == id)
            .map(|k| (k.start, k.bars, k.slot))
            .collect();
        assert_eq!(lane.len(), 2, "{lane:?}");
        assert_eq!((lane[0].0, lane[0].1), (0, 2));
        assert!(lane[0].2 >= c.project.scenes.len());
        assert_eq!(lane[1], (2, 1, 1));
    }

    #[test]
    fn groove_take_starts_at_the_first_bar_that_sounds() {
        let (mut c, mut e) = setup();
        c.clear_song();
        c.groove = true;
        c.record_song = true;
        c.play();
        // a silent bar and a half, then a section starts at the next bar
        for _ in 0..45 {
            run(&mut e, 0.1);
            c.poll();
        }
        c.launch_scene(1).unwrap();
        for _ in 0..30 {
            run(&mut e, 0.1);
            c.poll();
        }
        c.stop();
        assert!(!c.project.song.clips.is_empty());
        assert!(
            c.project.song.clips.iter().all(|k| k.start == 0),
            "{:?}",
            c.project.song.clips
        );
    }

    #[test]
    fn new_section_moves_takes_out_of_its_row() {
        let (mut c, _e) = setup();
        let row = c.project.scenes.len();
        let id = c.project.tracks[0].id;
        c.project.tracks[0].slots[row] = Some(Pattern::new("take", 1));
        c.project.song.insert(SongClip {
            track: id,
            slot: row,
            start: 0,
            bars: 2,
        });
        let r = c.add_section(None).unwrap();
        assert_eq!(r, row);
        assert!(c.project.tracks[0].slots[row].is_none());
        let moved = c
            .project
            .song
            .clips
            .iter()
            .find(|k| k.track == id && k.start == 0)
            .unwrap()
            .slot;
        assert!(moved > row);
        assert_eq!(
            c.project.tracks[0].slots[moved].as_ref().unwrap().name,
            "take"
        );
    }

    #[test]
    fn song_mode_follows_the_timeline() {
        let (mut c, mut e) = setup();
        c.set_song_mode(true);
        c.play();
        run(&mut e, 0.2);
        let st = c.status();
        assert!(st.song_mode);
        // bar 1: intro clips (Rhodes and pluck slot 0), drums and bass silent
        assert_eq!(st.tracks[0].active, None);
        assert_eq!(st.tracks[1].active, None);
        assert_eq!(st.tracks[2].active, Some(0));
        assert_eq!(st.tracks[3].active, Some(0));
        // jump to bar 5: verse
        c.seek(4);
        run(&mut e, 0.2);
        let st = c.status();
        assert_eq!(st.tracks[0].active, Some(1));
        assert_eq!(st.tracks[1].active, Some(1));
        // session launches are ignored in song mode
        c.launch(0, Some(0)).unwrap();
        run(&mut e, 0.05);
        assert_eq!(c.status().tracks[0].active, Some(1));
    }

    #[test]
    fn sections_grow_and_shrink_with_their_loops() {
        let (mut c, _e) = setup();
        let rows = c.project.scenes.len();
        let row = c.add_section(Some("Bridge 2".into())).unwrap();
        assert_eq!(row, rows);
        c.create_pattern(0, row, 1).unwrap();

        // a song clip on row 1 and one on the last row
        let id = c.project.tracks[0].id;
        c.project.song.clips.clear();
        c.add_clip(0, row, 0, Some(1)).unwrap();
        let had_row1 = c.project.tracks[0].slots[1].clone();
        if had_row1.is_some() {
            c.add_clip(0, 1, 4, Some(1)).unwrap();
        }

        c.delete_section(0).unwrap();
        assert_eq!(c.project.scenes.len(), rows);
        // everything moved up one row
        assert!(c.project.tracks[0].slots[row - 1].is_some());
        assert!(c.project.tracks[0].slots[row].is_none());
        assert_eq!(c.project.tracks[0].slots[0], had_row1);
        assert_eq!(c.project.tracks[0].slots.len(), SLOTS_PER_TRACK);
        for clip in c.project.song.clips.iter().filter(|cl| cl.track == id) {
            assert!(clip.slot < c.project.scenes.len());
            assert!(c.project.tracks[0].slots[clip.slot].is_some());
        }
        for (i, scene) in c.project.scenes.iter().enumerate() {
            assert_eq!(scene.slots.len(), c.project.tracks.len());
            assert!(scene.slots.iter().all(|s| s.is_none_or(|s| s == i)));
        }

        // never below one section, never above the slot capacity
        while c.project.scenes.len() > 1 {
            c.delete_section(0).unwrap();
        }
        assert!(c.delete_section(0).is_err());
        while c.project.scenes.len() < SLOTS_PER_TRACK {
            c.add_section(None).unwrap();
        }
        assert!(c.add_section(None).is_err());
    }

    #[test]
    fn hidden_rows_take_no_loops() {
        let (mut c, _e) = setup();
        while c.project.scenes.len() > 2 {
            let last = c.project.scenes.len() - 1;
            c.delete_section(last).unwrap();
        }
        assert!(c.create_pattern(0, 2, 1).is_err());
        assert!(c.create_pattern(0, 1, 1).is_ok());
    }

    #[test]
    fn undo_redo_roundtrip() {
        let (mut c, mut e) = setup();
        let original = c.project.clone();
        c.checkpoint();
        c.toggle_step(0, 1, 3, DRUM_BASE_NOTE + 2, 1.0).unwrap();
        c.checkpoint();
        c.add_track(
            TrackKind::Melodic,
            InstrumentSpec::Synth {
                preset: SynthPreset::Pad,
            },
        )
        .unwrap();
        let edited = c.project.clone();
        c.undo().unwrap();
        c.undo().unwrap();
        assert_eq!(c.project, original);
        run(&mut e, 0.05);
        c.redo().unwrap();
        c.redo().unwrap();
        assert_eq!(c.project, edited);
        assert!(c.redo().is_err());
        run(&mut e, 0.05);
        // engine has the 4 tracks back: playing them must not panic
        c.launch_scene(1).unwrap();
        c.play();
        run(&mut e, 0.3);
        assert_eq!(c.status().tracks.len(), 5);
    }

    #[test]
    fn master_lofi_is_bounded() {
        let (mut c, mut e) = setup();
        c.set_master(Master {
            volume: 1.0,
            vinyl: 1.0,
            tape: 1.0,
            warmth: 1.0,
        });
        c.launch_scene(1).unwrap();
        c.play();
        let mut buf = vec![0.0; 2 * 480];
        for _ in 0..200 {
            e.process(&mut buf, 2);
            assert!(buf.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
        }
    }

    #[test]
    fn piano_roll_moves_notes_and_sets_velocity() {
        let (_e, ep) = Engine::new(44100.0);
        let mut c = Controller::new(ep, 44100.0);
        let t = c
            .add_track(
                TrackKind::Melodic,
                InstrumentSpec::Synth {
                    preset: SynthPreset::Keys,
                },
            )
            .unwrap();
        c.create_pattern(t, 0, 1).unwrap();
        let n = |start, pitch, velocity| Note {
            start,
            length: 48,
            pitch,
            velocity,
        };
        c.add_note(t, 0, n(0, 60, 0.8)).unwrap();
        c.add_note(t, 0, n(0, 64, 0.8)).unwrap();
        // drag the E one step later and a tone up, twice as long
        assert!(c
            .move_note(
                t,
                0,
                0,
                64,
                Note {
                    start: 24,
                    length: 96,
                    pitch: 66,
                    velocity: 0.8
                }
            )
            .unwrap());
        assert!(
            !c.move_note(t, 0, 0, 64, n(0, 64, 0.8)).unwrap(),
            "already moved"
        );
        // the velocity lane edits a whole step
        c.set_velocity(t, 0, 0, 24, 0.3).unwrap();
        let notes = &c.project.pattern(t, 0).unwrap().notes;
        assert_eq!(notes.len(), 2);
        assert_eq!((notes[0].pitch, notes[0].velocity), (60, 0.3));
        assert_eq!(
            (
                notes[1].start,
                notes[1].pitch,
                notes[1].length,
                notes[1].velocity
            ),
            (24, 66, 96, 0.8)
        );
    }
}
