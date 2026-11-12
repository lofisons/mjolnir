# Architecture

## One engine, one interface, thin hosts

```
        ui (Svelte)            actions as JSON             crates/app
  loops · track · song   ───────────────────────────▶   handle(Action)
  mix · set                ◀──── the project, or null ───      │
        ▲                                                       ▼
        │ status 30×/s (transport, meters, the scope)     crates/engine
        └──────────────────────────────────────────────  Controller ⇄ Engine
```

- **`crates/engine`** owns the project model, the sequencer and all sound. The `Controller` (control side) edits the project and sends commands over a lock-free queue to the `Engine` (audio side), which never allocates or locks. Offline rendering runs the same engine.
- **`crates/app`** is the only door: `Action` in, JSON out. Files and devices belong to the host; a project travels as text.
- **`crates/web`** wraps both behind a plain C interface (`mj_init`, `mj_dispatch`, `mj_process`, `mj_status`, `mj_render`). No generated glue: an AudioWorklet instantiates the module itself.
- **`ui`** draws the project and sends actions. Its state (`ui/src/lib/app.svelte.ts`) is what the engine last answered plus what is selected.

## Hosts

| Host | How it runs the engine | What it lends |
|---|---|---|
| Browser, installed web app | WebAssembly in an AudioWorklet; a second instance in a worker bounces audio | downloads, a file picker, IndexedDB for sets, Web MIDI |
| Desktop (`desktop/`) | the same, inside Electron's Chromium, served from its own `app://` scheme | a menu, file association, save dialogs |
| A DAW (`crates/plugin`) | natively: the engine on the DAW's audio thread, a helper thread for everything that allocates | notes, tempo, transport, parameters and their automation, state |
| Android (`android/`) | the same, inside a WebView, served from the package at `https://mjolnir.app` | saving to Downloads, opening `.mjolnir` files, the microphone, the back button |

Because every app host runs the same module in a Chromium (or WebKit) audio worklet, what is tested in one is what sounds in the others. The plugin is the exception: the same engine compiled natively, with one track and no interface of its own.

## Sound

- **Instruments**: sampled kits (slices of one buffer), a zoned sampler with loops for the real instruments, a granular player for the airs and leads, a drum synth, a subtractive synth, and the macro synth (`macro_synth.rs`: ten engines, one voice structure).
- **Per track** (`fx.rs`): filter, drive, saturation, volume, and two sends.
- **Shared** (`space.rs`): a plate reverb after Dattorro and a tape echo; the pump follows the low end of the drums and ducks everything else; the punch effects sit over the sum.
- **Master** (`fx.rs`): tape wow and flutter, warmth, vinyl, a limiter.

`examples/parts` prints the cost of each stage in nanoseconds per sample; the audio callback is tested never to allocate.

## Files

A set is JSON: the project plus any recordings as base64 WAV. Notes carry start, length, pitch, level and, when not the default, chance and repeat. Sounds that are no longer offered (the first synth presets, the Dust kit) still load.

## Why not native audio

A native audio path per platform would lower latency on some machines and cost a second engine host to build, sign and test on each. The worklet path is one path. The door for a native host stays open: `crates/app` needs only a callback that asks for samples.
