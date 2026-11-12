#!/usr/bin/env node
// Write the three sets that ship with Mjolnir, by driving the engine the way
// the interface does. Run after `pnpm wasm`:  node tools/build_demos.mjs
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");
const wasm = fs.readFileSync(path.join(root, "ui/public/mjolnir.wasm"));
const out = path.join(root, "ui/public/demos");
const enc = new TextEncoder();
const dec = new TextDecoder();
const STEP = 24;

function session() {
  const { exports: x } = new WebAssembly.Instance(new WebAssembly.Module(wasm), {});
  x.mj_init(48000);
  const call = (a) => {
    const b = enc.encode(JSON.stringify(a));
    const p = x.mj_alloc(b.length);
    new Uint8Array(x.memory.buffer, p, b.length).set(b);
    const n = x.mj_dispatch(p, b.length);
    x.mj_free(p, b.length);
    const r = JSON.parse(dec.decode(new Uint8Array(x.memory.buffer, x.mj_result_ptr(), n)));
    if (r.err) throw new Error(`${JSON.stringify(a)} → ${r.err}`);
    return r.ok;
  };
  const catalog = call({ type: "info" }).catalog;
  const macro = (id) => {
    const m = catalog.macro_sounds.find((s) => s.id === id);
    return { spec: { type: "macro", name: m.label, patch: m.patch }, octave: m.octave, name: m.label };
  };
  const preset = (id) => {
    const p = catalog.presets.find((s) => s.id === id);
    return { spec: { type: "synth", preset: id }, octave: p.octave, name: p.label };
  };
  const texture = (id) => {
    const t = catalog.textures.find((s) => s.id === id);
    return { spec: { type: "granular", texture: id, sample: null }, octave: t.octave, name: t.label };
  };
  const kit = (id) => ({ spec: { type: "drums", kit: id }, name: "Drums" });
  /** Make the set's tracks exactly these sounds, in order. */
  const tracks = (sounds) => {
    let p = call({ type: "get_project" });
    for (let i = p.tracks.length; i < sounds.length; i++)
      p = call({ type: "add_track", kind: sounds[i].spec.type === "drums" ? "drums" : "melodic" });
    sounds.forEach((s, i) => {
      // a drum track stays a drum track: put kits where the drums are
      call({ type: "swap", track: i, instrument: s.spec });
      call({ type: "rename_track", track: i, name: s.name });
      if (s.octave !== undefined && s.spec.type !== "drums") call({ type: "set_octave", track: i, octave: s.octave });
    });
  };
  const mix = (track, m) => {
    for (const [param, value] of Object.entries(m)) {
      if (param === "pan") call({ type: "set_pan", track, pan: value });
      else call({ type: "set_macro", track, param, value });
    }
  };
  const loop = (track, slot, bars, notes) => {
    call({ type: "create_pattern", track, slot, bars });
    for (const [step, pitch, len, velocity = 0.8, more = {}] of notes)
      call({
        type: "add_note",
        track,
        slot,
        note: { start: Math.round(step * STEP), length: Math.round(len * STEP), pitch, velocity, ...more },
      });
  };
  const copy = (track, from, to) => call({ type: "duplicate_pattern", track, from, to });
  const save = (file) => {
    const json = call({ type: "save_project" }).json;
    fs.writeFileSync(path.join(out, file), json);
    console.log(file, (json.length / 1024).toFixed(0), "KB");
  };
  return { call, macro, preset, texture, kit, tracks, mix, loop, copy, save };
}

/** Notes for every bar: `bar(b)` gives [step in bar, pitch, len, vel, more] rows. */
const bars = (n, bar) => Array.from({ length: n }, (_, b) => bar(b).map(([s, ...rest]) => [b * 16 + s, ...rest])).flat();

// ------------------------------------------------------------ shadow walk
// A slow boom-bap in A minor: the Shadow kit, sub and air, a soft piano
// line and a dark bell that only sometimes answers.
{
  const s = session();
  const { call } = s;
  call({ type: "set_name", name: "Shadow walk" });
  call({ type: "set_bpm", bpm: 84 });
  call({ type: "set_swing", swing: 0.16 });
  call({ type: "set_scale", root: 9, kind: "minor", track: null });
  s.tracks([s.kit("shadow"), s.preset("shadow_sub"), s.texture("shadow_air"), s.preset("soft_piano"), s.macro("dark_bell")]);
  call({ type: "set_master", master: { volume: 0.8, vinyl: 0.22, tape: 0.34, warmth: 0.42, space: 0.68, pump: 0.4 } });
  s.mix(0, { volume: 0.82, saturation: 0.15, reverb: 0.12 });
  s.mix(1, { volume: 0.8 });
  s.mix(2, { volume: 0.7, reverb: 0.5, cutoff: 0.72, pan: -0.15 });
  s.mix(3, { volume: 0.8, reverb: 0.5, delay: 0.28, pan: 0.12 });
  s.mix(4, { volume: 0.62, reverb: 0.6, delay: 0.35, pan: 0.3 });

  const KICK = 36, SNARE = 37, HALL = 38, HAT = 39, OPEN = 40, RIM = 44, TICK = 47;
  const drums = bars(2, (b) => [
    [0, KICK, 1, 0.95],
    [b ? 6 : 7, KICK, 1, 0.75],
    [10, KICK, 1, 0.85],
    ...(b ? [[11, KICK, 1, 0.5, { chance: 60 }]] : []),
    [4, SNARE, 1, 0.8],
    [12, b ? HALL : SNARE, 1, 0.8],
    ...[0, 2, 4, 6, 8, 10, 12, 14].map((i) => [i, HAT, 1, i % 4 === 0 ? 0.5 : 0.36]),
    [3, HAT, 1, 0.22, { chance: 50 }],
    [11, HAT, 1, 0.22, { chance: 50 }],
    [14, HAT, 1, 0.4, { repeat: 2, chance: 55 }],
    [15, RIM, 1, 0.45, { chance: 40 }],
    ...(b ? [[8, OPEN, 1, 0.3, { chance: 70 }]] : [[7, TICK, 1, 0.3, { chance: 45 }]]),
  ]);
  // Am | Am | Dm | Em
  const roots = [33, 33, 38, 40];
  const sub = bars(4, (b) => [
    [0, roots[b], 6, 0.9],
    [7, roots[b], 2, 0.7],
    [10, roots[b], 4, 0.85],
  ]);
  const air = bars(4, (b) => [[0, [57, 57, 50, 52][b], 16, 0.7]]);
  const piano = [
    [0, 76, 6, 0.66], [6, 72, 4, 0.55], [10, 69, 6, 0.6],
    [16, 71, 4, 0.55], [20, 72, 4, 0.58], [24, 76, 8, 0.66],
    [32, 77, 6, 0.66], [38, 74, 4, 0.55], [42, 69, 6, 0.6],
    [48, 79, 4, 0.6], [52, 76, 4, 0.58], [56, 71, 8, 0.62],
  ];
  const bell = [
    [8, 81, 6, 0.6], [28, 88, 4, 0.5, { chance: 60 }],
    [40, 86, 6, 0.55], [60, 83, 4, 0.5, { chance: 60 }],
  ];
  // A: air and piano · B: the beat comes in · C: everything · D: no drums
  s.loop(2, 0, 4, air);
  s.loop(3, 0, 4, piano);
  s.loop(0, 1, 2, drums);
  s.loop(1, 1, 4, sub);
  s.copy(2, 0, 1);
  for (const t of [0, 1]) s.copy(t, 1, 2);
  for (const t of [2, 3]) s.copy(t, 0, 2);
  s.loop(4, 2, 4, bell);
  s.copy(1, 1, 3);
  s.copy(2, 0, 3);
  s.copy(4, 2, 3);
  ["intro", "beat", "full", "break"].forEach((name, i) => call({ type: "rename_scene", scene: i, name: "ABCD"[i] }));
  for (const scene of [0, 1, 1, 2, 2, 3, 2, 2, 0]) call({ type: "append_scene", scene });
  call({ type: "set_song_fades", fade_in: 0, fade_out: 2 });
  s.save("shadow-walk.mjolnir");
}

// -------------------------------------------------------------------- fog
// Sixty beats a minute in D sharp minor: strings, a choir, the wind, a bowl
// that rings when it wants, and a voice from Silent Focus.
{
  const s = session();
  const { call } = s;
  call({ type: "set_name", name: "Fog" });
  call({ type: "set_bpm", bpm: 60 });
  call({ type: "set_swing", swing: 0 });
  call({ type: "set_scale", root: 3, kind: "minor", track: null });
  s.tracks([
    s.kit("shadow"),
    s.preset("shadow_sub"),
    s.macro("solina"),
    s.macro("choir"),
    s.macro("bowl"),
    s.macro("wind"),
    s.texture("silent_lead"),
  ]);
  call({ type: "set_master", master: { volume: 0.8, vinyl: 0.3, tape: 0.45, warmth: 0.5, space: 0.9, pump: 0.25 } });
  s.mix(0, { volume: 0.7, reverb: 0.55, cutoff: 0.7 });
  s.mix(1, { volume: 0.78 });
  s.mix(2, { volume: 0.72, reverb: 0.55, cutoff: 0.66, pan: -0.2 });
  s.mix(3, { volume: 0.6, reverb: 0.7, pan: 0.2 });
  s.mix(4, { volume: 0.66, reverb: 0.7, delay: 0.4, pan: 0.35 });
  s.mix(5, { volume: 0.5, reverb: 0.6, pan: -0.35 });
  s.mix(6, { volume: 0.7, reverb: 0.6, delay: 0.3 });

  // D#m for two bars, G#m for two
  const strings = [
    [0, 51, 32, 0.6], [0, 54, 32, 0.55], [0, 58, 32, 0.55],
    [32, 44, 32, 0.6], [32, 47, 32, 0.55], [32, 51, 32, 0.55],
  ];
  const choir = [[0, 58, 30, 0.5], [32, 59, 14, 0.5], [48, 56, 14, 0.5]];
  const bowl = [
    [4, 75, 8, 0.6], [26, 70, 6, 0.5, { chance: 60 }],
    [34, 78, 8, 0.55], [60, 73, 4, 0.45, { chance: 50 }],
  ];
  const wind = [[0, 63, 64, 0.6]];
  const sub = [[0, 27, 28, 0.85], [32, 32, 28, 0.85]];
  const lead = [
    [8, 63, 8, 0.6], [16, 61, 4, 0.5], [20, 58, 10, 0.55],
    [40, 59, 8, 0.6], [48, 56, 6, 0.5], [56, 58, 8, 0.55, { chance: 70 }],
  ];
  const drums = [
    [0, 41, 1, 0.8], [32, 41, 1, 0.75],
    [24, 38, 1, 0.45], [56, 38, 1, 0.5],
    [14, 48, 1, 0.25, { chance: 50 }], [46, 48, 1, 0.25, { chance: 50, repeat: 2 }],
    [60, 44, 1, 0.3, { chance: 35 }],
  ];
  // A: strings and wind · B: the ground and the choir · C: everything · D: what is left
  s.loop(2, 0, 4, strings);
  s.loop(5, 0, 4, wind);
  for (const t of [2, 5]) s.copy(t, 0, 1);
  s.loop(1, 1, 4, sub);
  s.loop(3, 1, 4, choir);
  s.loop(4, 1, 4, bowl);
  for (const t of [1, 2, 3, 4, 5]) s.copy(t, 1, 2);
  s.loop(6, 2, 4, lead);
  s.loop(0, 2, 4, drums);
  for (const t of [3, 4, 5]) s.copy(t, 1, 3);
  for (const scene of [0, 1, 2, 2, 3, 2, 0]) call({ type: "append_scene", scene });
  call({ type: "set_song_fades", fade_in: 2, fade_out: 4 });
  s.save("fog.mjolnir");
}

// ----------------------------------------------------------------- signal
// A modular sketch in G minor: a kalimba line that is never quite the same
// twice, a string bass, tine chords and a signal from far away.
{
  const s = session();
  const { call } = s;
  call({ type: "set_name", name: "Signal" });
  call({ type: "set_bpm", bpm: 92 });
  call({ type: "set_swing", swing: 0.08 });
  call({ type: "set_scale", root: 7, kind: "minor", track: null });
  s.tracks([
    s.kit("session"),
    s.macro("pluck_bass"),
    s.macro("drift_table"),
    s.macro("tine"),
    s.macro("kalimba"),
    s.macro("sine_lead"),
  ]);
  call({ type: "set_master", master: { volume: 0.8, vinyl: 0.12, tape: 0.3, warmth: 0.35, space: 0.6, pump: 0.45 } });
  s.mix(0, { volume: 0.8, saturation: 0.2, reverb: 0.1 });
  s.mix(1, { volume: 0.85, cutoff: 0.6 });
  s.mix(2, { volume: 0.55, reverb: 0.5, cutoff: 0.6, pan: -0.25 });
  s.mix(3, { volume: 0.72, reverb: 0.35, delay: 0.2, pan: 0.15 });
  s.mix(4, { volume: 0.7, reverb: 0.4, delay: 0.45, pan: -0.3 });
  s.mix(5, { volume: 0.5, reverb: 0.55, delay: 0.4, pan: 0.3 });

  // Gm | D# | Cm | Dm : root, third, fifth
  const chords = [
    [55, 58, 62],
    [51, 55, 58],
    [48, 51, 55],
    [50, 53, 57],
  ];
  const tine = bars(4, (b) => chords[b].map((p, i) => [0, p + 12, 12, 0.5 - i * 0.04]));
  const pad = bars(4, (b) => [[0, chords[b][0], 16, 0.55], [0, chords[b][2], 16, 0.5]]);
  const bass = bars(4, (b) => [
    [0, chords[b][0] - 12, 3, 0.9],
    [6, chords[b][0] - 12, 2, 0.7],
    [10, chords[b][0] - 12, 3, 0.8],
    [14, chords[b][2] - 12, 2, 0.6, { chance: 60 }],
  ]);
  const order = [0, 1, 2, 1, 3, 2, 1, 2, 0, 2];
  const places = [0, 2, 3, 5, 6, 8, 10, 11, 13, 14];
  const kalimba = bars(4, (b) =>
    places.map((at, i) => {
      const tones = [...chords[b], chords[b][0] + 12];
      const more = i % 4 === 3 ? { chance: 65 } : i === 9 ? { repeat: 2, chance: 80 } : {};
      return [at, tones[order[i]] + 12, 2, 0.5 + (i % 3) * 0.1, more];
    }),
  );
  const lead = [
    [4, 74, 6, 0.55], [12, 70, 4, 0.5],
    [20, 75, 8, 0.55], [36, 72, 6, 0.55],
    [44, 70, 4, 0.5, { chance: 70 }], [52, 77, 4, 0.55], [56, 74, 8, 0.55],
  ];
  const drums = bars(2, (b) => [
    [0, 36, 1, 0.95], [6, 36, 1, 0.7], [10, 36, 1, 0.85],
    [4, 37, 1, 0.8], [12, b ? 38 : 37, 1, 0.8],
    ...Array.from({ length: 16 }, (_, i) => [i, 39, 1, i % 4 === 0 ? 0.5 : i % 2 ? 0.25 : 0.38, i % 2 ? { chance: 80 } : {}]),
    [15, 47, 1, 0.4, { repeat: 3, chance: 50 }],
    ...(b ? [[9, 44, 1, 0.4, { chance: 50 }]] : []),
  ]);
  // A: the line alone · B: bass and drums · C: everything · D: chords and signal
  s.loop(4, 0, 4, kalimba);
  s.loop(2, 0, 4, pad);
  s.loop(0, 1, 2, drums);
  s.loop(1, 1, 4, bass);
  for (const t of [2, 4]) s.copy(t, 0, 1);
  for (const t of [0, 1]) s.copy(t, 1, 2);
  for (const t of [2, 4]) s.copy(t, 0, 2);
  s.loop(3, 2, 4, tine);
  s.loop(5, 2, 4, lead);
  s.copy(3, 2, 3);
  s.copy(5, 2, 3);
  s.copy(2, 0, 3);
  for (const scene of [0, 1, 1, 2, 2, 3, 2, 2, 0]) call({ type: "append_scene", scene });
  call({ type: "set_song_fades", fade_in: 0, fade_out: 2 });
  s.save("signal.mjolnir");
}
