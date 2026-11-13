<script lang="ts">
  import { tx } from "../lib/i18n.svelte";
  import { capture } from "../lib/api";
  // Inside a melodic track: the piano roll above, and below it something to
  // play: a keyboard, or pads of the key's notes (chords, when chords are on).
  import Roll from "./Roll.svelte";
  import Keys from "./Keys.svelte";
  import NoteDetail from "./NoteDetail.svelte";
  import { app, act, live, refreshPads, track, pattern, setBars, pasteLoop } from "../lib/app.svelte";
  import { INTERVALS, intervalFrom, shiftInKey, type ChordKind, type Note } from "../lib/api";

  const t = $derived(track());
  let held = $state<Record<number, boolean>>({});

  const LENGTHS: [number, string][] = [[1, "1/16"], [2, "1/8"], [4, "1/4"], [8, "1/2"], [16, "1"]];
  const CHORDS: [ChordKind | null, string][] = [[null, "off"], ["triad", "triad"], ["seventh", "7th"], ["sus2", "sus2"], ["sus4", "sus4"], ["power", "5th"]];

  // ---- the whole loop at once: carried to another note or octave, or doubled at an interval
  const pat = $derived(pattern());
  const scale = $derived(t?.scale ?? app.project?.scale ?? { root: 0, kind: "minor" });
  let below = $state(false);

  // ---- one sheet at a time, over the roll: the choice asked for, and nothing else on screen
  type Sheet = "length" | "bars" | "chords" | "edit";
  let sheet = $state<Sheet | null>(null);
  const toggle = (which: Sheet) => (sheet = sheet === which ? null : which);
  const bars = $derived(pat?.bars ?? 1);
  const lengthName = $derived(LENGTHS.find(([n]) => n === app.noteLen)?.[1] ?? "1/8");
  const chordName = $derived(CHORDS.find(([m]) => m === (t?.chord_mode ?? null))?.[1] ?? "off");
  const copied = $derived(app.clip?.track === app.track && app.clip?.slot === app.section);

  // ---- the keyboard folds away: on a small screen the roll needs the room
  const KEPT = "mjolnir.keys";
  function firstKeys(): boolean {
    try {
      const kept = localStorage.getItem(KEPT);
      if (kept) return kept === "shown";
    } catch {
      /* private window */
    }
    // out on a screen with room for both; folded on a phone
    return typeof innerHeight === "number" ? innerHeight >= 720 && innerWidth >= 700 : true;
  }
  let keysShown = $state(firstKeys());
  function showKeys(on: boolean) {
    keysShown = on;
    try {
      localStorage.setItem(KEPT, on ? "shown" : "hidden");
    } catch {
      /* private window */
    }
  }

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
    sheet = null;
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

<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape" && sheet) {
      e.preventDefault();
      sheet = null;
    }
  }}
/>

{#if t}
  <div class="melodic">
    <!-- one line of tools; a picked note takes it over with its own -->
    <div class="bar">
      {#if app.pick && pat}
        <NoteDetail inline />
      {:else}
        <button class="tool" aria-expanded={sheet === "length"} aria-pressed={sheet === "length"} aria-label="note length: {lengthName}" onclick={() => toggle("length")}>
          <span class="label">{tx("note")}</span>{lengthName}
        </button>
        <button class="tool" aria-expanded={sheet === "bars"} aria-pressed={sheet === "bars"} aria-label="bars: {bars}" onclick={() => toggle("bars")}>
          <span class="label">bars</span>{bars}
        </button>
        <button class="tool" aria-expanded={sheet === "chords"} aria-pressed={sheet === "chords"} aria-label="chords: {chordName}" onclick={() => toggle("chords")}>
          <span class="label">{tx("chords")}</span>{chordName}
        </button>
        <span class="grow"></span>
        <button class="tool" aria-expanded={sheet === "edit"} aria-pressed={sheet === "edit"} onclick={() => toggle("edit")}>{tx("edit")}</button>
      {/if}
    </div>

    <div class="stage">
      <Roll />
      {#if sheet}
        <!-- a touch on the roll behind an open sheet closes it, and writes nothing -->
        <button class="veil" aria-label="close" onclick={() => (sheet = null)}></button>
        <div class="sheet" role="group" aria-label={sheet}>
          {#if sheet === "length"}
            <div class="line">
              <span class="label">{tx("note")}</span>
              {#each LENGTHS as [n, name]}
                <button
                  class="opt"
                  aria-pressed={app.noteLen === n}
                  onclick={() => {
                    app.noteLen = n;
                    sheet = null;
                  }}>{name}</button
                >
              {/each}
            </div>
          {:else if sheet === "bars"}
            <div class="line">
              <span class="label">bars</span>
              {#each [1, 2, 4, 8] as b}
                <button
                  class="opt"
                  aria-pressed={bars === b}
                  onclick={() => {
                    void setBars(b);
                    sheet = null;
                  }}>{b}</button
                >
              {/each}
            </div>
          {:else if sheet === "chords"}
            <div class="line">
              <span class="label">{tx("chords")}</span>
              {#each CHORDS as [mode, name]}
                <button class="opt" aria-pressed={(t.chord_mode ?? null) === mode} onclick={() => chords(mode)}>{name}</button>
              {/each}
            </div>
          {:else}
            <div class="line" role="group" aria-label={tx("move")}>
              <span class="label">{tx("move")}</span>
              <span class="pair">
                <button class="opt" disabled={!pat?.notes.length} aria-label="octave lower" onclick={() => carry({ octaves: -1 })}>−</button>
                <span class="unit">{tx("octave")}</span>
                <button class="opt" disabled={!pat?.notes.length} aria-label="octave higher" onclick={() => carry({ octaves: 1 })}>+</button>
              </span>
              <span class="pair">
                <button class="opt" disabled={!pat?.notes.length} aria-label="note lower" onclick={() => carry({ degrees: -1 })}>−</button>
                <span class="unit">{tx("note")}</span>
                <button class="opt" disabled={!pat?.notes.length} aria-label="note higher" onclick={() => carry({ degrees: 1 })}>+</button>
              </span>
            </div>
            <div class="line" role="group" aria-label={tx("harmony")}>
              <span class="label">{tx("harmony")}</span>
              <button class="opt" aria-pressed={!below} onclick={() => (below = false)}>{tx("above")}</button>
              <button class="opt" aria-pressed={below} onclick={() => (below = true)}>{tx("below")}</button>
              <span class="sep"></span>
              {#each INTERVALS as iv}
                <button class="opt" disabled={!pat?.notes.length} onclick={() => harmonise(iv)}>{tx(iv.name)}</button>
              {/each}
            </div>
            <div class="line" role="group" aria-label="loop">
              <span class="label">loop</span>
              <button class="opt" disabled={!pat} aria-pressed={copied} onclick={() => (app.clip = { track: app.track, slot: app.section })}>
                {copied ? tx("copied") : tx("copy")}
              </button>
              <button class="opt" disabled={!app.clip || copied} onclick={pasteLoop}>{tx("paste")}</button>
              <button class="opt" disabled={!pat} onclick={() => act({ type: "quantize", track: app.track, slot: app.section })}>{tx("snap")}</button>
              {#if pat?.automation.length}
                <button class="opt" onclick={() => act({ type: "clear_automation", track: app.track, slot: app.section, param: null })}>
                  {tx("forget knob moves")}
                </button>
              {/if}
              <button class="opt" disabled={!pat} onclick={() => act({ type: "delete_pattern", track: app.track, slot: app.section })}>{tx("clear")}</button>
            </div>
          {/if}
        </div>
      {/if}
    </div>

    <!-- what plays the track by hand; folded away, the roll has the whole screen -->
    <div class="playbar">
      <button class="tool" aria-expanded={keysShown} aria-pressed={keysShown && app.surface === "keys"} onclick={() => (keysShown && app.surface === "keys" ? showKeys(false) : ((app.surface = "keys"), showKeys(true)))}>
        {tx("keys")}
      </button>
      <button class="tool" aria-expanded={keysShown} aria-pressed={keysShown && app.surface === "pads"} onclick={() => (keysShown && app.surface === "pads" ? showKeys(false) : ((app.surface = "pads"), showKeys(true)))}>
        pads
      </button>
      <span class="grow"></span>
      <span class="pair">
        <button class="tool sq" aria-label="octave down" onclick={() => octave(-1)}>−</button>
        <span class="unit" aria-label="octave">{t.octave}</span>
        <button class="tool sq" aria-label="octave up" onclick={() => octave(1)}>+</button>
      </span>
    </div>

    {#if keysShown}
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
    {/if}
  </div>
{/if}

<style>
  .melodic {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  /* the two bars keep one height whatever they hold, so nothing below them jumps */
  .bar,
  .playbar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 52px;
    padding: 0 16px;
    min-width: 0;
  }
  .bar {
    border-bottom: 1px solid var(--line);
  }
  .playbar {
    border-top: 1px solid var(--line);
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .tool {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 10px;
    border-color: var(--line);
    font-variant-numeric: tabular-nums;
  }
  .tool .label {
    color: inherit;
    opacity: 0.62;
  }
  .tool.sq {
    width: 32px;
    padding: 0;
    justify-content: center;
  }
  .pair {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .unit {
    min-width: 2ch;
    text-align: center;
    font: var(--tick);
    color: var(--fg-muted);
    font-variant-numeric: tabular-nums;
  }
  .playbar .unit {
    font: var(--ui);
    color: var(--fg);
  }

  .stage {
    position: relative;
    flex: 1 1 0;
    min-height: 0;
  }
  .veil {
    position: absolute;
    inset: 0;
    z-index: 3;
    width: 100%;
    height: 100%;
    border: 0;
    padding: 0;
    background: color-mix(in srgb, var(--bg) 55%, transparent);
    cursor: default;
  }
  .sheet {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    z-index: 4;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    border-bottom: 1px solid var(--edge);
    animation: drop var(--dur-fast) var(--ease);
  }
  @keyframes drop {
    from {
      clip-path: inset(0 0 100% 0);
    }
    to {
      clip-path: inset(0 0 0 0);
    }
  }
  .line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 10px 16px;
  }
  .line + .line {
    border-top: 1px solid var(--line);
  }
  .line > .label {
    flex: none;
    width: 56px;
  }
  .opt {
    height: 36px;
    min-width: 40px;
    padding: 0 10px;
    border-color: var(--line);
  }
  .sep {
    width: 1px;
    height: 20px;
    margin: 0 4px;
    background: var(--line);
  }

  .surface {
    flex: none;
    height: clamp(120px, 24%, 172px);
    padding: 0 16px 12px;
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
    .tool {
      height: 36px;
    }
    .tool.sq {
      width: 36px;
    }
    .opt {
      height: 44px;
      min-width: 44px;
    }
  }
  @media (max-width: 420px) {
    .bar,
    .playbar {
      padding: 0 12px;
    }
    .line {
      padding: 10px 12px;
    }
    .line > .label {
      width: 100%;
    }
  }
</style>
