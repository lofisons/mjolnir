<script lang="ts">
  // The clips face: tracks across, sections down. A cell is a loop; a row
  // plays together.
  import { app, act, selectTrack, selectSection, playing, pattern } from "../lib/app.svelte";
  import { TICKS_PER_STEP, type Pattern } from "../lib/api";

  const tracks = $derived(app.project?.tracks ?? []);
  const scenes = $derived(app.project?.scenes ?? []);

  /** A loop as a row of sixteenths per bar: which ones hold a note. */
  function map(p: Pattern): boolean[] {
    const cells = new Array<boolean>(p.bars * 16).fill(false);
    for (const n of p.notes) {
      const i = Math.round(n.start / TICKS_PER_STEP);
      if (i >= 0 && i < cells.length) cells[i] = true;
    }
    return cells;
  }

  async function cell(t: number, s: number) {
    const same = app.track === t && app.section === s;
    app.section = s;
    if (app.track !== t) await selectTrack(t);
    if (!playing() || app.status?.song_mode) return;
    const active = app.status?.tracks[t]?.active === s;
    // a second touch on the playing loop stops it
    if (active && same) await act({ type: "launch", track: t, slot: null });
    else if (pattern(t, s)) await act({ type: "launch", track: t, slot: s });
  }

  async function addTrack(kind: "drums" | "melodic") {
    await act({ type: "add_track", kind });
    await selectTrack((app.project?.tracks.length ?? 1) - 1);
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

<div class="clips">
  <div class="grid" style="grid-template-columns: minmax(72px, 0.8fr) repeat({tracks.length}, minmax(0, 1fr))">
    <span></span>
    {#each tracks as t, i}
      <button class="plain th" aria-pressed={app.track === i} onclick={() => selectTrack(i)}>
        {t.name.toLowerCase()}
      </button>
    {/each}
    {#each scenes as scene, s}
      <button class="row" aria-pressed={app.section === s} onclick={() => selectSection(s)}>
        <span class="label">{s + 1}</span>
        {scene.name.toLowerCase()}
      </button>
      {#each tracks as t, i}
        {@const p = t.slots[s]}
        {@const st = app.status?.tracks[i]}
        <button
          class="cell"
          class:sel={app.track === i && app.section === s}
          class:live={playing() && st?.active === s}
          class:queued={playing() && st?.queued === s}
          aria-label="{t.name}, section {s + 1}{p ? '' : ', empty'}"
          onclick={() => cell(i, s)}
        >
          {#if p}
            <span class="map" style="grid-template-rows: repeat({p.bars}, 1fr)">
              {#each map(p) as lit}<i class:lit></i>{/each}
            </span>
            {#if playing() && st?.active === s}
              <span class="pos" style="left: {(st?.progress ?? 0) * 100}%"></span>
            {/if}
          {/if}
        </button>
      {/each}
    {/each}
  </div>
  <div class="tools">
    <button onclick={() => act({ type: "add_section" })}>new section</button>
    <button onclick={copySection}>copy section</button>
    <button
      disabled={!pattern()}
      onclick={() => act({ type: "delete_pattern", track: app.track, slot: app.section })}>clear loop</button
    >
    <button disabled={scenes.length < 2} onclick={() => act({ type: "delete_section", scene: app.section })}>
      delete section
    </button>
  </div>
  <div class="tools">
    <button disabled={tracks.length >= 8} onclick={() => addTrack("drums")}>new drum track</button>
    <button disabled={tracks.length >= 8} onclick={() => addTrack("melodic")}>new keys track</button>
  </div>
</div>

<style>
  .clips {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 8px 12px 12px;
    height: 100%;
    overflow-y: auto;
  }
  .grid {
    display: grid;
    gap: 4px;
    grid-auto-rows: minmax(44px, auto);
  }
  .th {
    overflow: hidden;
    text-overflow: ellipsis;
    padding: 0 4px;
    height: 24px;
    align-self: end;
    color: var(--fg-muted);
  }
  .th[aria-pressed="true"] {
    background: none;
    color: var(--fg);
    border-color: transparent;
    box-shadow: inset 0 -1px 0 var(--fg);
  }
  .row {
    height: auto;
    text-align: left;
    padding: 0 8px;
    overflow: hidden;
    text-overflow: ellipsis;
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .row[aria-pressed="true"] .label {
    color: var(--bg);
  }
  .cell {
    position: relative;
    height: auto;
    padding: 6px;
    border-color: var(--line);
    overflow: hidden;
  }
  .cell.sel {
    border-color: var(--fg);
  }
  .cell.queued {
    border-style: dashed;
    border-color: var(--fg);
  }
  .cell.live {
    background: var(--selected);
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
    background: var(--line);
    min-height: 2px;
  }
  .map i.lit {
    background: var(--fg);
  }
  .pos {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--fg);
  }
  .tools {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
</style>
