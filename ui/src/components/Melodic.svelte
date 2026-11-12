<script lang="ts">
  import { tx } from "../lib/i18n.svelte";
  import { capture } from "../lib/api";
  // Inside a melodic track: the piano roll above, and below it something to
  // play: a keyboard, or pads of the key's notes (chords, when chords are on).
  import Roll from "./Roll.svelte";
  import Keys from "./Keys.svelte";
  import NoteDetail from "./NoteDetail.svelte";
  import LoopTools from "./LoopTools.svelte";
  import { app, act, live, refreshPads, track, pattern } from "../lib/app.svelte";
  import { INTERVALS, intervalFrom, shiftInKey, type ChordKind, type Note } from "../lib/api";

  const t = $derived(track());
  let held = $state<Record<number, boolean>>({});

  const LENGTHS: [number, string][] = [[1, "1/16"], [2, "1/8"], [4, "1/4"], [8, "1/2"], [16, "1"]];
  const CHORDS: [ChordKind | null, string][] = [[null, "off"], ["triad", "triad"], ["seventh", "7th"], ["sus2", "sus2"], ["sus4", "sus4"], ["power", "5th"]];

  // ---- the whole loop at once: carried to another note or octave, or doubled at an interval
  const pat = $derived(pattern());
  const scale = $derived(t?.scale ?? app.project?.scale ?? { root: 0, kind: "minor" });
  let tool = $state<"move" | "harmony" | null>(null);
  let below = $state(false);

  async function rewrite(notes: Note[]) {
    app.pick = null;
    await act({ type: "set_notes", track: app.track, slot: app.section, notes });
  }
  /** Carry every note: by octaves, or by notes of the key. Nothing moves if one would fall off the keyboard. */
  async function carry(by: { octaves?: number; degrees?: number }) {
    if (!pat?.notes.length) return;
    const moved = pat.notes.map((n) => ({
      ...n,
      pitch: by.octaves ? n.pitch + 12 * by.octaves : shiftInKey(scale, n.pitch, by.degrees ?? 0),
    }));
    if (moved.some((n) => n.pitch < 0 || n.pitch > 127)) return;
    await rewrite(moved);
  }
  /** Double every note at an interval of the key; a note already there is left alone. */
  async function harmonise(interval: (typeof INTERVALS)[number]) {
    if (!pat?.notes.length) return;
    const have = new Set(pat.notes.map((n) => `${n.start}:${n.pitch}`));
    const added: Note[] = [];
    for (const n of pat.notes) {
      const pitch = intervalFrom(scale, n.pitch, interval, below);
      const key = `${n.start}:${pitch}`;
      if (pitch < 0 || pitch > 127 || have.has(key)) continue;
      have.add(key);
      added.push({ ...n, pitch, velocity: Math.max(0.05, n.velocity * 0.85) });
    }
    if (added.length) await rewrite([...pat.notes, ...added]);
  }

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
    const r = (e.currentTarget as Element).getBoundingClientRect();
    const height = 1 - Math.min(1, Math.max(0, (e.clientY - r.top) / r.height));
    live({ type: "pad_on", track: app.track, pad: i, velocity: 0.5 + 0.5 * height });
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
        <span class="label">{tx("note")}</span>
        {#each LENGTHS as [n, name]}
          <button class="mini" aria-pressed={app.noteLen === n} onclick={() => (app.noteLen = n)}>{name}</button>
        {/each}
      </span>
      <span class="group">
        <button class="mini" disabled={!pat?.notes.length} aria-expanded={tool === "move"} aria-pressed={tool === "move"} onclick={() => (tool = tool === "move" ? null : "move")}>{tx("move")}</button>
        <button class="mini" disabled={!pat?.notes.length} aria-expanded={tool === "harmony"} aria-pressed={tool === "harmony"} onclick={() => (tool = tool === "harmony" ? null : "harmony")}>{tx("harmony")}</button>
      </span>
      <LoopTools />
      {#if tool === "move" && pat?.notes.length}
        <span class="group wide from-right" role="group" aria-label={tx("move")}>
          <span class="label">{tx("octave")}</span>
          <button class="mini" aria-label="octave lower" onclick={() => carry({ octaves: -1 })}>−</button>
          <button class="mini" aria-label="octave higher" onclick={() => carry({ octaves: 1 })}>+</button>
          <span class="label gap">{tx("note")}</span>
          <button class="mini" aria-label="note lower" onclick={() => carry({ degrees: -1 })}>−</button>
          <button class="mini" aria-label="note higher" onclick={() => carry({ degrees: 1 })}>+</button>
        </span>
      {:else if tool === "harmony" && pat?.notes.length}
        <span class="group wide from-right" role="group" aria-label={tx("harmony")}>
          <button class="mini" aria-pressed={!below} onclick={() => (below = false)}>{tx("above")}</button>
          <button class="mini" aria-pressed={below} onclick={() => (below = true)}>{tx("below")}</button>
          <span class="label gap">{tx("add a")}</span>
          {#each INTERVALS as iv}
            <button class="mini" onclick={() => harmonise(iv)}>{tx(iv.name)}</button>
          {/each}
        </span>
      {/if}
    </div>

    <div class="roll"><Roll /></div>

    <NoteDetail />

    <div class="head">
      <span class="group">
        <button class="mini" aria-pressed={app.surface === "keys"} onclick={() => (app.surface = "keys")}>{tx("keys")}</button>
        <button class="mini" aria-pressed={app.surface === "pads"} onclick={() => (app.surface = "pads")}>pads</button>
        <button class="mini" aria-label="octave down" onclick={() => octave(-1)}>−</button>
        <span class="oct">{t.octave}</span>
        <button class="mini" aria-label="octave up" onclick={() => octave(1)}>+</button>
      </span>
      <span class="group end">
        <span class="label">{tx("chords")}</span>
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
    display: flex;
    flex-direction: column;
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
  .group.wide {
    flex-basis: 100%;
    flex-wrap: wrap;
  }
  .label.gap {
    margin-left: 8px;
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
  .roll {
    flex: 1.25 1 0;
    min-height: 0;
  }
  .surface {
    flex: 1 1 0;
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
