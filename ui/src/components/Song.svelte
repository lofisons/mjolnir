<script lang="ts">
  // The song face: one lane per track, loops placed as clips on a bar grid.
  // Pick a section, touch a lane to place that track's loop there; drag a
  // clip to move it; touch it to pick it.
  import { app, act, exportSong, trackColor } from "../lib/app.svelte";

  const tracks = $derived(app.project?.tracks ?? []);
  const scenes = $derived(app.project?.scenes ?? []);
  const song = $derived(app.project?.song);
  const clips = $derived(song?.clips ?? []);
  const length = $derived(Math.max(0, ...clips.map((c) => c.start + c.bars)));
  const span = $derived(Math.max(16, Math.ceil((length + 4) / 8) * 8));
  const beats = $derived(app.project?.meter?.beats ?? 4);
  const head = $derived.by(() => {
    const s = app.status;
    if (!s || !s.song_mode || !s.playing) return (app.project?.view.start_bar ?? 0) / span;
    return (s.bar + s.beat / beats) / span;
  });
  const seconds = $derived(length * beats * (60 / (app.project?.bpm ?? 120)));
  const clock = (t: number) => `${Math.floor(t / 60)}:${String(Math.floor(t % 60)).padStart(2, "0")}`;
  const label = (slot: number) => {
    const n = scenes[slot]?.name ?? "";
    return n.length <= 3 && n ? n.toLowerCase() : String(slot + 1);
  };

  let picked = $state<{ track: number; start: number } | null>(null);
  const pickedIndex = $derived(picked ? clips.findIndex((c) => c.track === picked!.track && c.start === picked!.start) : -1);
  let lanes: HTMLDivElement;
  let drag = $state<{ index: number; x0: number; start: number; from: number; moved: boolean } | null>(null);

  const barAt = (x: number, el: Element) => {
    const r = el.getBoundingClientRect();
    return Math.max(0, Math.min(span - 1, Math.floor(((x - r.left) / r.width) * span)));
  };

  function seek(e: PointerEvent) {
    void act({ type: "seek", bar: barAt(e.clientX, e.currentTarget as Element) });
  }

  async function place(e: PointerEvent, ti: number) {
    if (e.target !== e.currentTarget) return;
    const t = tracks[ti];
    const slot = t.slots[app.section] ? app.section : t.slots.findIndex(Boolean);
    if (slot < 0) {
      app.note = `${t.name.toLowerCase()} has no loop yet → make one in loops`;
      return;
    }
    const start = barAt(e.clientX, e.currentTarget as Element);
    await act({ type: "add_clip", track: ti, slot, start, bars: null });
    picked = { track: t.id, start };
  }

  function clipDown(e: PointerEvent, index: number) {
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    const c = clips[index];
    drag = { index, x0: e.clientX, start: c.start, from: c.start, moved: false };
    picked = { track: c.track, start: c.start };
  }
  function clipMove(e: PointerEvent) {
    if (!drag) return;
    const barW = lanes.getBoundingClientRect().width / span;
    const next = Math.max(0, drag.from + Math.round((e.clientX - drag.x0) / barW));
    if (next !== drag.start) drag.moved = true;
    drag.start = next;
  }
  async function clipUp() {
    if (!drag) return;
    const d = drag;
    drag = null;
    if (!d.moved || d.start === d.from) return;
    const c = clips[d.index];
    const ti = tracks.findIndex((t) => t.id === c.track);
    await act({ type: "edit_clip", index: d.index, track: ti, start: d.start, bars: c.bars });
    picked = { track: c.track, start: d.start };
  }

  async function resize(by: number) {
    const c = clips[pickedIndex];
    if (!c) return;
    const ti = tracks.findIndex((t) => t.id === c.track);
    await act({ type: "edit_clip", index: pickedIndex, track: ti, start: c.start, bars: Math.max(1, c.bars + by) });
  }
  async function remove() {
    await act({ type: "delete_clip", index: pickedIndex });
    picked = null;
  }
</script>

<div class="song">
  <div class="tools">
    <span class="label">place</span>
    {#each scenes as scene, s}
      <button aria-pressed={app.section === s} onclick={() => (app.section = s)}>{label(s)}</button>
    {/each}
    <button onclick={() => act({ type: "append_scene", scene: app.section })}>add the row to the end</button>
  </div>

  <div class="lanes" bind:this={lanes}>
    <div class="ruler" onpointerdown={seek} role="slider" tabindex="-1" aria-label="start position" aria-valuenow={Math.round(head * span)}>
      {#each Array.from({ length: span / 4 }, (_, i) => i) as i}
        <span class="label" style="left: {((i * 4) / span) * 100}%">{i * 4 + 1}</span>
      {/each}
    </div>
    {#each tracks as t, ti}
      <div
        class="lane"
        style="--id: {trackColor(t)}; background-size: {(4 / span) * 100}% 100%"
        role="button"
        tabindex="-1"
        aria-label="{t.name}: place a loop"
        onpointerdown={(e) => place(e, ti)}
      >
        <span class="name label">{t.name.toLowerCase()}</span>
        {#each clips as c, i}
          {#if c.track === t.id}
            {@const start = drag?.index === i ? drag.start : c.start}
            <button
              class="clip"
              class:picked={pickedIndex === i}
              style="left: {(start / span) * 100}%; width: {(c.bars / span) * 100}%"
              aria-label="{t.name}, bar {c.start + 1}, {c.bars} bars"
              onpointerdown={(e) => clipDown(e, i)}
              onpointermove={clipMove}
              onpointerup={clipUp}
              onpointercancel={() => (drag = null)}
            >
              {label(c.slot)}
            </button>
          {/if}
        {/each}
      </div>
    {/each}
    <span class="head" style="left: {head * 100}%"></span>
  </div>

  <div class="tools">
    {#if pickedIndex >= 0}
      <span class="label">clip</span>
      <button onclick={() => resize(-1)}>shorter</button>
      <button onclick={() => resize(1)}>longer</button>
      <button onclick={remove}>remove</button>
    {:else}
      <span class="label">touch a lane to place a loop · drag a clip to move it</span>
    {/if}
  </div>

  <div class="tools foot">
    <span><span class="label">length</span> {length} bars · {clock(seconds)}</span>
    <span class="grow"></span>
    <button aria-pressed={song?.looping ?? false} onclick={() => act({ type: "set_song_looping", looping: !song?.looping })}>
      loop
    </button>
    <button disabled={!length || !!app.busy} onclick={() => exportSong(true)}>
      {app.busy ? "rendering" : "export audio"}
    </button>
    <button disabled={!length} onclick={() => act({ type: "clear_song" })}>clear</button>
  </div>
</div>

<style>
  .song {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 8px 16px 12px;
    height: 100%;
    overflow-y: auto;
  }
  .tools {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
    min-height: var(--row);
  }
  .tools .label {
    margin-right: 4px;
  }
  .foot {
    gap: 4px 8px;
    margin-top: auto;
  }
  .grow {
    flex: 1;
  }
  .lanes {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .ruler {
    position: relative;
    height: 24px;
    border-bottom: 1px solid var(--line);
    cursor: pointer;
    touch-action: none;
  }
  .ruler .label {
    position: absolute;
    bottom: 4px;
    padding-left: 3px;
    border-left: 1px solid var(--edge);
  }
  .lane {
    position: relative;
    height: 44px;
    border-bottom: 1px solid var(--line);
    /* a tick every four bars, so a touch knows where it lands */
    background-image: linear-gradient(to right, var(--line) 1px, transparent 1px);
    cursor: copy;
    touch-action: none;
  }
  .lane .name {
    position: absolute;
    left: 4px;
    top: 2px;
    color: var(--fg-faint);
    pointer-events: none;
  }
  .clip {
    position: absolute;
    top: 14px;
    bottom: 3px;
    height: auto;
    padding: 0 4px;
    background: var(--id);
    color: #0b0b0a;
    border: 1px solid var(--bg);
    text-align: left;
    font: var(--tick);
    overflow: hidden;
    cursor: grab;
    touch-action: none;
    transition: none;
  }
  .clip.picked {
    border-color: var(--fg);
    outline: 1px solid var(--fg);
  }
  .head {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--fg);
    pointer-events: none;
  }
</style>
