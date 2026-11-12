<script lang="ts">
  import { onMount } from "svelte";
  import Screen from "./components/Screen.svelte";
  import Loops from "./components/Loops.svelte";
  import Track from "./components/Track.svelte";
  import Song from "./components/Song.svelte";
  import Sound from "./components/Sound.svelte";
  import Setup from "./components/Setup.svelte";
  import Mix from "./components/Mix.svelte";
  import Punch from "./components/Punch.svelte";
  import Help from "./components/Help.svelte";
  import Export from "./components/Export.svelte";
  import {
    app,
    act,
    start,
    togglePlay,
    toggleRecord,
    playing,
    engine,
    openText,
    openFile,
    saveFile,
    newSet,
    addSample,
    type Face,
  } from "./lib/app.svelte";

  const FACES: Face[] = ["loops", "song", "mix", "set"];
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

    // a set handed over by the system: a desktop app's "open with", an
    // installed web app's file handler, or a file dropped on the window
    addEventListener("mjolnir-open", (e) => void openText((e as CustomEvent<string>).detail));
    const queue = (window as { launchQueue?: { setConsumer: (f: (p: { files: { getFile: () => Promise<File> }[] }) => void) => void } })
      .launchQueue;
    queue?.setConsumer(async (p) => {
      const file = await p.files[0]?.getFile();
      if (file) void openText(await file.text());
    });
    addEventListener("dragover", (e) => e.preventDefault());
    addEventListener("drop", async (e) => {
      e.preventDefault();
      const file = e.dataTransfer?.files?.[0];
      if (!file) return;
      if (/\.(mjolnir|yggdrasil|tinydaw|json)$/i.test(file.name)) return void openText(await file.text());
      // anything else is taken for sound: it becomes a new track
      try {
        const audio = await engine.decode(await file.arrayBuffer());
        await addSample(audio, "melodic", file.name.replace(/\.[^.]+$/, "").slice(0, 24));
      } catch {
        app.note = "that file is neither a set nor a sound";
      }
    });
    // the Android app: its back button, and leaving it
    const host = window as { mjolnirBack?: () => boolean; mjolnirPause?: () => void };
    host.mjolnirBack = () => {
      if (app.help) app.help = false;
      else if (app.open) app.open = false;
      else if (app.face !== "loops") app.face = "loops";
      else return false;
      return true;
    };
    host.mjolnirPause = () => {
      if (playing()) void act({ type: "stop" });
    };
    // the menu of a desktop app
    addEventListener("mjolnir-menu", (e) => {
      const what = (e as CustomEvent<string>).detail;
      if (what === "new") void newSet();
      else if (what === "open") void openFile();
      else if (what === "save") void saveFile();
      else if (what === "export") app.exporting = true;
      else if (what === "undo" || what === "redo") void act({ type: what });
      else if (what === "help") app.help = true;
    });
  });

  const scenes = $derived(app.project?.scenes ?? []);
  const recording = $derived((app.project?.record.notes ?? false) && playing());

  // one sentence: what is wrong, else what is happening
  const sentence = $derived.by(() => {
    if (app.note) return app.note;
    if (app.busy) return app.busy;
    if (!app.awake) return "touch to wake the sound";
    const s = app.status;
    if (s?.count_in) return `count in ${s.count_in}`;
    if (recording) return "recording · play the pads";
    if (s?.song_mode && playing()) return "playing the song";
    if (playing()) return `playing · ${(scenes[app.section]?.name ?? "").toLowerCase()}`;
    if (app.face === "loops" && !app.open)
      return app.project?.tracks.some((t) => t.slots.some(Boolean)) ? "click plays a loop · double click opens it" : "touch + to begin";
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
    } else if (e.key === "?") {
      app.help = !app.help;
    } else if (e.key === "Escape" && (app.help || app.exporting)) {
      app.help = false;
      app.exporting = false;
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
        {:else if app.face === "mix"}<div class="from-right fill"><Mix /></div>
        {:else}<div class="from-right fill"><Setup /></div>{/if}
      </main>
    </div>

    {#if wide}
      <aside class="panel"><Sound /></aside>
    {/if}

    {#if app.punch}<div class="punchbar"><Punch /></div>{/if}

    <footer class="status">
      <span class="sentence" role="status" class:warn={!!app.note}>{sentence}</span>
      <button class="plain slim" disabled={!app.project?.history.undo} onclick={() => act({ type: "undo" })}>undo</button>
      <button class="plain slim" disabled={!app.project?.history.redo} onclick={() => act({ type: "redo" })}>redo</button>
      <button aria-pressed={app.punch} onclick={() => (app.punch = !app.punch)}>fx</button>
      <button class="lit" aria-pressed={app.project?.record.notes ?? false} onclick={toggleRecord}>
        <span class="led" class:rd={app.project?.record.notes} class:blink={recording}></span>rec
      </button>
      <button class="primary lit" aria-pressed={playing()} onclick={togglePlay}>
        <span class="led" class:gr={playing()}></span>{playing() ? "stop" : "play"}
      </button>
    </footer>
  </div>
{/if}
{#if app.help}<Help />{/if}
{#if app.exporting}<Export />{/if}

<style>
  .failed {
    padding: 24px;
    max-width: 60ch;
  }
  .shell {
    height: 100%;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto auto;
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
  .punchbar {
    grid-column: 1 / -1;
    grid-row: 3;
    min-width: 0;
    overflow: hidden;
  }
  .status {
    grid-column: 1 / -1;
    grid-row: 4;
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
  .slim {
    padding: 0 6px;
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
