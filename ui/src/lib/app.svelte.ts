// What the screen knows: the project as the engine last answered it, the
// transport, and what the person has selected.

import type { Action, Info, PadInfo, Pattern, Project, Status } from "./api";
import { TICKS_PER_STEP, barTicks } from "./api";
import * as engine from "./engine";
import * as library from "./library";

export type Face = "loops" | "song" | "mix" | "set";
export type Pane = "notes" | "sound";
const RECOVERY = "mjolnir.recovery";
const CURRENT = "mjolnir.set";

export const app = $state({
  ready: false,
  /** Why the app could not start, when it could not. */
  failed: "",
  project: null as Project | null,
  status: null as Status | null,
  info: null as Info | null,
  face: "loops" as Face,
  /** Inside a track: the loops face shows its notes or its sound instead of the grid. */
  open: false,
  pane: "notes" as Pane,
  track: 0,
  section: 0,
  pad: 0,
  /** The bar of the loop the step keys show. */
  page: 0,
  pads: [] as PadInfo[],
  /** The last thing that went wrong, for the status line. */
  note: "",
  /** How a melodic track is played on screen. */
  surface: "keys" as "keys" | "pads",
  /** Length of a note written in the roll, in sixteenths. */
  noteLen: 2,
  /** Sound is running (it needs a first touch). */
  awake: false,
  busy: "",
  /** The set on screen: its id in this browser's library. */
  set: "",
  sets: [] as library.SetInfo[],
  /** The note whose detail is on screen. */
  pick: null as { start: number; pitch: number } | null,
  /** The punch strip is out. */
  punch: false,
  help: false,
  /** Connected MIDI controllers. */
  midi: [] as string[],
});

/** The picture of the signal: not reactive, the screen reads it every frame. */
export const signal: { scope: Float32Array; level: number } = { scope: new Float32Array(512), level: 0 };

let revision = 0;
let unsaved = false;

function adopt(reply: unknown): void {
  const r = reply as { tracks?: unknown; project?: Project } | null;
  if (r && typeof r === "object") {
    if ("tracks" in r) app.project = r as unknown as Project;
    else if (r.project) app.project = r.project;
    else return;
    unsaved = true;
    const p = app.project!;
    if (app.track >= p.tracks.length) app.track = Math.max(0, p.tracks.length - 1);
    if (app.section >= p.scenes.length) app.section = Math.max(0, p.scenes.length - 1);
  }
}

/** Run an action; the project on screen follows the engine's answer. */
export async function act<T = unknown>(action: Action): Promise<T | undefined> {
  engine.wake();
  try {
    const reply = await engine.call<T>(action);
    adopt(reply);
    app.note = "";
    return reply;
  } catch (e) {
    app.note = String(e);
    return undefined;
  }
}

/** A gesture that needs no answer: a pad, a knob while it turns. */
export function live(action: Action): void {
  engine.wake();
  engine.call(action).catch((e) => (app.note = String(e)));
}

export async function start(): Promise<void> {
  try {
    await engine.boot();
    app.info = await engine.call<Info>({ type: "info" });
    // the set that was open last time; older versions kept a single copy elsewhere
    let project: Project | undefined;
    app.set = safe(() => localStorage.getItem(CURRENT)) ?? "";
    let saved = app.set ? await library.loadSet(app.set) : null;
    const old = safe(() => localStorage.getItem(RECOVERY));
    if (!saved && old) {
      saved = old;
      app.set = library.newId();
      await library.saveSet(app.set, old).catch(() => {});
      safe(() => localStorage.removeItem(RECOVERY));
    }
    if (saved) project = await engine.call<Project>({ type: "open_project", json: saved }).catch(() => undefined);
    if (!project) {
      app.set = library.newId();
      project = await engine.call<Project>({ type: "get_project" });
      unsaved = true;
    }
    safe(() => localStorage.setItem(CURRENT, app.set));
    app.project = project;
    // a set seen for the first time goes straight into the library
    if (unsaved) void keepNow();
    else void refreshSets();
    void listenMidi();
    await refreshPads();
    engine.onStatus((s, scope) => {
      app.status = s;
      app.awake = true;
      signal.scope = scope;
      signal.level = s.master_level;
      if (s.revision !== revision) {
        revision = s.revision;
        void engine.call<Project>({ type: "get_project" }).then(adopt);
      }
    });
    setInterval(keep, 4000);
    addEventListener("pagehide", keep);
    app.ready = true;
  } catch (e) {
    app.failed = String(e);
  }
}

function safe<T>(f: () => T): T | null {
  try {
    return f();
  } catch {
    return null;
  }
}

/** Keep the set in this browser's library, so a closed tab loses nothing. */
async function keep(): Promise<void> {
  if (!unsaved || app.status?.playing) return;
  unsaved = false;
  const r = await engine.call<{ json: string }>({ type: "snapshot" }).catch(() => null);
  if (!r) return;
  await library.saveSet(app.set, r.json).catch(() => (app.note = "this browser would not keep the set → save it to a file"));
  void refreshSets();
}

export async function refreshSets(): Promise<void> {
  app.sets = await library.listSets();
}

async function show(json: string | null, id: string): Promise<void> {
  // what is on screen first goes to the library
  unsaved = true;
  const wasPlaying = app.status?.playing;
  if (wasPlaying) await act({ type: "stop" });
  await keepNow();
  if (json) await act({ type: "open_project", json });
  else await act({ type: "new_project" });
  app.set = id;
  safe(() => localStorage.setItem(CURRENT, id));
  app.track = 0;
  app.section = 0;
  app.open = false;
  unsaved = true;
  await refreshPads();
  await keepNow();
}

async function keepNow(): Promise<void> {
  const r = await engine.call<{ json: string }>({ type: "snapshot" }).catch(() => null);
  if (r) await library.saveSet(app.set, r.json).catch(() => {});
  unsaved = false;
  await refreshSets();
}

/** Open another set from the library. */
export async function openSet(id: string): Promise<void> {
  if (id === app.set) return;
  const json = await library.loadSet(id);
  if (json) await show(json, id);
}

export const newSet = () => show(null, library.newId());

/** Open a project given as text (a file, a demo) as a new set. */
export const openText = (json: string) => show(json, library.newId());

export async function duplicateSet(id: string): Promise<void> {
  if (id === app.set) await keepNow();
  const json = await library.loadSet(id);
  if (!json) return;
  try {
    const file = JSON.parse(json);
    file.project.name = `${file.project.name} copy`;
    await library.saveSet(library.newId(), JSON.stringify(file));
  } catch {
    app.note = "that set could not be copied";
  }
  await refreshSets();
}

export async function removeSet(id: string): Promise<void> {
  await library.deleteSet(id);
  await refreshSets();
  if (id !== app.set) return;
  // the open set was removed: open the next one, or a fresh one
  const next = app.sets[0];
  const json = next ? await library.loadSet(next.id) : null;
  app.set = next?.id ?? library.newId();
  safe(() => localStorage.setItem(CURRENT, app.set));
  if (json) await act({ type: "open_project", json });
  else await act({ type: "new_project" });
  app.track = 0;
  app.section = 0;
  app.open = false;
  await refreshPads();
  await keepNow();
}

/** MIDI controllers play the selected track. */
async function listenMidi(): Promise<void> {
  const nav = navigator as Navigator & { requestMIDIAccess?: () => Promise<MIDIAccess> };
  if (!nav.requestMIDIAccess) return;
  try {
    const access = await nav.requestMIDIAccess();
    const wire = () => {
      const names: string[] = [];
      access.inputs.forEach((input) => {
        names.push(input.name ?? "controller");
        input.onmidimessage = (e) => {
          if (e.data && e.data.length >= 2) live({ type: "midi", bytes: Array.from(e.data) });
        };
      });
      app.midi = names;
    };
    wire();
    access.onstatechange = wire;
  } catch {
    /* MIDI is not allowed here */
  }
}

/** Hold or let go a punch effect. */
export function punch(effect: string, on: boolean): void {
  live({ type: "punch", effect, on });
}

/** Keep what was just played on the track as its loop for this section. */
export async function capture(): Promise<void> {
  const free = !pattern();
  const r = await act<{ slot: number }>({ type: "capture", track: app.track, slot: free ? app.section : undefined });
  if (r && typeof r.slot === "number") app.section = Math.min(r.slot, (app.project?.scenes.length ?? 1) - 1);
}

/** 16-bit mono WAV, as base64, from samples at `rate`. */
function wavBase64(samples: Float32Array, rate: number): string {
  const bytes = new Uint8Array(44 + samples.length * 2);
  const v = new DataView(bytes.buffer);
  const text = (at: number, s: string) => [...s].forEach((c, i) => v.setUint8(at + i, c.charCodeAt(0)));
  text(0, "RIFF");
  v.setUint32(4, 36 + samples.length * 2, true);
  text(8, "WAVEfmt ");
  v.setUint32(16, 16, true);
  v.setUint16(20, 1, true);
  v.setUint16(22, 1, true);
  v.setUint32(24, rate, true);
  v.setUint32(28, rate * 2, true);
  v.setUint16(32, 2, true);
  v.setUint16(34, 16, true);
  text(36, "data");
  v.setUint32(40, samples.length * 2, true);
  for (let i = 0; i < samples.length; i++) v.setInt16(44 + i * 2, Math.max(-1, Math.min(1, samples[i])) * 32767, true);
  let out = "";
  for (let i = 0; i < bytes.length; i += 0x8000) out += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(out);
}

/** Longest recording a sound keeps, and the rate it is kept at. */
const SAMPLE_SECONDS = 20;
const SAMPLE_RATE = 32000;

/** Turn decoded audio into a sound on a new track (or on `onTrack`). */
export async function addSample(
  audio: AudioBuffer,
  mode: "melodic" | "kit" | "granular",
  name: string,
  onTrack?: number,
): Promise<void> {
  // to mono, at the rate sounds are kept at
  const length = Math.min(audio.length, Math.round(SAMPLE_SECONDS * audio.sampleRate));
  const mono = new Float32Array(length);
  for (let c = 0; c < audio.numberOfChannels; c++) {
    const d = audio.getChannelData(c);
    for (let i = 0; i < length; i++) mono[i] += d[i] / audio.numberOfChannels;
  }
  const ratio = audio.sampleRate / SAMPLE_RATE;
  const out = new Float32Array(Math.floor(length / ratio));
  for (let i = 0; i < out.length; i++) {
    const x = i * ratio;
    const k = Math.floor(x);
    out[i] = mono[k] + ((mono[Math.min(length - 1, k + 1)] ?? 0) - mono[k]) * (x - k);
  }
  const before = app.project?.tracks.length ?? 0;
  await act({ type: "add_sample", wav: wavBase64(out, SAMPLE_RATE), mode, track: onTrack, name });
  if (onTrack === undefined && (app.project?.tracks.length ?? 0) > before) await openCell(before, app.section);
  else await refreshPads();
}

/** Ask for an audio file and make a sound of it. */
export async function sampleFromFile(mode: "melodic" | "kit" | "granular", onTrack?: number): Promise<void> {
  const file = await engine.pickFile("audio/*,.wav,.mp3,.m4a,.ogg,.flac,.aif,.aiff");
  if (!file) return;
  try {
    const audio = await engine.decode(await file.arrayBuffer());
    await addSample(audio, mode, file.name.replace(/\.[^.]+$/, "").slice(0, 24), onTrack);
  } catch {
    app.note = "that file could not be read as sound → try a WAV or an MP3";
  }
}

export async function refreshPads(): Promise<void> {
  const pads = await engine.call<PadInfo[]>({ type: "pads", track: app.track }).catch(() => []);
  app.pads = pads;
  if (app.pad >= pads.length) app.pad = 0;
}

export const track = () => app.project?.tracks[app.track] ?? null;
export const pattern = (t = app.track, s = app.section): Pattern | null =>
  app.project?.tracks[t]?.slots[s] ?? null;
export const padPitch = () => app.pads[app.pad]?.pitches[0] ?? 36;
export const playing = () => app.status?.playing ?? false;

export async function selectTrack(i: number): Promise<void> {
  app.track = i;
  app.pick = null;
  app.pad = 0;
  app.page = 0;
  live({ type: "select_track", track: i, slot: app.section });
  await refreshPads();
}

/** Step into a loop: its track, its section, ready to play and write. */
export async function openCell(t: number, s: number): Promise<void> {
  app.section = s;
  app.page = 0;
  if (app.track !== t) await selectTrack(t);
  app.face = "loops";
  app.pane = "notes";
  app.open = true;
}

/** A track's identity colour: stable by id, so it survives other tracks coming and going. */
export const trackColor = (t: { id: number } | null | undefined): string =>
  t ? `var(--id-${t.id % 8})` : "var(--fg)";

export async function selectSection(s: number): Promise<void> {
  app.section = s;
  app.pick = null;
  app.page = 0;
  if (playing() && !app.status?.song_mode) await act({ type: "launch_scene", scene: s });
}

/** Turn a step of the selected pad on or off; the loop is made when missing. */
export async function toggleStep(step: number): Promise<void> {
  const t = app.track;
  const s = app.section;
  if (!pattern(t, s)) await act({ type: "create_pattern", track: t, slot: s, bars: 1 });
  const pitch = padPitch();
  const start = step * TICKS_PER_STEP;
  await act({ type: "toggle_step", track: t, slot: s, step, pitch, velocity: 0.8 });
  // a step just switched on is the one whose detail shows
  const on = pattern(t, s)?.notes.some((n) => n.start === start && n.pitch === pitch);
  app.pick = on ? { start, pitch } : null;
  await hear(t, s);
}

/** While the transport runs, a loop being written is a loop being heard. */
async function hear(t: number, s: number): Promise<void> {
  if (playing() && app.status?.tracks[t]?.active !== s && !app.status?.song_mode)
    await act({ type: "launch", track: t, slot: s });
}

/** Write a note (or, with chords on, the chord built on it) into the loop. */
export async function writeNote(pitches: number[], step: number, steps: number): Promise<void> {
  const t = app.track;
  const s = app.section;
  if (!pattern(t, s)) await act({ type: "create_pattern", track: t, slot: s, bars: 1 });
  for (const pitch of pitches)
    await act({
      type: "add_note",
      track: t,
      slot: s,
      note: { start: step * TICKS_PER_STEP, length: steps * TICKS_PER_STEP, pitch, velocity: 0.8 },
    });
  await hear(t, s);
}

/** The loop's length in bars. Growing it repeats what is there, so it keeps playing. */
export async function setBars(bars: number): Promise<void> {
  const t = app.track;
  const s = app.section;
  const p = pattern(t, s);
  if (!p) {
    await act({ type: "create_pattern", track: t, slot: s, bars });
    return;
  }
  const old = p.bars;
  const notes = p.notes.map((n) => ({ ...n }));
  await act({ type: "set_bars", track: t, slot: s, bars });
  const bar = barTicks(app.project?.meter);
  for (let k = 1; k < bars / old; k++)
    for (const n of notes)
      await act({ type: "add_note", track: t, slot: s, note: { ...n, start: n.start + k * old * bar } });
  if (app.page >= bars) app.page = bars - 1;
}

/** Start or stop one loop from the grid; the transport starts with the first. */
export async function toggleCell(t: number, s: number): Promise<boolean> {
  app.section = s;
  if (app.track !== t) await selectTrack(t);
  const st = app.status?.tracks[t];
  const on = playing() && !app.status?.song_mode && (st?.active === s || st?.queued === s) && !st?.queued_stop;
  if (on) {
    await act({ type: "launch", track: t, slot: null });
    return false;
  }
  if (app.status?.song_mode) await act({ type: "set_song_mode", on: false });
  await act({ type: "launch", track: t, slot: s });
  if (!playing()) await act({ type: "play" });
  return true;
}

/** Play a whole row. */
export async function playRow(s: number): Promise<void> {
  app.section = s;
  app.page = 0;
  if (app.status?.song_mode) await act({ type: "set_song_mode", on: false });
  await act({ type: "launch_scene", scene: s });
  if (!playing()) await act({ type: "play" });
}

/** Steps of the selected pad that are on, in the bar on screen. */
export function stepsOn(): boolean[] {
  const p = pattern();
  const out = new Array<boolean>(16).fill(false);
  if (!p) return out;
  const pitch = padPitch();
  const base = app.page * 16;
  for (const n of p.notes) {
    if (n.pitch !== pitch) continue;
    const i = Math.round(n.start / TICKS_PER_STEP) - base;
    if (i >= 0 && i < 16) out[i] = true;
  }
  return out;
}

/** The step being played in the selected loop, or -1. */
export function playhead(): number {
  const st = app.status?.tracks[app.track];
  const p = pattern();
  if (!st || !p || !playing() || app.status?.song_mode || st.active !== app.section) return -1;
  return Math.floor(st.progress * p.bars * 16) % (p.bars * 16);
}

export async function togglePlay(): Promise<void> {
  if (playing()) {
    await act({ type: "stop" });
    return;
  }
  const song = app.face === "song" && !app.open && (app.project?.song.clips.length ?? 0) > 0;
  await act({ type: "set_song_mode", on: song });
  if (!song) await act({ type: "launch_scene", scene: app.section });
  await act({ type: "play" });
}

export async function toggleRecord(): Promise<void> {
  const r = app.project?.record;
  if (!r) return;
  const on = !r.notes;
  await act({ type: "set_record", notes: on, automation: on, quantize: true, song: false });
  if (on && !pattern()) await act({ type: "create_pattern", track: app.track, slot: app.section, bars: 1 });
  if (on && !playing()) await togglePlay();
}

/** Give the track another sound, in the octave where that sound sits best. */
export async function setInstrument(spec: import("./api").InstrumentSpec, octave?: number): Promise<void> {
  await act({ type: "swap", track: app.track, instrument: spec });
  if (octave !== undefined && track()?.kind === "melodic")
    await act({ type: "set_octave", track: app.track, octave });
  await refreshPads();
}

export async function saveFile(): Promise<void> {
  const r = await act<{ json: string }>({ type: "save_project" });
  if (!r) return;
  const name = fileName();
  await engine
    .deliver([{ name: `${name}.mjolnir`, data: r.json, type: "application/json" }], name)
    .catch((e) => (app.note = String(e)));
}

export async function openFile(): Promise<void> {
  const json = await engine.pickText(".mjolnir,.yggdrasil,.tinydaw,.json,application/json");
  if (json) await openText(json);
}

/** Open one of the sets that ship with the app. */
export async function openDemo(file: string): Promise<void> {
  try {
    const json = await (await fetch(new URL(`demos/${file}`, document.baseURI).href)).text();
    await openText(json);
  } catch {
    app.note = "the demo could not be loaded";
  }
}

const fileName = () => (app.project?.name || "beat").replace(/[^\w\- ]+/g, "").trim() || "beat";

/** Bounce the song to a WAV file (and the MIDI file when asked). */
export async function exportSong(midi = false): Promise<void> {
  if (!app.project?.song.clips.length) {
    app.note = "the song is empty → add sections to it first";
    return;
  }
  app.busy = "rendering";
  try {
    const r = await engine.call<{ json: string }>({ type: "snapshot" });
    const [wav, mid] = await engine.render(r.json, midi ? [0, -1] : [0]);
    const name = fileName();
    const files: engine.OutFile[] = [];
    if (wav) files.push({ name: `${name}.wav`, data: wav, type: "audio/wav" });
    if (mid) files.push({ name: `${name}.mid`, data: mid, type: "audio/midi" });
    await engine.deliver(files, name);
  } catch (e) {
    app.note = String(e);
  }
  app.busy = "";
}

export { engine };
