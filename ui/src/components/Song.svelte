<script lang="ts">
  // The song face: one lane per track, loops placed as clips on a bar grid.
  // Each lane has a loop in hand (its picker); a click on the lane places it.
  // Drag a clip to move it, its right edge to change its length, hold ctrl
  // or alt to drag a copy; a double click opens the loop.
  import NotePreview from "./NotePreview.svelte";
  import { app, act, openCell, trackColor } from "../lib/app.svelte";
  import { barTicks, type SongClip, capture } from "../lib/api";

  const tracks = $derived(app.project?.tracks ?? []);
  const scenes = $derived(app.project?.scenes ?? []);
  const song = $derived(app.project?.song);
  const clips = $derived(song?.clips ?? []);
  const LANE = 56;
  const RULER = 24;
  let barW = $state(22);

  const songBars = $derived(clips.reduce((m, c) => Math.max(m, c.start + c.bars), 0));
  const shownBars = $derived(Math.max(32, Math.ceil((songBars + 8) / 4) * 4));
  const seconds = $derived(songBars * (app.project?.meter?.beats ?? 4) * (60 / (app.project?.bpm ?? 120)));
  const startBar = $derived(app.project?.view.start_bar ?? 0);
  const letter = (slot: number) => {
    const n = scenes[slot]?.name ?? "";
    return n && n.length <= 3 ? n.toLowerCase() : String(slot + 1);
  };

  // ---- the loop each lane has in hand
  let brush = $state<Record<number, number>>({});
  const filled = (ti: number) => tracks[ti].slots.map((p, i) => (p ? i : -1)).filter((i) => i >= 0);
  function inHand(ti: number): number | null {
    const t = tracks[ti];
    const b = brush[t.id];
    if (b != null && t.slots[b]) return b;
    if (t.slots[app.section]) return app.section;
    return filled(ti)[0] ?? null;
  }

  let picked = $state<{ track: number; start: number } | null>(null);
  const pickedIndex = $derived(picked ? clips.findIndex((c) => c.track === picked!.track && c.start === picked!.start) : -1);
  const laneOf = (trackId: number) => tracks.findIndex((t) => t.id === trackId);
  const patternOf = (c: SongClip) => tracks[laneOf(c.track)]?.slots[c.slot] ?? null;

  async function place(ti: number, bar: number) {
    const slot = inHand(ti);
    if (slot == null) {
      app.note = `${tracks[ti].name.toLowerCase()} has no loop yet → make one in loops`;
      return;
    }
    app.track = ti;
    if (await act({ type: "add_clip", track: ti, slot, start: bar, bars: null })) picked = { track: tracks[ti].id, start: bar };
  }
  function laneClick(e: MouseEvent, ti: number) {
    const r = (e.currentTarget as Element).getBoundingClientRect();
    void place(ti, Math.max(0, Math.floor((e.clientX - r.left) / barW)));
  }
  function seek(e: PointerEvent) {
    const r = (e.currentTarget as Element).getBoundingClientRect();
    void act({ type: "seek", bar: Math.max(0, Math.floor((e.clientX - r.left) / barW)) });
  }

  // ---- move, stretch, copy
  let drag = $state<{ index: number; mode: "move" | "stretch"; copy: boolean; x0: number; start: number; bars: number; moved: boolean } | null>(null);
  function clipDown(e: PointerEvent, index: number, mode: "move" | "stretch") {
    e.stopPropagation();
    capture(e);
    const c = clips[index];
    drag = { index, mode, copy: e.ctrlKey || e.altKey, x0: e.clientX, start: c.start, bars: c.bars, moved: false };
    picked = { track: c.track, start: c.start };
  }
  function clipMove(e: PointerEvent) {
    if (!drag) return;
    const c = clips[drag.index];
    const delta = Math.round((e.clientX - drag.x0) / barW);
    if (delta !== 0) drag.moved = true;
    if (drag.mode === "move") drag.start = Math.max(0, c.start + delta);
    else drag.bars = Math.max(1, c.bars + delta);
  }
  async function clipUp() {
    if (!drag) return;
    const d = drag;
    drag = null;
    if (!d.moved) return;
    const c = clips[d.index];
    const ti = laneOf(c.track);
    if (d.copy && d.mode === "move") await act({ type: "add_clip", track: ti, slot: c.slot, start: d.start, bars: c.bars });
    else await act({ type: "edit_clip", index: d.index, track: ti, start: d.start, bars: d.bars });
    picked = { track: c.track, start: d.mode === "move" ? d.start : c.start };
  }
  const view = (c: SongClip, i: number) =>
    drag && drag.index === i
      ? { start: drag.mode === "move" ? drag.start : c.start, bars: drag.mode === "stretch" ? drag.bars : c.bars }
      : c;

  async function remove() {
    if (pickedIndex < 0) return;
    await act({ type: "delete_clip", index: pickedIndex });
    picked = null;
  }
  function key(e: KeyboardEvent) {
    const el = e.target as HTMLElement;
    if (el.tagName === "INPUT" || el.tagName === "SELECT") return;
    if (e.key === "Escape") picked = null;
    if ((e.key === "Delete" || e.key === "Backspace") && pickedIndex >= 0) {
      e.preventDefault();
      void remove();
    }
  }

  const playX = $derived.by(() => {
    const st = app.status;
    if (!st?.playing || !st.song_mode) return -1;
    return (st.tick / barTicks(app.project?.meter)) * barW;
  });
  const used = $derived(scenes.map((sc, i) => ({ i, name: sc.name })).filter((s) => tracks.some((t) => t.slots[s.i])));
</script>

<svelte:window onkeydown={key} />

<div class="song">
  <div class="tools">
    <span class="label">add a row</span>
    {#each used as s}
      <button onclick={() => act({ type: "append_scene", scene: s.i })}>{letter(s.i)}</button>
    {/each}
    {#if !used.length}<span class="quiet">no loops yet</span>{/if}
    <span class="grow"></span>
    {#if pickedIndex >= 0}<button onclick={remove}>remove clip</button>{/if}
    <button aria-label="zoom out" onclick={() => (barW = Math.max(10, barW - 6))}>−</button>
    <button aria-label="zoom in" onclick={() => (barW = Math.min(76, barW + 6))}>+</button>
  </div>

  <div class="body">
    <div class="heads">
      <div class="corner label">{songBars} bars</div>
      {#each tracks as t, ti}
        {@const slots = filled(ti)}
        <div class="head" class:sel={app.track === ti} style="height: {LANE}px; --id: {trackColor(t)}">
          <button class="plain tname" onclick={() => (app.track = ti)}>{t.name.toLowerCase()}</button>
          <select
            aria-label="loop in hand for {t.name}"
            disabled={!slots.length}
            value={inHand(ti) ?? ""}
            onchange={(e) => (brush[t.id] = +e.currentTarget.value)}
          >
            {#each slots as s}<option value={s}>loop {letter(s)}</option>{/each}
            {#if !slots.length}<option value="">no loops</option>{/if}
          </select>
        </div>
      {/each}
    </div>

    <div class="scroll">
      <div class="canvas" style="width: {shownBars * barW}px; height: {RULER + tracks.length * LANE}px">
        <div class="ruler" onpointerdown={seek} role="slider" tabindex="-1" aria-label="start position" aria-valuenow={startBar + 1}>
          {#each { length: shownBars } as _, b}
            {#if b % 4 === 0}<span class="tick label" style="left: {b * barW}px">{b + 1}</span>{/if}
          {/each}
          <span class="marker" style="left: {startBar * barW}px"></span>
        </div>
        {#each tracks as t, ti}
          <div
            class="lane"
            class:sel={app.track === ti}
            style="top: {RULER + ti * LANE}px; height: {LANE}px; background-size: {barW}px 100%, {barW * 4}px 100%"
            role="button"
            tabindex="-1"
            aria-label="{t.name}: place the loop in hand"
            onclick={(e) => laneClick(e, ti)}
            onkeydown={() => {}}
          ></div>
        {/each}
        {#each clips as c, i}
          {@const ti = laneOf(c.track)}
          {@const v = view(c, i)}
          {@const p = patternOf(c)}
          {#if ti >= 0}
            <div
              class="clip"
              class:picked={pickedIndex === i}
              class:ghost={drag?.index === i && drag.copy}
              style="left: {v.start * barW}px; width: {v.bars * barW - 1}px; top: {RULER + ti * LANE + 3}px; height: {LANE - 6}px; --id: {trackColor(tracks[ti])}"
              role="button"
              tabindex="-1"
              aria-label="{tracks[ti].name}, loop {letter(c.slot)}, bar {c.start + 1}, {c.bars} bars"
              title="drag: move · right edge: length · ctrl drag: copy · double click: open"
              onpointerdown={(e) => clipDown(e, i, "move")}
              onpointermove={clipMove}
              onpointerup={clipUp}
              onpointercancel={() => (drag = null)}
              ondblclick={() => openCell(ti, c.slot)}
            >
              {#if p}<NotePreview pattern={p} drums={tracks[ti].kind === "drums"} repeat={v.bars / p.bars} />{/if}
              <span class="tag">{letter(c.slot)}</span>
              <span
                class="handle"
                role="separator"
                aria-label="length"
                onpointerdown={(e) => clipDown(e, i, "stretch")}
                onpointermove={clipMove}
                onpointerup={clipUp}
              ></span>
            </div>
          {/if}
        {/each}
        {#if songBars > 0 && (song?.fade_in || song?.fade_out)}
          <svg class="fades" width={songBars * barW} height={RULER + tracks.length * LANE} aria-hidden="true">
            {#if song?.fade_in}
              <polygon points="0,{RULER} 0,{RULER + tracks.length * LANE} {song.fade_in * barW},{RULER}" />
            {/if}
            {#if song?.fade_out}
              <polygon
                points="{songBars * barW},{RULER} {songBars * barW},{RULER + tracks.length * LANE} {(songBars - song.fade_out) * barW},{RULER}"
              />
            {/if}
          </svg>
        {/if}
        {#if playX >= 0}<span class="play" style="transform: translateX({playX}px)"></span>{/if}
      </div>
    </div>
  </div>

  <div class="tools foot">
    <span>{Math.floor(seconds / 60)}:{String(Math.round(seconds % 60)).padStart(2, "0")}</span>
    <button aria-pressed={song?.looping ?? false} onclick={() => act({ type: "set_song_looping", looping: !song?.looping })}>loop</button>
    <label class="label">
      fade in
      <select
        value={song?.fade_in ?? 0}
        onchange={(e) => act({ type: "set_song_fades", fade_in: +e.currentTarget.value, fade_out: song?.fade_out ?? 0 })}
      >
        {#each [0, 1, 2, 4, 8] as b}<option value={b}>{b || "none"}</option>{/each}
      </select>
    </label>
    <label class="label">
      fade out
      <select
        value={song?.fade_out ?? 0}
        onchange={(e) => act({ type: "set_song_fades", fade_in: song?.fade_in ?? 0, fade_out: +e.currentTarget.value })}
      >
        {#each [0, 1, 2, 4, 8] as b}<option value={b}>{b || "none"}</option>{/each}
      </select>
    </label>
    <span class="grow"></span>
    <button disabled={!songBars} onclick={() => act({ type: "clear_song" })}>clear</button>
    <button onclick={() => (app.exporting = true)}>export</button>
  </div>
</div>

<style>
  .song {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    grid-template-columns: minmax(0, 1fr);
    gap: 8px;
    padding: 8px 16px 12px;
    height: 100%;
  }
  .tools {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
    min-height: var(--row);
  }
  .tools > .label {
    margin-right: 4px;
  }
  .quiet {
    color: var(--fg-faint);
  }
  .foot {
    gap: 4px 8px;
  }
  .grow {
    flex: 1;
  }
  select {
    font: inherit;
    color: inherit;
    background: var(--bg);
    border: 1px solid var(--edge);
    border-radius: 0;
    height: 24px;
    max-width: 100%;
  }
  .foot label {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .body {
    display: grid;
    grid-template-columns: 96px minmax(0, 1fr);
    min-height: 0;
    overflow-y: auto;
    border: 1px solid var(--line);
  }
  .corner {
    height: 24px;
    display: flex;
    align-items: center;
    padding: 0 6px;
    border-bottom: 1px solid var(--line);
  }
  .head {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 2px;
    padding: 0 6px;
    border-bottom: 1px solid var(--line);
    border-left: 3px solid var(--id);
    min-width: 0;
  }
  .head.sel {
    background: var(--hover);
  }
  .tname {
    height: 20px;
    padding: 0;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .scroll {
    overflow-x: auto;
    overflow-y: hidden;
    border-left: 1px solid var(--line);
    overscroll-behavior-x: contain;
  }
  .canvas {
    position: relative;
  }
  .ruler {
    position: absolute;
    inset: 0 0 auto 0;
    height: 24px;
    border-bottom: 1px solid var(--line);
    cursor: pointer;
  }
  .tick {
    position: absolute;
    bottom: 4px;
    padding-left: 3px;
    border-left: 1px solid var(--edge);
  }
  /* where playback starts */
  .marker {
    position: absolute;
    bottom: 0;
    width: 7px;
    height: 7px;
    margin-left: -3px;
    background: var(--fg);
    clip-path: polygon(0 0, 100% 0, 50% 100%);
  }
  .lane {
    position: absolute;
    left: 0;
    right: 0;
    border-bottom: 1px solid var(--line);
    /* a hairline on every bar, a stronger one every four */
    background-image:
      linear-gradient(to right, var(--line) 1px, transparent 1px), linear-gradient(to right, var(--edge) 1px, transparent 1px);
    cursor: copy;
  }
  .lane.sel {
    background-color: var(--hover);
  }
  .clip {
    position: absolute;
    background: color-mix(in srgb, var(--id) 82%, var(--bg));
    color: var(--bg);
    border: 1px solid var(--bg);
    overflow: hidden;
    cursor: grab;
    touch-action: none;
    user-select: none;
  }
  .clip.picked {
    outline: 1px solid var(--fg);
    z-index: 1;
  }
  .clip.ghost {
    opacity: 0.6;
  }
  .tag {
    position: absolute;
    left: 3px;
    top: 1px;
    font: var(--tick);
    pointer-events: none;
  }
  .handle {
    position: absolute;
    top: 0;
    bottom: 0;
    right: 0;
    width: 9px;
    cursor: ew-resize;
    border-right: 3px solid color-mix(in srgb, var(--bg) 45%, transparent);
    touch-action: none;
  }
  .fades {
    position: absolute;
    left: 0;
    top: 0;
    pointer-events: none;
  }
  .fades polygon {
    fill: var(--bg);
    opacity: 0.55;
  }
  .play {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    width: 1px;
    background: var(--fg);
    pointer-events: none;
    z-index: 2;
  }
</style>
