<script lang="ts">
  // The song face: sections laid end to end on a small timeline.
  import { app, act, exportSong, playing } from "../lib/app.svelte";

  const tracks = $derived(app.project?.tracks ?? []);
  const scenes = $derived(app.project?.scenes ?? []);
  const song = $derived(app.project?.song);
  const length = $derived(Math.max(0, ...(song?.clips ?? []).map((c) => c.start + c.bars)));
  const span = $derived(Math.max(16, Math.ceil(length / 4) * 4));
  const beats = $derived(app.project?.meter?.beats ?? 4);
  const head = $derived.by(() => {
    const s = app.status;
    if (!s || !s.song_mode || !s.playing) return (app.project?.view.start_bar ?? 0) / span;
    return (s.bar + s.beat / beats) / span;
  });
  const seconds = $derived(length * beats * (60 / (app.project?.bpm ?? 120)));
  const clock = (t: number) => `${Math.floor(t / 60)}:${String(Math.floor(t % 60)).padStart(2, "0")}`;

  function seek(e: PointerEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const bar = Math.max(0, Math.min(span - 1, Math.floor(((e.clientX - r.left) / r.width) * span)));
    void act({ type: "seek", bar });
  }
</script>

<div class="song">
  <div class="readout">
    <span><span class="label">length</span> {length} bars</span>
    <span><span class="label">time</span> {clock(seconds)}</span>
    <span class="grow"></span>
    <button aria-pressed={song?.looping ?? false} onclick={() => act({ type: "set_song_looping", looping: !song?.looping })}>
      loop
    </button>
  </div>

  <div class="lanes">
    <div class="ruler" onpointerdown={seek} role="slider" tabindex="-1" aria-label="position" aria-valuenow={Math.round(head * span)}>
      {#each Array.from({ length: span / 4 }, (_, i) => i) as i}
        <span class="label" style="left: {((i * 4) / span) * 100}%">{i * 4 + 1}</span>
      {/each}
    </div>
    {#each tracks as t}
      <div class="lane">
        <span class="name label">{t.name.toLowerCase()}</span>
        {#each song?.clips ?? [] as c, i}
          {#if c.track === t.id}
            <button
              class="clip"
              style="left: {(c.start / span) * 100}%; width: {(c.bars / span) * 100}%"
              aria-label="{t.name}, bar {c.start + 1}: remove"
              onclick={() => act({ type: "delete_clip", index: i })}
            >
              {c.slot + 1}
            </button>
          {/if}
        {/each}
      </div>
    {/each}
    <span class="head" style="left: {head * 100}%"></span>
  </div>

  <div>
    <div class="label caption">add to the end</div>
    <div class="tools">
      {#each scenes as scene, s}
        <button onclick={() => act({ type: "append_scene", scene: s })}>{s + 1} {scene.name.toLowerCase()}</button>
      {/each}
    </div>
  </div>
  <div class="tools">
    <button disabled={!length || !!app.busy} onclick={() => exportSong(false)}>
      {app.busy ? "rendering" : "export wav"}
    </button>
    <button disabled={!length || !!app.busy} onclick={() => exportSong(true)}>export wav and midi</button>
    <span class="grow"></span>
    <button disabled={!length} onclick={() => act({ type: "clear_song" })}>clear song</button>
  </div>
</div>

<style>
  .song {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 8px 16px 12px;
    height: 100%;
    overflow-y: auto;
  }
  .readout,
  .tools {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    align-items: center;
  }
  .tools {
    gap: 4px;
  }
  .grow {
    flex: 1;
  }
  .caption {
    margin-bottom: 6px;
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
    height: 32px;
    border-bottom: 1px solid var(--line);
  }
  .lane .name {
    position: absolute;
    left: 3px;
    top: 2px;
    color: var(--fg-faint);
    pointer-events: none;
  }
  .clip {
    position: absolute;
    top: 3px;
    bottom: 3px;
    height: auto;
    padding: 0 4px;
    background: var(--fg);
    color: var(--bg);
    border-color: var(--bg);
    text-align: left;
    font: var(--tick);
    overflow: hidden;
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
