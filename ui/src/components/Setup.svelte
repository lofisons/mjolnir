<script lang="ts">
  // The set face: the project (name, tempo, key), the master's colour, files.
  import Knob from "./Knob.svelte";
  import { app, act, live, saveFile, openFile, replaceProject, refreshPads, engine } from "../lib/app.svelte";
  import { NOTE_NAMES } from "../lib/api";

  const p = $derived(app.project);
  const kinds = $derived(app.info?.catalog.scales ?? []);
  let theme = $state(document.documentElement.dataset.theme === "paper" ? "paper" : "dark");

  const BPM = [40, 200];
  const bpmNorm = (b: number) => (b - BPM[0]) / (BPM[1] - BPM[0]);

  async function scale(root: number, kind: string) {
    await act({ type: "set_scale", root, kind, track: null });
    await refreshPads();
  }
  function master(key: "volume" | "tape" | "vinyl" | "warmth", v: number) {
    if (!p) return;
    p.master[key] = v;
    live({ type: "set_master", master: { ...p.master } });
  }
  function setTheme(t: string) {
    theme = t;
    document.documentElement.dataset.theme = t;
    try {
      localStorage.setItem("mjolnir.theme", t);
    } catch {
      /* private window */
    }
  }
</script>

{#if p}
  <div class="set">
    <section>
      <div class="group">project</div>
      <div class="line">
        <input
          type="text"
          aria-label="name"
          value={p.name}
          onchange={(e) => act({ type: "set_name", name: e.currentTarget.value })}
        />
      </div>
      <div class="knobs">
        <Knob
          value={bpmNorm(p.bpm)}
          label="tempo"
          text="{Math.round(p.bpm)} bpm"
          reset={bpmNorm(84)}
          onstart={() => void act({ type: "checkpoint" })}
          onchange={(v) => {
            const bpm = Math.round(BPM[0] + v * (BPM[1] - BPM[0]));
            p.bpm = bpm;
            live({ type: "set_bpm", bpm });
          }}
        />
        <Knob
          value={p.swing}
          label="swing"
          reset={0}
          onstart={() => void act({ type: "checkpoint" })}
          onchange={(v) => {
            p.swing = v;
            live({ type: "set_swing", swing: v });
          }}
        />
        <div class="tempo">
          <button aria-label="slower" onclick={() => act({ type: "set_bpm", bpm: p.bpm - 1 })}>−</button>
          <span class="big">{Math.round(p.bpm)}</span>
          <button aria-label="faster" onclick={() => act({ type: "set_bpm", bpm: p.bpm + 1 })}>+</button>
        </div>
      </div>
      <div class="line">
        <button
          aria-pressed={app.status?.metronome ?? false}
          onclick={() => live({ type: "set_metronome", on: !app.status?.metronome })}>metronome</button
        >
      </div>
    </section>

    <section>
      <div class="group">key</div>
      <div class="roots">
        {#each NOTE_NAMES as n, i}
          <button aria-pressed={p.scale.root === i} onclick={() => scale(i, p.scale.kind)}>{n}</button>
        {/each}
      </div>
      <div class="line wrap">
        {#each kinds as k}
          <button aria-pressed={p.scale.kind === k.id} onclick={() => scale(p.scale.root, k.id)}>
            {k.label.toLowerCase()}
          </button>
        {/each}
      </div>
    </section>

    <section>
      <div class="group">master</div>
      <div class="knobs">
        <Knob value={p.master.volume} label="volume" reset={0.8} onchange={(v) => master("volume", v)} />
        <Knob value={p.master.tape} label="tape" reset={0} onchange={(v) => master("tape", v)} />
        <Knob value={p.master.vinyl} label="vinyl" reset={0} onchange={(v) => master("vinyl", v)} />
        <Knob value={p.master.warmth} label="warmth" reset={0} onchange={(v) => master("warmth", v)} />
      </div>
    </section>

    <section>
      <div class="group">files</div>
      <div class="line wrap">
        <button onclick={saveFile}>save file</button>
        <button onclick={openFile}>open file</button>
        <button onclick={() => replaceProject("new_project")}>new</button>
        <button onclick={() => replaceProject("open_demo")}>open the demo</button>
      </div>
    </section>

    <section>
      <div class="group">this device</div>
      <div class="line wrap">
        <button aria-pressed={theme === "dark"} onclick={() => setTheme("dark")}>dark</button>
        <button aria-pressed={theme === "paper"} onclick={() => setTheme("paper")}>paper</button>
        <span class="label">output</span>
        <span>{Math.round(engine.sampleRate() / 100) / 10} kHz · {engine.latencyMs()} ms</span>
      </div>
    </section>
  </div>
{/if}

<style>
  .set {
    display: flex;
    flex-direction: column;
    gap: 20px;
    padding: 8px 12px 16px;
    height: 100%;
    overflow-y: auto;
  }
  .group {
    font: var(--ui-strong);
    padding-bottom: 4px;
    margin-bottom: 12px;
    border-bottom: 1px solid var(--line);
  }
  .line {
    display: flex;
    gap: 4px 8px;
    align-items: center;
    margin-top: 8px;
  }
  .line.wrap {
    flex-wrap: wrap;
  }
  .line input {
    flex: 1;
  }
  .knobs {
    display: flex;
    flex-wrap: wrap;
    gap: 12px 20px;
    align-items: center;
  }
  .tempo {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-left: auto;
  }
  .big {
    font: var(--title);
    min-width: 3ch;
    text-align: center;
  }
  .roots {
    display: grid;
    grid-template-columns: repeat(12, 1fr);
    gap: 2px;
  }
  .roots button {
    padding: 0;
  }
  @media (max-width: 520px) {
    .roots {
      grid-template-columns: repeat(6, 1fr);
    }
  }
</style>
