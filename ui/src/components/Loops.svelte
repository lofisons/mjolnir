<script lang="ts">
  // The loops face, where everything starts: tracks across, sections down.
  // A cell is a loop in its track's colour. Touch an empty one to step in and
  // play; touch a loop to pick it (and start it while the transport runs),
  // touch it again to step in.
  import { app, act, selectTrack, selectSection, openCell, playing, pattern, trackColor } from "../lib/app.svelte";
  import { TICKS_PER_STEP, type Pattern } from "../lib/api";

  const tracks = $derived(app.project?.tracks ?? []);
  const scenes = $derived(app.project?.scenes ?? []);
  let adding = $state(false);

  /** A loop as sixteenths per bar: which ones hold a note. */
  function map(p: Pattern): boolean[] {
    const cells = new Array<boolean>(p.bars * 16).fill(false);
    for (const n of p.notes) {
      const i = Math.round(n.start / TICKS_PER_STEP);
      if (i >= 0 && i < cells.length) cells[i] = true;
    }
    return cells;
  }

  async function cell(t: number, s: number) {
    const p = pattern(t, s);
    const same = app.track === t && app.section === s;
    if (!p || same) return openCell(t, s);
    app.section = s;
    if (app.track !== t) await selectTrack(t);
    if (!playing() || app.status?.song_mode) return;
    const st = app.status?.tracks[t];
    if (st?.active !== s && st?.queued !== s) await act({ type: "launch", track: t, slot: s });
  }

  async function stopTrack(e: Event, t: number) {
    e.stopPropagation();
    await act({ type: "launch", track: t, slot: null });
  }

  async function addTrack(kind: "drums" | "melodic") {
    adding = false;
    await act({ type: "add_track", kind });
    await openCell((app.project?.tracks.length ?? 1) - 1, app.section);
  }

  async function copySection() {
    const from = app.section;
    await act({ type: "add_section" });
    const to = (app.project?.scenes.length ?? 1) - 1;
    for (let t = 0; t < tracks.length; t++)
      if (pattern(t, from)) await act({ type: "duplicate_pattern", track: t, from, to });
    await selectSection(to);
  }
</script>

<div class="loops">
  <div class="grid" style="grid-template-columns: 40px repeat({tracks.length}, minmax(0, 1fr)){tracks.length < 8 ? ' 40px' : ''}">
    <span></span>
    {#each tracks as t, i}
      <button
        class="plain th"
        class:sel={app.track === i}
        class:muted={t.mute}
        style="--id: {trackColor(t)}"
        aria-label="{t.name}: open"
        onclick={() => openCell(i, app.section)}
      >
        <span class="tname">{t.name.toLowerCase()}</span>
        <span class="meter" style="transform: scaleX({Math.min(1, (app.status?.tracks[i]?.level ?? 0) * 2.5)})"></span>
      </button>
    {/each}
    {#if tracks.length < 8}
      <button class="plain th add" aria-label="add a track" aria-expanded={adding} onclick={() => (adding = !adding)}>+</button>
    {/if}

    {#each scenes as scene, s}
      <button
        class="row"
        aria-pressed={app.section === s}
        aria-label="section {scene.name}: play the row"
        onclick={() => selectSection(s)}
      >
        {scene.name.length <= 3 ? scene.name.toLowerCase() : s + 1}
      </button>
      {#each tracks as t, i}
        {@const p = t.slots[s]}
        {@const st = app.status?.tracks[i]}
        {@const live = playing() && !app.status?.song_mode && st?.active === s}
        {@const queued = playing() && st?.queued === s}
        <button
          class="cell"
          class:filled={!!p}
          class:sel={app.track === i && app.section === s}
          class:live
          class:queued
          style="--id: {trackColor(t)}"
          aria-label="{t.name}, section {scene.name}{p ? (live ? ', playing' : '') : ', empty'}"
          onclick={() => cell(i, s)}
        >
          {#if p}
            <span class="map" style="grid-template-rows: repeat({p.bars}, 1fr)">
              {#each map(p) as lit}<i class:lit></i>{/each}
            </span>
            {#if live}
              <span class="sweep" style="transform: scaleX({st?.progress ?? 0})"></span>
              <span
                class="stop"
                role="button"
                tabindex="-1"
                aria-label="stop {t.name}"
                onclick={(e) => stopTrack(e, i)}
                onkeydown={() => {}}
              ></span>
            {/if}
            {#if queued}<span class="led yl blink corner"></span>{/if}
          {:else}
            <span class="plus">+</span>
          {/if}
        </button>
      {/each}
      {#if tracks.length < 8}<span></span>{/if}
    {/each}
  </div>

  {#if adding}
    <div class="tools from-right">
      <span class="label">new track</span>
      <button onclick={() => addTrack("drums")}>drums</button>
      <button onclick={() => addTrack("melodic")}>keys</button>
    </div>
  {/if}

  <div class="tools">
    <span class="label">section</span>
    <button onclick={() => act({ type: "add_section" })}>new</button>
    <button onclick={copySection}>copy</button>
    <button disabled={scenes.length < 2} onclick={() => act({ type: "delete_section", scene: app.section })}>delete</button>
  </div>
</div>

<style>
  .loops {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 8px 16px 12px;
    height: 100%;
    overflow-y: auto;
  }
  .grid {
    display: grid;
    gap: 4px;
    grid-auto-rows: minmax(52px, 84px);
    grid-template-rows: 32px;
    flex: 0 1 auto;
  }
  .th {
    position: relative;
    height: 32px;
    padding: 0 4px;
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    color: var(--fg-muted);
    text-align: left;
  }
  .th.sel {
    color: var(--fg);
  }
  .th.muted .tname {
    color: var(--fg-faint);
  }
  /* the track's colour sits under its name; the level grows along it */
  .th::after {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 2px;
    background: color-mix(in srgb, var(--id, transparent) 35%, transparent);
  }
  .th.add::after {
    display: none;
  }
  .th.add {
    justify-content: center;
  }
  .tname {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meter {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 2px;
    background: var(--id);
    transform-origin: left;
    transform: scaleX(0);
    z-index: 1;
  }
  .row {
    height: auto;
    padding: 0;
    border-color: var(--line);
    color: var(--fg-muted);
  }
  .row[aria-pressed="true"] {
    color: var(--bg);
  }
  .cell {
    position: relative;
    height: auto;
    padding: 6px;
    border-color: var(--line);
    overflow: hidden;
  }
  .cell .plus {
    color: var(--fg-faint);
  }
  .cell.filled {
    border-color: color-mix(in srgb, var(--id) 45%, transparent);
    background: color-mix(in srgb, var(--id) 9%, transparent);
  }
  .cell.sel {
    border-color: var(--fg);
  }
  .cell.live {
    background: color-mix(in srgb, var(--id) 26%, transparent);
  }
  .cell.queued {
    border-style: dashed;
    border-color: var(--yl);
  }
  .map {
    display: grid;
    grid-template-columns: repeat(16, 1fr);
    gap: 1px;
    width: 100%;
    height: 100%;
    min-height: 20px;
  }
  .map i {
    background: color-mix(in srgb, var(--id) 16%, transparent);
    min-height: 2px;
  }
  .map i.lit {
    background: var(--id);
  }
  /* the part of the loop already played, as a wash that grows with it */
  .sweep {
    position: absolute;
    inset: 0;
    background: color-mix(in srgb, var(--id) 22%, transparent);
    transform-origin: left;
    pointer-events: none;
  }
  .stop {
    position: absolute;
    right: 4px;
    bottom: 4px;
    width: 14px;
    height: 14px;
    background: var(--fg);
    border: 3px solid var(--bg);
    box-sizing: content-box;
  }
  .corner {
    position: absolute;
    top: 4px;
    right: 4px;
  }
  .tools {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
  }
  .tools .label {
    margin-right: 4px;
  }
</style>
