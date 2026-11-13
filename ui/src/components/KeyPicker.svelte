<script lang="ts">
  // The set's key: a root and a scale. The roll, the pads and the chords all
  // follow it, so it is worth having within reach.
  import { app, act, refreshPads } from "../lib/app.svelte";
  import { NOTE_NAMES } from "../lib/api";

  const scale = $derived(app.project?.scale ?? { root: 0, kind: "minor" });
  const kinds = $derived(app.info?.catalog.scales ?? []);
  async function set(root: number, kind: string) {
    await act({ type: "set_scale", root, kind, track: null });
    await refreshPads();
  }
</script>

<div class="key">
  <div class="roots" role="group" aria-label="root">
    {#each NOTE_NAMES as n, i}
      <button aria-pressed={scale.root === i} onclick={() => set(i, scale.kind)}>{n}</button>
    {/each}
  </div>
  <div class="kinds" role="group" aria-label="scale">
    {#each kinds as k}
      <button aria-pressed={scale.kind === k.id} onclick={() => set(scale.root, k.id)}>{k.label.toLowerCase()}</button>
    {/each}
  </div>
</div>

<style>
  .key {
    display: grid;
    gap: 8px;
  }
  .roots {
    display: grid;
    grid-template-columns: repeat(6, minmax(0, 1fr));
    gap: 4px;
  }
  .kinds {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  button {
    height: 32px;
    padding: 0 8px;
    min-width: 0;
    border-color: var(--line);
  }
  @media (pointer: coarse) {
    button {
      height: 40px;
    }
  }
</style>
