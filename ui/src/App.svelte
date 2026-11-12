<script lang="ts">
  import { onMount } from "svelte";
  import Screen from "./components/Screen.svelte";
  import Play from "./components/Play.svelte";
  import Clips from "./components/Clips.svelte";
  import Song from "./components/Song.svelte";
  import Sound from "./components/Sound.svelte";
  import Setup from "./components/Setup.svelte";
  import {
    app,
    act,
    start,
    selectTrack,
    selectSection,
    togglePlay,
    toggleRecord,
    playing,
    engine,
    type Face,
  } from "./lib/app.svelte";

  const FACES: Face[] = ["play", "clips", "song", "sound", "set"];
  let wide = $state(false);

  onMount(() => {
    try {
      const t = localStorage.getItem("mjolnir.theme");
      if (t) document.documentElement.dataset.theme = t;
    } catch {
      /* private window */
    }
    const mq = matchMedia("(min-width: 860px)");
    const fit = () => {
      wide = mq.matches;
      // on a wide window the sound sits in the panel, always there
      if (wide && app.face === "sound") app.face = "play";
    };
    fit();
    mq.addEventListener("change", fit);
    void start();
    const wake = () => engine.wake();
    addEventListener("pointerdown", wake, true);
    addEventListener("keydown", wake, true);
  });

  const tracks = $derived(app.project?.tracks ?? []);
  const scenes = $derived(app.project?.scenes ?? []);
  const faces = $derived(wide ? FACES.filter((f) => f !== "sound") : FACES);

  // one sentence: what is wrong, else what is happening
  const sentence = $derived.by(() => {
    if (app.note) return app.note;
    if (app.busy) return app.busy;
    if (!app.awake) return "touch anything to start the sound";
    const s = app.status;
    if (s?.count_in) return `count in ${s.count_in}`;
    if (app.project?.record.notes && playing()) return "recording · play the pads";
    if (s?.song_mode && playing()) return "playing the song";
    if (playing()) return `playing · ${(scenes[app.section]?.name ?? "").toLowerCase()}`;
    return "stopped";
  });

  function key(e: KeyboardEvent) {
    if ((e.target as HTMLElement).tagName === "INPUT") return;
    const mod = e.metaKey || e.ctrlKey;
    if (e.code === "Space") {
      e.preventDefault();
      void togglePlay();
    } else if (mod && e.key.toLowerCase() === "z") {
      e.preventDefault();
      void act({ type: e.shiftKey ? "redo" : "undo" });
    } else if (e.key === "Tab" && !mod) {
      e.preventDefault();
      const i = faces.indexOf(app.face);
      app.face = faces[(i + (e.shiftKey ? faces.length - 1 : 1)) % faces.length];
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
        {#each faces as f}
          <button class="plain tab" role="tab" aria-selected={app.face === f} onclick={() => (app.face = f)}>{f}</button>
        {/each}
      </div>
    </header>

    <div class="work">
      <Screen />

      <div class="strip" role="tablist" aria-label="tracks">
        {#each tracks as t, i}
          <button class="plain track" role="tab" aria-selected={app.track === i} class:muted={t.mute} onclick={() => selectTrack(i)}>
            <span class="label">{i + 1}</span>
            <span class="tname">{t.name.toLowerCase()}</span>
            <span class="meter" style="transform: scaleX({Math.min(1, (app.status?.tracks[i]?.level ?? 0) * 2.5)})"></span>
          </button>
        {/each}
      </div>

      {#if app.face === "play" || app.face === "sound"}
        <div class="strip sections" role="tablist" aria-label="sections">
          {#each scenes as s, i}
            <button class="plain track" role="tab" aria-selected={app.section === i} onclick={() => selectSection(i)}>
              <span class="label">{i + 1}</span>
              <span class="tname">{s.name.toLowerCase()}</span>
            </button>
          {/each}
        </div>
      {/if}

      <main class="face">
        {#if app.face === "play"}<Play />
        {:else if app.face === "clips"}<Clips />
        {:else if app.face === "song"}<Song />
        {:else if app.face === "sound"}<Sound />
        {:else}<Setup />{/if}
      </main>
    </div>

    {#if wide}
      <aside class="panel"><Sound /></aside>
    {/if}

    <footer class="status">
      <span class="sentence" role="status" class:warn={!!app.note}>{sentence}</span>
      <button class="plain" disabled={!app.project?.history.undo} onclick={() => act({ type: "undo" })}>undo</button>
      <button class:rec={app.project?.record.notes} aria-pressed={app.project?.record.notes ?? false} onclick={toggleRecord}>
        rec
      </button>
      <button class="primary" aria-pressed={playing()} onclick={togglePlay}>{playing() ? "stop" : "play"}</button>
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
    grid-template-rows: minmax(72px, 14vh) auto auto minmax(0, 1fr);
    min-height: 0;
  }
  .strip {
    display: flex;
    border-bottom: 1px solid var(--line);
    min-height: var(--row);
    overflow: hidden;
  }
  .track {
    position: relative;
    flex: 1 1 0;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px;
    border-right: 1px solid var(--line);
  }
  .track:first-child {
    padding-left: 16px;
    text-align: left;
  }
  .sections .track {
    flex: 0 1 96px;
  }
  .track .tname {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .track.muted .tname {
    color: var(--fg-faint);
  }
  .track[aria-selected="true"] .label {
    color: var(--bg);
  }
  .track[aria-selected="true"].muted .tname {
    color: var(--bg);
  }
  .meter {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 2px;
    background: var(--fg);
    transform-origin: left;
    transform: scaleX(0);
  }
  .track[aria-selected="true"] .meter {
    background: var(--bg);
  }
  .face {
    min-height: 0;
    grid-row: 4;
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
