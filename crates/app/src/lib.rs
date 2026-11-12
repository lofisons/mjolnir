//! The app, without a window: every action a person can take, as JSON in and
//! JSON out. The browser build, the desktop shell and the plugin all drive the
//! engine through this one door.

use mjolnir_engine::controller::Controller;
use mjolnir_engine::engine::EngineEndpoints;
use mjolnir_engine::grooves::GrooveId;
use mjolnir_engine::model::{
    AutoPoint, DrumKitId, InstrumentSpec, MacroParam, Master, Note, SynthPreset, TextureKind,
    TrackKind,
};
use mjolnir_engine::session::Session;
use mjolnir_engine::theory::{ChordKind, Scale, ScaleKind};
use serde::Deserialize;
use serde_json::{json, Value};

/// The controller and what the host needs to know about unsaved work.
pub struct App {
    pub ctrl: Controller,
    session: Session,
}

impl App {
    pub fn new(endpoints: EngineEndpoints, sample_rate: f32) -> App {
        let mut ctrl = Controller::new(endpoints, sample_rate);
        ctrl.new_project();
        let session = Session::new(ctrl.project.clone(), std::path::PathBuf::new());
        App { ctrl, session }
    }

    /// Drain engine events and return the transport and meter status.
    pub fn status(&mut self) -> Value {
        self.ctrl.poll();
        serde_json::to_value(self.ctrl.status()).unwrap_or(Value::Null)
    }
}

/// Every UI action. Mirrors `ui/src/lib/api.ts`. Files, devices and
/// dialogs belong to the host: here a project or a sound travels as JSON text.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    GetProject,
    /// The track the MIDI keyboard plays (the selected one).
    SelectTrack {
        track: usize,
        /// The section on screen: a take into an empty loop goes there.
        #[serde(default)]
        slot: Option<usize>,
    },
    SetMetronome {
        on: bool,
    },
    Info,
    NewProject,
    OpenDemo,
    AddChord {
        track: usize,
        slot: usize,
        degree: i32,
        octave: i32,
        kind: ChordKind,
        start: u32,
        length: u32,
    },
    Play,
    Stop,
    /// Time signature, e.g. 3/4 or 7/8.
    SetMeter {
        beats: u32,
        unit: u32,
    },
    SetBpm {
        bpm: f32,
    },
    SetSwing {
        swing: f32,
    },
    SetName {
        name: String,
    },
    SetScale {
        root: u8,
        kind: ScaleKind,
        track: Option<usize>,
    },
    ClearTrackScale {
        track: usize,
    },
    SetCountIn {
        bars: u32,
    },
    SetRecord {
        notes: bool,
        automation: bool,
        quantize: bool,
        #[serde(default)]
        song: bool,
    },
    Undo,
    Redo,
    /// Explicit undo point (e.g. when a knob is grabbed).
    Checkpoint,

    SetSolo {
        track: usize,
        solo: bool,
    },
    SetPan {
        track: usize,
        pan: f32,
    },
    SetHumanize {
        track: usize,
        amount: f32,
    },
    /// The parts of a track's instrument (drum pads, synth layers) and their levels.
    Parts {
        track: usize,
    },
    /// Level of one part, 0..1. Live like a knob: the UI sends `checkpoint`.
    SetPart {
        track: usize,
        part: usize,
        level: f32,
    },
    SetMaster {
        master: Master,
    },

    SetSongMode {
        on: bool,
    },
    /// Groove view: recording appends to the timeline and edits keep takes.
    SetGrooveMode {
        on: bool,
    },
    Seek {
        bar: u32,
    },
    AddClip {
        track: usize,
        slot: usize,
        start: u32,
        bars: Option<u32>,
    },
    EditClip {
        index: usize,
        track: usize,
        start: u32,
        bars: u32,
    },
    DeleteClip {
        index: usize,
    },
    AppendScene {
        scene: usize,
    },
    ClearSong,
    SetSongLooping {
        looping: bool,
    },
    SetSongFades {
        fade_in: u32,
        fade_out: u32,
    },

    AddTrack {
        kind: TrackKind,
    },
    RemoveTrack {
        track: usize,
    },
    RenameTrack {
        track: usize,
        name: String,
    },
    SetMute {
        track: usize,
        mute: bool,
    },
    Swap {
        track: usize,
        instrument: InstrumentSpec,
    },
    /// Make the track's sound editable (a designed copy of it).
    DesignSound {
        track: usize,
    },
    /// Turn one knob of a designed sound (`pad` for a kit).
    SetSoundParam {
        track: usize,
        #[serde(default)]
        pad: Option<usize>,
        key: String,
        value: f32,
    },
    RenameSound {
        track: usize,
        #[serde(default)]
        pad: Option<usize>,
        name: String,
    },
    /// The track's sound as the text of a sound file.
    SoundFile {
        track: usize,
    },
    /// Load the text of a sound file into the track.
    LoadSoundFile {
        track: usize,
        json: String,
    },
    SetOctave {
        track: usize,
        octave: i32,
    },
    SetChordMode {
        track: usize,
        mode: Option<ChordKind>,
    },

    Pads {
        track: usize,
    },
    PadOn {
        track: usize,
        pad: usize,
        velocity: f32,
    },
    PadOff {
        track: usize,
        pad: usize,
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

    CreatePattern {
        track: usize,
        slot: usize,
        bars: u32,
    },
    DeletePattern {
        track: usize,
        slot: usize,
    },
    DuplicatePattern {
        track: usize,
        from: usize,
        to: usize,
    },
    ToggleStep {
        track: usize,
        slot: usize,
        step: u32,
        pitch: u8,
        velocity: f32,
    },
    AddNote {
        track: usize,
        slot: usize,
        note: Note,
    },
    MoveNote {
        track: usize,
        slot: usize,
        start: u32,
        pitch: u8,
        note: Note,
    },
    /// Not undoable by itself: the UI sends `checkpoint` when a drag starts.
    SetVelocity {
        track: usize,
        slot: usize,
        from: u32,
        to: u32,
        velocity: f32,
    },
    RemoveNote {
        track: usize,
        slot: usize,
        start: u32,
        pitch: u8,
    },
    SetBars {
        track: usize,
        slot: usize,
        bars: u32,
    },
    Quantize {
        track: usize,
        slot: usize,
    },
    /// Every note of the loop, rewritten together (transpose, harmonise).
    SetNotes {
        track: usize,
        slot: usize,
        notes: Vec<Note>,
    },
    ClearPattern {
        track: usize,
        slot: usize,
    },
    RenamePattern {
        track: usize,
        slot: usize,
        name: String,
    },
    /// A ready-made drum loop into a slot (hats layer, beats replace).
    LoadGroove {
        track: usize,
        slot: usize,
        groove: GrooveId,
    },

    Launch {
        track: usize,
        slot: Option<usize>,
    },
    LaunchScene {
        scene: usize,
    },
    StopAll,
    RenameScene {
        scene: usize,
        name: String,
    },
    /// A new section (grid row) at the bottom.
    AddSection {
        #[serde(default)]
        name: Option<String>,
    },
    /// Delete a section and its loops; the rows below move up.
    DeleteSection {
        scene: usize,
    },

    /// Keep what was just played on the track as a loop (in `slot`, or the
    /// first free one) even though nothing was recording.
    Capture {
        track: usize,
        #[serde(default)]
        slot: Option<usize>,
    },
    /// A recording or an audio file (a WAV, as base64) becomes a sound: on a
    /// new track, or on `track` when given. `mode` is how it is played:
    /// across the keys, sliced into pads, or as grains.
    AddSample {
        wav: String,
        mode: mjolnir_engine::controller::SampleMode,
        #[serde(default)]
        track: Option<usize>,
        #[serde(default)]
        name: Option<String>,
    },

    SetMacro {
        track: usize,
        param: MacroParam,
        value: f32,
    },
    Touch {
        track: usize,
        param: MacroParam,
        touching: bool,
    },
    SetAutomation {
        track: usize,
        slot: usize,
        param: MacroParam,
        points: Vec<AutoPoint>,
    },
    ClearAutomation {
        track: usize,
        slot: usize,
        param: Option<MacroParam>,
    },

    /// The project as the text of a project file; the project is then clean.
    SaveProject,
    /// The same text without marking the project clean: a recovery copy.
    Snapshot,
    OpenProject {
        json: String,
    },
    /// Hold (`on`) or let go a punch effect over the whole mix.
    Punch {
        effect: String,
        on: bool,
    },
    /// A raw MIDI message from a controller.
    Midi {
        bytes: Vec<u8>,
    },
}

impl Action {
    /// Actions that change the project in a way worth undoing.
    fn undoable(&self) -> bool {
        !matches!(
            self,
            Action::GetProject
                | Action::SelectTrack { .. }
                | Action::SetMetronome { .. }
                | Action::SetVelocity { .. }
                | Action::Info
                | Action::NewProject
                | Action::OpenDemo
                | Action::Play
                | Action::Stop
                | Action::SetRecord { .. }
                | Action::SetCountIn { .. }
                | Action::Undo
                | Action::Redo
                | Action::Checkpoint
                | Action::Pads { .. }
                | Action::PadOn { .. }
                | Action::PadOff { .. }
                | Action::NoteOn { .. }
                | Action::NoteOff { .. }
                | Action::Launch { .. }
                | Action::LaunchScene { .. }
                | Action::StopAll
                | Action::SetMacro { .. }
                | Action::Touch { .. }
                | Action::SetPan { .. }
                | Action::SetHumanize { .. }
                | Action::Parts { .. }
                | Action::SetPart { .. }
                | Action::SetMaster { .. }
                | Action::SetSoundParam { .. }
                | Action::SoundFile { .. }
                | Action::SetSongMode { .. }
                | Action::SetGrooveMode { .. }
                | Action::Seek { .. }
                | Action::SaveProject
                | Action::Snapshot
                | Action::OpenProject { .. }
                | Action::Midi { .. }
                | Action::Punch { .. }
        )
    }
}

fn project(c: &Controller, session: &Session) -> Result<Value, String> {
    let mut v = serde_json::to_value(&c.project).map_err(|e| e.to_string())?;
    v["session"] = json!({ "path": session.path, "dirty": session.dirty(&c.project) });
    v["record"] = json!({
        "notes": c.record_notes,
        "automation": c.record_automation,
        "quantize": c.quantize,
        "song": c.record_song,
    });
    v["view"] = json!({ "song_mode": c.song_mode(), "start_bar": c.start_bar() });
    // always present for the UI (the file leaves out a plain 4/4)
    v["meter"] = json!(c.project.meter);
    v["history"] = json!({ "undo": c.can_undo(), "redo": c.can_redo() });
    Ok(v)
}

fn catalog() -> Value {
    json!({
        "scales": ScaleKind::ALL.iter().map(|k| json!({ "id": k, "label": k.label() })).collect::<Vec<_>>(),
        "kits": DrumKitId::ALL.iter().map(|k| json!({ "id": k, "label": k.label(), "category": k.category() })).collect::<Vec<_>>(),
        "presets": SynthPreset::ALL.iter().map(|k| json!({ "id": k, "label": k.label(), "category": k.category(), "open": k.zoned().is_none() && k.sampled().is_none(), "octave": k.home_octave() })).collect::<Vec<_>>(),
        "textures": TextureKind::ALL.iter().map(|k| json!({ "id": k, "label": k.label(), "category": k.category(), "octave": 3 })).collect::<Vec<_>>(),
        "grooves": GrooveId::ALL.iter().map(|g| json!({ "id": g, "label": g.label(), "kind": g.kind() })).collect::<Vec<_>>(),
        "macros": MacroParam::ALL.iter().map(|k| json!({ "id": k, "label": k.label() })).collect::<Vec<_>>(),
        "meters": mjolnir_engine::model::Meter::COMMON,
        "macro_sounds": mjolnir_engine::macro_synth::PRESETS.iter().map(|p| json!({
            "id": p.id, "label": p.label, "category": p.category, "octave": p.octave, "patch": p.patch,
        })).collect::<Vec<_>>(),
        "macro_params": mjolnir_engine::macro_synth::MACRO_PARAMS,
        "punch": mjolnir_engine::space::PunchKind::ALL.iter().map(|k| k.id()).collect::<Vec<_>>(),
        "synth_params": mjolnir_engine::sound::SYNTH_PARAMS,
        "drum_params": mjolnir_engine::sound::DRUM_PARAMS,
    })
}

impl App {
    /// Execute one UI action and answer with JSON: the project after a change,
    /// `null` after a live gesture.
    pub fn handle(&mut self, action: Action) -> Result<Value, String> {
        let App { ctrl: c, session } = self;
        if action.undoable() {
            c.checkpoint();
        }
        match action {
            // ---- high-frequency actions: no project in the response
            Action::PadOn {
                track,
                pad,
                velocity,
            } => return c.pad_on(track, pad, velocity).map(|_| Value::Null),
            Action::PadOff { track, pad } => return c.pad_off(track, pad).map(|_| Value::Null),
            Action::NoteOn {
                track,
                pitch,
                velocity,
            } => return c.note_on(track, pitch, velocity).map(|_| Value::Null),
            Action::NoteOff { track, pitch } => {
                return c.note_off(track, pitch).map(|_| Value::Null)
            }
            Action::SetMetronome { on } => {
                c.set_metronome(on);
                return Ok(Value::Null);
            }
            Action::SelectTrack { track, slot } => {
                c.set_focus(track);
                c.focus_slot = slot;
                return Ok(Value::Null);
            }
            Action::Touch {
                track,
                param,
                touching,
            } => {
                if touching {
                    c.checkpoint();
                }
                c.touch(track, param, touching)?;
                return if touching {
                    Ok(Value::Null)
                } else {
                    project(c, session)
                };
            }
            Action::SetMacro {
                track,
                param,
                value,
            } => {
                c.set_macro(track, param, value)?;
                return Ok(Value::Null);
            }
            Action::Pads { track } => {
                return serde_json::to_value(c.pad_layout(track)?).map_err(|e| e.to_string())
            }
            Action::Info => {
                return Ok(json!({ "catalog": catalog(), "sample_rate": c.sample_rate() }))
            }
            Action::Punch { effect, on } => {
                let kind =
                    mjolnir_engine::space::PunchKind::from_id(&effect).ok_or("no such effect")?;
                c.punch(kind, on);
                return Ok(Value::Null);
            }
            Action::Midi { bytes } => {
                c.midi_message(&bytes);
                return Ok(Value::Null);
            }
            Action::SoundFile { track } => return Ok(json!({ "json": c.sound_file(track)? })),
            Action::Snapshot => {
                return Ok(json!({ "json": c.save_json()?, "dirty": session.dirty(&c.project) }))
            }
            Action::SaveProject => {
                let json = c.save_json()?;
                session.clean(&c.project, None);
                return Ok(json!({ "json": json, "project": project(c, session)? }));
            }
            Action::Capture { track, slot } => {
                let slot = c.capture(track, slot)?;
                return Ok(json!({ "project": project(c, session)?, "slot": slot }));
            }
            Action::SetPan { track, pan } => return c.set_pan(track, pan).map(|_| Value::Null),
            Action::SetHumanize { track, amount } => {
                return c.set_humanize(track, amount).map(|_| Value::Null)
            }
            Action::Parts { track } => {
                return serde_json::to_value(c.parts(track)?).map_err(|e| e.to_string())
            }
            Action::SetPart { track, part, level } => {
                return c.set_part(track, part, level).map(|_| Value::Null)
            }
            Action::SetMaster { master } => {
                c.set_master(master);
                return Ok(Value::Null);
            }

            // ---- everything else mutates the project and returns it
            Action::GetProject => {}
            Action::NewProject | Action::OpenDemo => {
                if matches!(action, Action::NewProject) {
                    c.new_project();
                } else {
                    c.open_demo();
                }
                session.clean(&c.project, None);
            }
            Action::AddChord {
                track,
                slot,
                degree,
                octave,
                kind,
                start,
                length,
            } => c.add_chord(track, slot, degree, octave, kind, start, length)?,
            Action::Play => {
                // anything recorded during this take can be undone in one step
                if c.record_notes || c.record_automation || c.record_song {
                    c.checkpoint();
                }
                c.play()
            }
            Action::Stop => c.stop(),
            Action::SetMeter { beats, unit } => {
                c.set_meter(mjolnir_engine::model::Meter { beats, unit });
            }
            Action::SetBpm { bpm } => c.set_bpm(bpm),
            Action::SetSwing { swing } => c.set_swing(swing),
            Action::SetName { name } => c.project.name = name,
            Action::SetScale { root, kind, track } => c.set_scale(Scale::new(root, kind), track)?,
            Action::ClearTrackScale { track } => c.clear_track_scale(track)?,
            Action::SetCountIn { bars } => c.count_in_bars = bars.min(2),
            Action::SetRecord {
                notes,
                automation,
                quantize,
                song,
            } => {
                c.record_notes = notes;
                c.record_automation = automation;
                c.quantize = quantize;
                c.record_song = song;
            }
            Action::Undo => c.undo()?,
            Action::Redo => c.redo()?,
            Action::Checkpoint => c.checkpoint(),
            Action::SetSolo { track, solo } => c.set_solo(track, solo)?,
            Action::SetSongMode { on } => c.set_song_mode(on),
            Action::SetGrooveMode { on } => c.groove = on,
            Action::Seek { bar } => c.seek(bar),
            Action::AddClip {
                track,
                slot,
                start,
                bars,
            } => {
                c.add_clip(track, slot, start, bars)?;
            }
            Action::EditClip {
                index,
                track,
                start,
                bars,
            } => {
                c.edit_clip(index, track, start, bars)?;
            }
            Action::DeleteClip { index } => c.delete_clip(index)?,
            Action::AppendScene { scene } => c.append_scene(scene, None)?,
            Action::ClearSong => c.clear_song(),
            Action::SetSongLooping { looping } => c.set_song_looping(looping),
            Action::SetSongFades { fade_in, fade_out } => c.set_song_fades(fade_in, fade_out),
            Action::AddTrack { kind } => {
                let spec = match kind {
                    TrackKind::Drums => InstrumentSpec::Drums {
                        kit: DrumKitId::Tr808,
                    },
                    TrackKind::Melodic => InstrumentSpec::Synth {
                        preset: SynthPreset::Pluck,
                    },
                };
                c.add_track(kind, spec)?;
            }
            Action::RemoveTrack { track } => c.remove_track(track)?,
            Action::RenameTrack { track, name } => c.rename_track(track, name)?,
            Action::SetMute { track, mute } => c.set_mute(track, mute)?,
            Action::Swap { track, instrument } => c.swap_instrument(track, instrument)?,
            Action::DesignSound { track } => c.design_sound(track)?,
            Action::SetSoundParam {
                track,
                pad,
                key,
                value,
            } => {
                // a knob drag: no project in the reply (the UI keeps its own copy)
                return c
                    .set_sound_param(track, pad, &key, value)
                    .map(|_| Value::Null);
            }
            Action::RenameSound { track, pad, name } => c.rename_sound(track, pad, name)?,
            Action::LoadSoundFile { track, json } => c.load_sound_file(track, &json)?,
            Action::AddSample {
                wav,
                mode,
                track,
                name,
            } => {
                use base64::Engine as _;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(wav.as_bytes())
                    .map_err(|_| "the audio could not be read")?;
                let (sample, _) = c.import_wav(&bytes)?;
                let index = match track {
                    Some(t) => {
                        let spec = c.sample_spec(sample, mode)?;
                        c.swap_instrument(t, spec)?;
                        t
                    }
                    None => c.add_sample_track(sample, mode)?,
                };
                if let Some(name) = name.filter(|n| !n.trim().is_empty()) {
                    c.rename_track(index, name)?;
                }
            }
            Action::SetOctave { track, octave } => c.set_octave(track, octave)?,
            Action::SetChordMode { track, mode } => c.set_chord_mode(track, mode)?,
            Action::CreatePattern { track, slot, bars } => c.create_pattern(track, slot, bars)?,
            Action::DeletePattern { track, slot } => c.delete_pattern(track, slot)?,
            Action::DuplicatePattern { track, from, to } => c.duplicate_pattern(track, from, to)?,
            Action::ToggleStep {
                track,
                slot,
                step,
                pitch,
                velocity,
            } => {
                c.toggle_step(track, slot, step, pitch, velocity)?;
            }
            Action::AddNote { track, slot, note } => c.add_note(track, slot, note)?,
            Action::MoveNote {
                track,
                slot,
                start,
                pitch,
                note,
            } => {
                c.move_note(track, slot, start, pitch, note)?;
            }
            Action::SetVelocity {
                track,
                slot,
                from,
                to,
                velocity,
            } => {
                c.set_velocity(track, slot, from, to, velocity)?;
            }
            Action::RemoveNote {
                track,
                slot,
                start,
                pitch,
            } => {
                c.remove_note(track, slot, start, pitch)?;
            }
            Action::SetBars { track, slot, bars } => c.set_pattern_bars(track, slot, bars)?,
            Action::Quantize { track, slot } => c.quantize_pattern(track, slot)?,
            Action::SetNotes { track, slot, notes } => c.set_pattern_notes(track, slot, notes)?,
            Action::ClearPattern { track, slot } => c.clear_pattern(track, slot)?,
            Action::RenamePattern { track, slot, name } => c.rename_pattern(track, slot, name)?,
            Action::LoadGroove {
                track,
                slot,
                groove,
            } => c.load_groove(track, slot, groove)?,
            Action::Launch { track, slot } => c.launch(track, slot)?,
            Action::LaunchScene { scene } => c.launch_scene(scene)?,
            Action::StopAll => c.stop_all_clips(),
            Action::RenameScene { scene, name } => c.rename_scene(scene, name)?,
            Action::AddSection { name } => {
                c.add_section(name)?;
            }
            Action::DeleteSection { scene } => c.delete_section(scene)?,
            Action::SetAutomation {
                track,
                slot,
                param,
                points,
            } => c.set_automation(track, slot, param, points)?,
            Action::ClearAutomation { track, slot, param } => {
                c.clear_automation(track, slot, param)?
            }
            Action::OpenProject { json } => {
                c.open_json(&json)?;
                session.clean(&c.project, None);
            }
        }
        project(c, session)
    }
}
