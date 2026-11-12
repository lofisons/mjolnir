<script lang="ts">
  // Inside a track: its loop for the section on screen, as steps and pads,
  // or its sound. The way back to the loops is always top left.
  import Play from "./Play.svelte";
  import Sound from "./Sound.svelte";
  import { app, selectTrack, selectSection, trackColor } from "../lib/app.svelte";

  let { wide = false }: { wide?: boolean } = $props();
  const tracks = $derived(app.project?.tracks ?? []);
  const scenes = $derived(app.project?.scenes ?? []);
</script>

<div class="track from-right">
  <div class="head">
    <button class="back" aria-label="back to the loops" onclick={() => (app.open = false)}>← loops</button>
    <div class="tabs" role="tablist" aria-label="tracks">
      {#each tracks as t, i}
        <button
          class="plain tab"
          role="tab"
          aria-selected={app.track === i}
          style="--id: {trackColor(t)}"
          onclick={() => selectTrack(i)}
        >
          <span class="chip"></span>
          <span class="tname">{t.name.toLowerCase()}</span>
        </button>
      {/each}
    </div>
  </div>
  <div class="head">
    <div class="tabs" role="tablist" aria-label="sections">
      {#each scenes as s, i}
        <button class="plain tab narrow" role="tab" aria-selected={app.section === i} onclick={() => selectSection(i)}>
          {s.name.length <= 3 ? s.name.toLowerCase() : i + 1}
        </button>
      {/each}
    </div>
    {#if !wide}
      <div class="panes" role="tablist" aria-label="view">
        <button role="tab" aria-selected={app.pane === "notes"} onclick={() => (app.pane = "notes")}>notes</button>
        <button role="tab" aria-selected={app.pane === "sound"} onclick={() => (app.pane = "sound")}>sound</button>
      </div>
    {/if}
  </div>
  <div class="body">
    {#if app.pane === "sound" && !wide}<Sound />{:else}<Play />{/if}
  </div>
</div>

<style>
  .track {
    display: grid;
    grid-template-rows: auto auto minmax(0, 1fr);
    height: 100%;
  }
  .head {
    display: flex;
    align-items: stretch;
    border-bottom: 1px solid var(--line);
    min-height: var(--row);
  }
  .back {
    border: 0;
    border-right: 1px solid var(--line);
    padding: 0 12px 0 16px;
    height: auto;
  }
  .tabs {
    display: flex;
    flex: 1;
    min-width: 0;
  }
  .tab {
    flex: 1 1 0;
    min-width: 0;
    height: auto;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px;
    border-right: 1px solid var(--line);
    color: var(--fg-muted);
  }
  .tab.narrow {
    flex: 0 1 48px;
    justify-content: center;
  }
  /* the selected track wears its colour as a line, not as a fill: the fill
     belongs to what is switched on */
  .tab[aria-selected="true"] {
    background: none;
    color: var(--fg);
    border-color: transparent;
    border-right-color: var(--line);
    box-shadow: inset 0 -2px 0 var(--id, var(--fg));
  }
  .chip {
    width: 6px;
    height: 6px;
    flex: none;
    background: var(--id);
  }
  .tname {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .panes {
    display: flex;
    padding: 0 16px 0 8px;
    align-items: center;
  }
  .panes button {
    height: calc(var(--row) - 8px);
    min-height: 24px;
  }
  .panes button + button {
    border-left: 0;
  }
  .body {
    min-height: 0;
  }
</style>
