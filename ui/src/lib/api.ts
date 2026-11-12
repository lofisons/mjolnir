// The engine's actions and what it answers, typed. Mirrors `crates/app`.

export type TrackKind = "drums" | "melodic";
export type ScaleKind = string;
export type MacroParam = "cutoff" | "resonance" | "drive" | "saturation" | "reverb" | "delay" | "volume";
export type ChordKind = "triad" | "seventh" | "sus2" | "sus4" | "power";

export const MACROS: MacroParam[] = ["cutoff", "resonance", "drive", "saturation", "reverb", "delay", "volume"];
export const PPQ = 96;
export const TICKS_PER_STEP = PPQ / 4;
/** Time signature: `beats` per bar of `unit` notes (4 = quarter, 8 = eighth). */
export interface Meter {
  beats: number;
  unit: number;
}
export const DEFAULT_METER: Meter = { beats: 4, unit: 4 };
/** Ticks in one beat of the meter (a quarter in x/4, an eighth in x/8). */
export const beatTicks = (m: Meter = DEFAULT_METER) => (PPQ * 4) / m.unit;
export const barTicks = (m: Meter = DEFAULT_METER) => m.beats * beatTicks(m);
/** 1/16 steps in a bar, and in a beat. */
export const barSteps = (m: Meter = DEFAULT_METER) => barTicks(m) / TICKS_PER_STEP;
export const beatSteps = (m: Meter = DEFAULT_METER) => beatTicks(m) / TICKS_PER_STEP;
export const DRUM_BASE_NOTE = 36;

export interface Note {
  start: number;
  length: number;
  pitch: number;
  velocity: number;
}
export interface AutoPoint {
  tick: number;
  value: number;
}
export interface AutomationLane {
  param: MacroParam;
  points: AutoPoint[];
}
export interface Pattern {
  name: string;
  bars: number;
  notes: Note[];
  automation: AutomationLane[];
}
export type InstrumentSpec =
  | { type: "drums"; kit: string }
  | { type: "synth"; preset: string }
  | { type: "sample_melodic"; sample: number; root: number }
  | { type: "sample_kit"; sample: number; slices: [number, number][] }
  | { type: "granular"; texture: string; sample: number | null }
  | { type: "custom_synth"; name: string; patch: Record<string, number> }
  | { type: "custom_kit"; name: string; pads: Record<string, number>[]; names: string[] };

/** One knob of a designable sound (from the engine's parameter table). */
export interface ParamSpec {
  key: string;
  label: string;
  group: string;
  min: number;
  max: number;
  log: boolean;
  /** 0 = continuous, 1 = whole numbers (a switch when 0..1). */
  step: number;
  unit: string;
  choices: string[];
  help: string;
}

export interface Scale {
  root: number;
  kind: ScaleKind;
}
export interface Track {
  id: number;
  name: string;
  kind: TrackKind;
  instrument: InstrumentSpec;
  macros: number[];
  mute: boolean;
  scale: Scale | null;
  octave: number;
  chord_mode: ChordKind | null;
  slots: (Pattern | null)[];
  pan: number;
  solo: boolean;
  humanize: number;
  /** Level of each part (drum pad or synth layer); missing = 0.8 (as designed). */
  parts?: number[];
}
export interface Scene {
  name: string;
  slots: (number | null)[];
}
export interface SongClip {
  track: number; // track id
  slot: number;
  start: number; // bar
  bars: number;
}
export interface Master {
  volume: number;
  vinyl: number;
  tape: number;
  warmth: number;
}
export interface Project {
  session?: { path: string | null; dirty: boolean };
  name: string;
  bpm: number;
  swing: number;
  scale: Scale;
  tracks: Track[];
  scenes: Scene[];
  song: { clips: SongClip[]; looping: boolean; fade_in: number; fade_out: number };
  master: Master;
  record: { notes: boolean; automation: boolean; quantize: boolean; song: boolean };
  view: { song_mode: boolean; start_bar: number };
  /** Always sent by the engine; older test fixtures may leave it out (4/4). */
  meter?: Meter;
  history: { undo: boolean; redo: boolean };
}
export interface TrackStatus {
  active: number | null;
  queued: number | null;
  queued_stop: boolean;
  level: number;
  macros: number[];
  progress: number;
}
export interface Status {
  count_in: number;
  note_onset?: number;
  playing: boolean;
  tick: number;
  bar: number;
  beat: number;
  master_level: number;
  tracks: TrackStatus[];
  song_mode: boolean;
  metronome: boolean;
  revision: number;
}
export interface PadInfo {
  label: string;
  pitches: number[];
  root: boolean;
}
/** A part of an instrument mixed on its own: a drum pad or a synth layer. */
export interface PartInfo {
  index: number;
  label: string;
  level: number;
}
export const PART_UNITY = 0.8;
export interface Option_ {
  id: string;
  label: string;
  /** Where the sound browser files it. */
  category?: string;
  /** A synth inside, whose every part can be edited. */
  open?: boolean;
  /** The octave where the sound sits best. */
  octave?: number;
}
export interface Groove {
  id: string;
  label: string;
  /** Hats layer over the loop; drums replace it. */
  kind: "hats" | "drums";
}
export interface Info {
  catalog: {
    scales: Option_[];
    kits: Option_[];
    presets: Option_[];
    textures: Option_[];
    grooves: Groove[];
    macros: { id: MacroParam; label: string }[];
    meters?: Meter[];
    synth_params?: ParamSpec[];
    drum_params?: ParamSpec[];
  };
  sample_rate: number;
}
export type Action =
  | { type: "get_project" }
  | { type: "select_track"; track: number; slot?: number }
  | { type: "set_metronome"; on: boolean }
  | { type: "move_note"; track: number; slot: number; start: number; pitch: number; note: Note }
  | { type: "set_velocity"; track: number; slot: number; from: number; to: number; velocity: number }
  | { type: "info" }
  | { type: "new_project" }
  | { type: "open_demo" }
  | { type: "add_chord"; track: number; slot: number; degree: number; octave: number; kind: ChordKind; start: number; length: number }
  | { type: "play" }
  | { type: "stop" }
  | { type: "set_bpm"; bpm: number }
  | { type: "set_meter"; beats: number; unit: number }
  | { type: "design_sound"; track: number }
  | { type: "set_sound_param"; track: number; pad: number | null; key: string; value: number }
  | { type: "rename_sound"; track: number; pad: number | null; name: string }
  | { type: "set_swing"; swing: number }
  | { type: "set_name"; name: string }
  | { type: "set_scale"; root: number; kind: ScaleKind; track: number | null }
  | { type: "clear_track_scale"; track: number }
  | { type: "set_count_in"; bars: number }
  | { type: "set_record"; notes: boolean; automation: boolean; quantize: boolean; song: boolean }
  | { type: "undo" }
  | { type: "redo" }
  | { type: "checkpoint" }
  | { type: "set_solo"; track: number; solo: boolean }
  | { type: "set_pan"; track: number; pan: number }
  | { type: "set_humanize"; track: number; amount: number }
  | { type: "parts"; track: number }
  | { type: "set_part"; track: number; part: number; level: number }
  | { type: "set_master"; master: Master }
  | { type: "set_song_mode"; on: boolean }
  | { type: "set_groove_mode"; on: boolean }
  | { type: "seek"; bar: number }
  | { type: "add_clip"; track: number; slot: number; start: number; bars: number | null }
  | { type: "edit_clip"; index: number; track: number; start: number; bars: number }
  | { type: "delete_clip"; index: number }
  | { type: "append_scene"; scene: number }
  | { type: "clear_song" }
  | { type: "set_song_looping"; looping: boolean }
  | { type: "set_song_fades"; fade_in: number; fade_out: number }
  | { type: "add_track"; kind: TrackKind }
  | { type: "remove_track"; track: number }
  | { type: "rename_track"; track: number; name: string }
  | { type: "set_mute"; track: number; mute: boolean }
  | { type: "swap"; track: number; instrument: InstrumentSpec }
  | { type: "set_octave"; track: number; octave: number }
  | { type: "set_chord_mode"; track: number; mode: ChordKind | null }
  | { type: "pads"; track: number }
  | { type: "pad_on"; track: number; pad: number; velocity: number }
  | { type: "pad_off"; track: number; pad: number }
  | { type: "note_on"; track: number; pitch: number; velocity: number }
  | { type: "note_off"; track: number; pitch: number }
  | { type: "create_pattern"; track: number; slot: number; bars: number }
  | { type: "delete_pattern"; track: number; slot: number }
  | { type: "duplicate_pattern"; track: number; from: number; to: number }
  | { type: "toggle_step"; track: number; slot: number; step: number; pitch: number; velocity: number }
  | { type: "add_note"; track: number; slot: number; note: Note }
  | { type: "remove_note"; track: number; slot: number; start: number; pitch: number }
  | { type: "set_bars"; track: number; slot: number; bars: number }
  | { type: "quantize"; track: number; slot: number }
  | { type: "clear_pattern"; track: number; slot: number }
  | { type: "rename_pattern"; track: number; slot: number; name: string }
  | { type: "load_groove"; track: number; slot: number; groove: string }
  | { type: "launch"; track: number; slot: number | null }
  | { type: "launch_scene"; scene: number }
  | { type: "stop_all" }
  | { type: "rename_scene"; scene: number; name: string }
  | { type: "add_section"; name?: string }
  | { type: "delete_section"; scene: number }
  | { type: "capture"; track: number }
  | { type: "set_macro"; track: number; param: MacroParam; value: number }
  | { type: "touch"; track: number; param: MacroParam; touching: boolean }
  | { type: "set_automation"; track: number; slot: number; param: MacroParam; points: AutoPoint[] }
  | { type: "clear_automation"; track: number; slot: number; param: MacroParam | null }
  | { type: "sound_file"; track: number }
  | { type: "load_sound_file"; track: number; json: string }
  | { type: "save_project" }
  | { type: "snapshot" }
  | { type: "open_project"; json: string }
  | { type: "midi"; bytes: number[] };

export const NOTE_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
export const noteName = (p: number) => `${NOTE_NAMES[p % 12]}${Math.floor(p / 12) - 1}`;

const SCALES: Record<string, number[]> = {
  major: [0, 2, 4, 5, 7, 9, 11],
  minor: [0, 2, 3, 5, 7, 8, 10],
  dorian: [0, 2, 3, 5, 7, 9, 10],
  phrygian: [0, 1, 3, 5, 7, 8, 10],
  lydian: [0, 2, 4, 6, 7, 9, 11],
  mixolydian: [0, 2, 4, 5, 7, 9, 10],
  locrian: [0, 1, 3, 5, 6, 8, 10],
  harmonic_minor: [0, 2, 3, 5, 7, 8, 11],
  melodic_minor: [0, 2, 3, 5, 7, 9, 11],
  major_pentatonic: [0, 2, 4, 7, 9],
  minor_pentatonic: [0, 3, 5, 7, 10],
  blues: [0, 3, 5, 6, 7, 10],
  chromatic: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
};

/** In-scale MIDI pitches from `low` to `high` inclusive (scale lock for the editor). */
export function scalePitches(scale: Scale, low: number, high: number): number[] {
  const iv = SCALES[scale.kind] ?? SCALES.chromatic;
  const out: number[] = [];
  for (let p = low; p <= high; p++) if (iv.includes((((p - scale.root) % 12) + 12) % 12)) out.push(p);
  return out;
}

/** The chord of the key built on `pitch`: every other note of the scale above it. */
export function chordOn(scale: Scale, pitch: number, kind: ChordKind): number[] {
  const notes = scalePitches(scale, 0, 127);
  const i = notes.indexOf(pitch);
  if (i < 0) return [pitch];
  const steps = { triad: [0, 2, 4], seventh: [0, 2, 4, 6], sus2: [0, 1, 4], sus4: [0, 3, 4], power: [0, 4] }[kind];
  return steps.map((d) => notes[i + d]).filter((p) => p !== undefined);
}

/** Hold a pointer on the element it went down on, where the browser allows it. */
export function capture(e: PointerEvent): void {
  try {
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
  } catch {
    /* a pointer that is already gone */
  }
}
