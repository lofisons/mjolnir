<script lang="ts">
  import { capture } from "../lib/api";
  // Inside a melodic track: the piano roll above, and below it something to
  // play: a keyboard, or pads of the key's notes (chords, when chords are on).
  import Roll from "./Roll.svelte";
  import Keys from "./Keys.svelte";
  import NoteDetail from "./NoteDetail.svelte";
  import { app, act, live, pattern, setBars, refreshPads, track } from "../lib/app.svelte";
  import type { ChordKind } from "../lib/api";

  const t = $derived(track());
  const pat = $derived(pattern());
  const bars = $derived(pat?.bars ?? 1);
  let held = $state<Record<number, boolean>>({});

  const LENGTHS: [number, string][] = [[1, "1/16"], [2, "1/8"], [4, "1/4"], [8, "1/2"], [16, "1"]];
  const CHORDS: [ChordKind | null, string][] = [[null, "off"], ["triad", "triad"], ["seventh", "7th"], ["sus2", "sus2"], ["sus4", "sus4"], ["power", "5th"]];

  async function chords(mode: ChordKind | null) {
    await act({ type: "set_chord_mode", track: app.track, mode });
    // chords are spelled by the pads
    if (mode) app.surface = "pads";
    await refreshPads();
  }
  async function octave(by: number) {
    if (!t) return;
    await act({ type: "set_octave", track: app.track, octave: t.octave + by });
    await refreshPads();
  }

  function down(e: PointerEvent, i: number) {
    capture(e);
    app.pad = i;
    held[i] = true;
    live({ type: "pad_on", track: app.track, pad: i, velocity: 0.85 });
  }
  function up(i: number) {
    if (!held[i]) return;
    held[i] = false;
    live({ type: "pad_off", track: app.track, pad: i });
  }
</script>

{#if t}
  <div class="melodic">
    <div class="head">
      <span class="group">
        <span class="label">note</span>
        {#each LENGTHS as [n, name]}
          <button class="mini" aria-pressed={app.noteLen === n} onclick={() => (app.noteLen = n)}>{name}</button>
        {/each}
      </span>
      <span class="group end">
        <span class="label">bars</span>
        {#each [1, 2, 4, 8] as b}
          <button class="mini" aria-pressed={bars === b} onclick={() => setBars(b)}>{b}</button>
        {/each}
        <button class="mini" disabled={!pat} onclick={() => act({ type: "delete_pattern", track: app.track, slot: app.section })}>
          clear
        </button>
      </span>
    </div>

    <div class="roll"><Roll /></div>

    <NoteDetail />

    <div class="head">
      <span class="group">
        <button class="mini" aria-pressed={app.surface === "keys"} onclick={() => (app.surface = "keys")}>keys</button>
        <button class="mini" aria-pressed={app.surface === "pads"} onclick={() => (app.surface = "pads")}>pads</button>
        <button class="mini" aria-label="octave down" onclick={() => octave(-1)}>−</button>
        <span class="oct">{t.octave}</span>
        <button class="mini" aria-label="octave up" onclick={() => octave(1)}>+</button>
      </span>
      <span class="group end">
        <span class="label">chords</span>
        {#each CHORDS as [mode, name]}
          <button class="mini" aria-pressed={(t.chord_mode ?? null) === mode} onclick={() => chords(mode)}>{name}</button>
        {/each}
      </span>
    </div>

    <div class="surface">
      {#if app.surface === "keys"}
        <Keys />
      {:else}
        <div class="pads" role="group" aria-label="pads">
          {#each app.pads as pad, i}
            <button
              class="pad"
              class:hit={held[i]}
              class:root={pad.root}
              style="grid-row: {4 - Math.floor(i / 4)}; grid-column: {(i % 4) + 1}"
              onpointerdown={(e) => down(e, i)}
              onpointerup={() => up(i)}
              onpointercancel={() => up(i)}
              oncontextmenu={(e) => e.preventDefault()}
            >
              <span class="name">{pad.label.toLowerCase()}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .melodic {
    display: grid;
    grid-template-rows: auto minmax(0, 1.25fr) auto auto minmax(0, 1fr);
    grid-template-columns: minmax(0, 1fr);
    gap: 8px;
    padding: 8px 16px 12px;
    height: 100%;
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 12px;
    min-height: 24px;
  }
  .group {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .group.end {
    margin-left: auto;
  }
  .oct {
    min-width: 2ch;
    text-align: center;
  }
  .mini {
    height: 24px;
    min-width: 24px;
    padding: 0 6px;
    border-color: var(--line);
  }
  .roll,
  .surface {
    min-height: 0;
  }
  .pads {
    height: 100%;
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    grid-template-rows: repeat(4, 1fr);
    gap: 4px;
    touch-action: none;
  }
  .pad {
    height: auto;
    min-height: 0;
    padding: 4px 8px;
    display: flex;
    align-items: flex-end;
    text-align: left;
    overflow: hidden;
    transition:
      background var(--dur-base) var(--ease),
      color var(--dur-base) var(--ease);
    touch-action: none;
  }
  .pad .name {
    font: var(--ui-strong);
  }
  .pad.root {
    border-color: var(--fg);
  }
  .pad.hit {
    background: var(--fg);
    color: var(--bg);
    transition: none;
  }
  @media (pointer: coarse) {
    .mini {
      height: 32px;
      min-width: 32px;
    }
  }
</style>
