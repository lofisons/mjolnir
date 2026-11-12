<script lang="ts">
  import { onMount } from "svelte";
  import Screen from "./components/Screen.svelte";
  import Loops from "./components/Loops.svelte";
  import Track from "./components/Track.svelte";
  import Song from "./components/Song.svelte";
  import Sound from "./components/Sound.svelte";
  import Setup from "./components/Setup.svelte";
  import { app, act, start, togglePlay, toggleRecord, playing, engine, type Face } from "./lib/app.svelte";

  const FACES: Face[] = ["loops", "song", "set"];
  let wide = $state(false);

  onMount(() => {
    try {
      const t = localStorage.getItem("mjolnir.theme");
      if (t) document.documentElement.dataset.theme = t;
    } catch {
      /* private window */
    }
    const mq = matchMedia("(min-width: 860px)");
    const fit = () => (wide = mq.matches);
    fit();
    mq.addEventListener("change", fit);
    void start();
    const wake = () => engine.wake();
    addEventListener("pointerdown", wake, true);
    addEventListener("keydown", wake, true);
  });

  const scenes = $derived(app.project?.scenes ?? []);
  const recording = $derived((app.project?.record.notes ?? false) && playing());

  // one sentence: what is wrong, else what is happening
  const sentence = $derived.by(() => {
    if (app.note) return app.note;
    if (app.busy) return app.busy;
    if (!app.awake) return "touch anything to start the sound";
    const s = app.status;
    if (s?.count_in) return `count in ${s.count_in}`;
    if (recording) return "recording · play the pads";
    if (s?.song_mode && playing()) return "playing the song";
    if (playing()) return `playing · ${(scenes[app.section]?.name ?? "").toLowerCase()}`;
    if (app.face === "loops" && !app.open) return "touch + to begin";
    return "stopped";
  });

  function go(f: Face) {
    app.face = f;
    app.open = false;
  }

  function key(e: KeyboardEvent) {
    if ((e.target as HTMLElement).tagName === "INPUT") return;
    const mod = e.metaKey || e.ctrlKey;
    if (e.code === "Space") {
      e.preventDefault();
      void togglePlay();
    } else if (mod && e.key.toLowerCase() === "z") {
      e.preventDefault();
      void act({ type: e.shiftKey ? "redo" : "undo" });
    } else if (e.key === "Escape" && app.open) {
      app.open = false;
    } else if (e.key === "Tab" && !mod) {
      e.preventDefault();
      const i = FACES.indexOf(app.face);
      go(FACES[(i + (e.shiftKey ? FACES.length - 1 : 1)) % FACES.length]);
    }
  }
</script>

<svelte:window onkeydown={key} />

{#if app.failed}
  <main class="failed">
    <p>mjolnir could not start: {app.failed}</p>
    <p>→ reload the page; a recent browser is needed (Chrome, Firefox or Safari).</p>
    <p>nothing was lost.</p>
  </main>
{:else if !app.ready}
  <main class="failed"><p>loading the engine</p></main>
{:else}
  <div class="shell" class:wide>
    <header class="bar">
      <span class="app">mjolnir</span>
      <div class="nav" role="tablist">
        {#each FACES as f}
          <button class="plain tab" role="tab" aria-selected={app.face === f} onclick={() => go(f)}>{f}</button>
        {/each}
      </div>
    </header>

    <div class="work">
      <Screen />
      <main class="face">
        {#if app.face === "loops" && app.open}<Track {wide} />
        {:else if app.face === "loops"}<div class="from-left fill"><Loops /></div>
        {:else if app.face === "song"}<div class="from-right fill"><Song /></div>
        {:else}<div class="from-right fill"><Setup /></div>{/if}
      </main>
    </div>

    {#if wide}
      <aside class="panel"><Sound /></aside>
    {/if}

    <footer class="status">
      <span class="sentence" role="status" class:warn={!!app.note}>{sentence}</span>
      <button class="plain" disabled={!app.project?.history.undo} onclick={() => act({ type: "undo" })}>undo</button>
      <button class="lit" aria-pressed={app.project?.record.notes ?? false} onclick={toggleRecord}>
        <span class="led" class:rd={app.project?.record.notes} class:blink={recording}></span>rec
      </button>
      <button class="primary lit" aria-pressed={playing()} onclick={togglePlay}>
        <span class="led" class:gr={playing()}></span>{playing() ? "stop" : "play"}
      </button>
    </footer>
  </div>
{/if}

<style>
  .failed {
    padding: 24px;
    max-width: 60ch;
  }
  .shell {
    height: 100%;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    grid-template-columns: minmax(0, 1fr);
    padding: env(safe-area-inset-top) env(safe-area-inset-right) env(safe-area-inset-bottom) env(safe-area-inset-left);
  }
  .shell.wide {
    grid-template-columns: minmax(0, 1fr) 320px;
  }
  .bar {
    grid-column: 1 / -1;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 16px;
    border-bottom: 1px solid var(--line);
    min-height: var(--row);
  }
  .app {
    font: var(--ui-strong);
  }
  .nav {
    display: flex;
    flex: 1;
    min-width: 0;
  }
  .tab {
    padding: 0 10px;
    color: var(--fg-muted);
  }
  .tab[aria-selected="true"] {
    background: none;
    color: var(--fg);
    border-color: transparent;
    box-shadow: inset 0 -1px 0 var(--fg);
  }
  .work {
    display: grid;
    grid-template-rows: minmax(72px, 13vh) minmax(0, 1fr);
    min-height: 0;
  }
  .face {
    min-height: 0;
    overflow: hidden;
  }
  .fill {
    height: 100%;
  }
  .lit {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
  }
  .panel {
    border-left: 1px solid var(--line);
    min-height: 0;
    overflow: hidden;
  }
  .status {
    grid-column: 1 / -1;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 16px;
    border-top: 1px solid var(--line);
  }
  .sentence {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--fg-muted);
  }
  .sentence.warn {
    color: var(--fg);
  }
  .primary {
    min-width: 72px;
  }
  @media (pointer: coarse) {
    .status button {
      height: 44px;
    }
    .primary {
      min-width: 96px;
    }
  }
</style>
