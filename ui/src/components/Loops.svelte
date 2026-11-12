<script lang="ts">
  import { tx } from "../lib/i18n.svelte";
  // The loops face, where everything starts: tracks across, sections down.
  // A cell is a loop in its track's colour. One click plays it, another
  // stops it; a double click steps inside. An empty cell has nothing to
  // play, so one click steps in.
  import { app, act, selectSection, openCell, toggleCell, startCell, sounding, playRow, playing, pattern, trackColor } from "../lib/app.svelte";
  import NotePreview from "./NotePreview.svelte";

  const tracks = $derived(app.project?.tracks ?? []);
  const scenes = $derived(app.project?.scenes ?? []);
  let adding = $state(false);

  let last = { t: -1, s: -1, at: 0 };
  async function cell(t: number, s: number) {
    if (!pattern(t, s)) return openCell(t, s);
    const now = performance.now();
    const double = last.t === t && last.s === s && now - last.at < 350;
    if (double) {
      // the first click of the pair already answered: if it stopped the
      // loop, bring it back, then step in with it playing
      last = { t: -1, s: -1, at: 0 };
      if (sounding(t) !== s) await startCell(t, s);
      return openCell(t, s);
    }
    last = { t, s, at: now };
    await toggleCell(t, s);
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
        onclick={() => playRow(s)}
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
          title={p ? "click: play or stop · double click: open" : "open"}
          onclick={() => cell(i, s)}
        >
          {#if p}
            <span class="notes"><NotePreview pattern={p} drums={t.kind === "drums"} /></span>
            {#if live}
              <span class="sweep" style="transform: scaleX({st?.progress ?? 0})"></span>
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
      <span class="label">{tx("new track")}</span>
      <button onclick={() => addTrack("drums")}>{tx("drums")}</button>
      <button onclick={() => addTrack("melodic")}>{tx("keys")}</button>
    </div>
  {/if}

  <div class="tools">
    <span class="label">{tx("section")}</span>
    <input
      type="text"
      class="sname"
      aria-label="name of the section"
      maxlength="12"
      value={scenes[app.section]?.name ?? ""}
      onchange={(e) => act({ type: "rename_scene", scene: app.section, name: e.currentTarget.value.trim() || String(app.section + 1) })}
    />
    <button onclick={() => act({ type: "add_section" })}>{tx("new")}</button>
    <button onclick={copySection}>{tx("copy")}</button>
    <button disabled={scenes.length < 2} onclick={() => act({ type: "delete_section", scene: app.section })}>{tx("delete")}</button>
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
    grid-auto-rows: minmax(40px, 76px);
    grid-template-rows: 36px;
    flex: 0 1 auto;
  }
  .th {
    position: relative;
    height: 36px;
    white-space: normal;
    line-height: 14px;
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
    max-height: 28px;
    overflow-wrap: anywhere;
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
  .notes {
    position: absolute;
    inset: 0;
    color: var(--id);
  }
  /* the part of the loop already played, as a wash that grows with it */
  .sweep {
    position: absolute;
    inset: 0;
    background: color-mix(in srgb, var(--id) 22%, transparent);
    transform-origin: left;
    pointer-events: none;
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
  .sname {
    width: 72px;
  }
</style>
