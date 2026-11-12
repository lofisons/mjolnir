# Working on Mjolnir

## The bar

Simple purpose, simple architecture, simple design, simple implementation. Before adding, ask what can be removed. The interface follows VSGARD: Paper Mono, ink or paper, hairlines, square corners, active means inverted; a track's identity colour tells it apart, signal colours only on an LED beside its word.

## Layout

The engine knows nothing about screens; the interface knows nothing about audio. They meet in `crates/app`: an action goes in as JSON, the project (or nothing) comes back. Add a capability there first, with a test, then draw it.

## Checks

```sh
cargo test --workspace     # the engine: sequencer, DSP, files, no allocation in the audio callback
pnpm check                 # types
pnpm build && pnpm test:ui # the built app, driven in a browser at phone and desktop sizes
```

`cargo run --release -p mjolnir-engine --example parts` prints what each part of the audio path costs; `--example sounds` measures every synth sound's level and pitch; `--example bench -- file.mjolnir` times a whole set.

## Sounds

Sounds are built by scripts and committed as small WAV files: `tools/build_stem_sounds.py` (from the author's stems), `tools/build_instruments.py` (from a CC0 orchestra). A new sound needs its source and licence in `SAMPLES.md`. Measure before you trust: level, pitch, and no click at the loop.

## Words

English first, lowercase labels, no exclamation marks; a button says what happens. Spanish lives in `ui/src/lib/i18n.svelte.ts`. A failure says what happened, then `→` what to try.
