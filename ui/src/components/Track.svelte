<script lang="ts">
  import { tx } from "../lib/i18n.svelte";
  // Inside a track: its loop for the section on screen, as steps and pads,
  // or its sound. The way back to the loops is always top left.
  import Play from "./Play.svelte";
  import Melodic from "./Melodic.svelte";
  import Sound from "./Sound.svelte";
  import Menu, { type MenuItem } from "./Menu.svelte";
  import { app, selectTrack, selectSection, trackColor, track } from "../lib/app.svelte";

  let { wide = false }: { wide?: boolean } = $props();
  const tracks = $derived(app.project?.tracks ?? []);
  const scenes = $derived(app.project?.scenes ?? []);
  const letter = (i: number) => {
    const n = scenes[i]?.name ?? "";
    return n && n.length <= 3 ? n.toLowerCase() : String(i + 1);
  };

  // the section on screen is one word; the others are a touch away
  let menu = $state<{ x: number; y: number; title: string; items: MenuItem[] } | null>(null);
  function sections(e: MouseEvent) {
    const b = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const t = track();
    menu = {
      x: b.right - 168,
      y: b.bottom + 4,
      title: tx("section"),
      items: scenes.map((sc, i) => ({
        // a dot marks the sections where this track already has a loop
        label: letter(i),
        note: t?.slots[i] ? tx("loop") : undefined,
        on: i === app.section,
        pick: () => void selectSection(i),
      })),
    };
  }
</script>

<div class="track from-right">
  <!-- one line: the way back, the tracks, the section, and the track's sound -->
  <div class="head">
    <button class="back" aria-label="back to the loops" onclick={() => (app.open = false)}>←<span class="word">loops</span></button>
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
    <button class="sec" aria-haspopup="menu" aria-label="section {scenes[app.section]?.name ?? ''}" onclick={sections}>
      <span class="label">{tx("section")}</span>{letter(app.section)}
    </button>
    {#if !wide}
      <button class="pane" role="tab" aria-selected={app.pane === "sound"} class:on={app.pane === "sound"} onclick={() => (app.pane = app.pane === "sound" ? "notes" : "sound")}>
        {tx("sound")}
      </button>
    {/if}
  </div>
  <div class="body">
    {#if app.pane === "sound" && !wide}<Sound />
    {:else if track()?.kind === "melodic"}{#key app.track}<Melodic />{/key}
    {:else}<Play />{/if}
  </div>
</div>
{#if menu}<Menu {...menu} onclose={() => (menu = null)} />{/if}

<style>
  .track {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    grid-template-columns: minmax(0, 1fr);
    height: 100%;
  }
  .head {
    display: flex;
    align-items: stretch;
    border-bottom: 1px solid var(--line);
    min-height: 40px;
    min-width: 0;
  }
  .back {
    flex: none;
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
  /* on a phone the other tracks are their colours; the one on screen has its name */
  @media (max-width: 520px) {
    .tab:not(.narrow):not([aria-selected="true"]) {
      flex: 0 0 34px;
      justify-content: center;
      padding: 0;
    }
    .tab:not(.narrow):not([aria-selected="true"]) .tname {
      display: none;
    }
  }
  .back .word {
    margin-left: 6px;
  }
  .sec,
  .pane {
    flex: none;
    height: auto;
    border: 0;
    border-left: 1px solid var(--line);
    padding: 0 12px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .pane.on {
    background: var(--fg);
    color: var(--bg);
  }
  .sec .label {
    color: inherit;
    opacity: 0.62;
  }
  @media (max-width: 520px) {
    .back {
      padding: 0 12px;
    }
    .back .word,
    .sec .label {
      display: none;
    }
    .sec {
      min-width: 44px;
      justify-content: center;
    }
  }
</style>
