# Mjolnir

An open instrument for sketching beats: pads, steps, loops and a small timeline, the same on a phone, a browser and a desktop. Made for slow, dark, dusty music.

Status: in the workshop. It plays, records, arranges and exports in a browser; the desktop shell and the plugin are on the way.

## What is here

| Path | What |
|---|---|
| `crates/engine` | The audio engine, sequencer and project model. Pure Rust, hand-written DSP, real-time safe, tested. |
| `crates/app` | Every action a person can take, as JSON in and JSON out. One door for every host. |
| `crates/web` | The engine as a WebAssembly module with a plain C interface, for an AudioWorklet. |
| `ui` | The interface (Svelte), drawn with VSGARD. It talks to the engine only through actions. |

The same engine and the same interface run everywhere. A host only provides sound out and files:

- **Browser and phone**: WebAssembly inside an AudioWorklet. Works today.
- **Desktop**: a native shell around the same interface, with the engine on the system's audio. Next.
- **Plugin** (CLAP, VST3): the engine with its sounds; the DAW keeps the timeline. Later.

## Run it

Needs Rust with the `wasm32-unknown-unknown` target, Node 22 and pnpm.

```sh
rustup target add wasm32-unknown-unknown
pnpm install
pnpm wasm      # build the engine into ui/public/mjolnir.wasm
pnpm dev       # http://localhost:5173
```

`pnpm build` writes a static site to `dist/`: any static host serves it.

## Check it

```sh
cargo test --workspace
pnpm check
```

## Play it

- **loops** is home: tracks across, eight sections down, each track in its own colour. A click plays a loop and another stops it; a double click steps inside. An empty cell opens on a click. A section's letter plays its row.
- **inside a drum track**: touch a pad to hear it and select it; the sixteen step keys write it into the loop.
- **inside a melodic track**: a piano roll (touch to write a note, drag to move it, drag its right edge to stretch it, touch it to remove it) over a keyboard or pads. `chords` writes and plays the chord of the key on every note.
- **sound**: sounds filed by kind (bass, keys, pads, leads; kits and chops for drums), the seven macros, and for a synth every part of it.
- **song**: one lane per track with a loop in hand; a click places it, drag a clip to move it, its right edge to change its length, ctrl-drag to copy, double click to open it. Export WAV and MIDI.
- **set**: tempo, swing, key, the master's tape, files.

Keys: `space` plays and stops; in a drum track `z x c v / a s d f / q w e r / 1 2 3 4` are the pads; in a melodic track `a w s e d f t g y h u j k` is a piano; `esc` goes back to the loops; `tab` changes face; `ctrl z` undoes; `delete` removes the picked clip.

Work is kept in the browser between visits; `save file` writes a `.mjolnir` file you own.

## Where it comes from

The engine was written for Yggdrasil and is carried over whole; Yggdrasil's project files (`.yggdrasil`) open here. The Shadow and Session kits, the chops, the sampled subs, the airs and the leads are cut from lofisons' own tracks; the soft piano, strings and other real instruments come from a CC0 orchestra: see [SAMPLES.md](SAMPLES.md). Code: MIT.
