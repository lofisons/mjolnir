<script lang="ts">
  // What can be done to the loop on screen: its length, and, on demand,
  // copy, paste, snap to the grid, clear.
  import { app, act, pattern, setBars, pasteLoop } from "../lib/app.svelte";

  const pat = $derived(pattern());
  const bars = $derived(pat?.bars ?? 1);
  let more = $state(false);
  const copied = $derived(app.clip?.track === app.track && app.clip?.slot === app.section);
</script>

<span class="group end">
  <span class="label">bars</span>
  {#each [1, 2, 4, 8] as b}
    <button class="mini" aria-pressed={bars === b} onclick={() => setBars(b)}>{b}</button>
  {/each}
  <button class="mini" aria-expanded={more} aria-pressed={more} onclick={() => (more = !more)}>loop</button>
</span>
{#if more}
  <span class="group wide from-right">
    <button class="mini" disabled={!pat} aria-pressed={copied} onclick={() => (app.clip = { track: app.track, slot: app.section })}>
      {copied ? "copied" : "copy"}
    </button>
    <button class="mini" disabled={!app.clip || copied} onclick={pasteLoop}>paste</button>
    <button class="mini" disabled={!pat} onclick={() => act({ type: "quantize", track: app.track, slot: app.section })}>snap</button>
    {#if pat?.automation.length}
      <button class="mini" onclick={() => act({ type: "clear_automation", track: app.track, slot: app.section, param: null })}>
        forget knob moves
      </button>
    {/if}
    <button class="mini" disabled={!pat} onclick={() => act({ type: "delete_pattern", track: app.track, slot: app.section })}>clear</button>
  </span>
{/if}

<style>
  .group {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .group.end {
    margin-left: auto;
  }
  .group.wide {
    flex-basis: 100%;
    justify-content: flex-end;
    flex-wrap: wrap;
  }
  .mini {
    height: 24px;
    min-width: 24px;
    padding: 0 6px;
    border-color: var(--line);
  }
  @media (pointer: coarse) {
    .mini {
      height: 32px;
      min-width: 32px;
    }
  }
</style>
