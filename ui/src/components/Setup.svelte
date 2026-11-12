<script lang="ts">
  // The set face: the sets kept in this browser, this set's name, tempo and
  // key, how recording behaves, and this device.
  import Knob from "./Knob.svelte";
  import {
    app,
    act,
    live,
    saveFile,
    openFile,
    openSet,
    newSet,
    duplicateSet,
    removeSet,
    openDemo,
    refreshPads,
    engine,
  } from "../lib/app.svelte";
  import { NOTE_NAMES } from "../lib/api";

  const p = $derived(app.project);
  const kinds = $derived(app.info?.catalog.scales ?? []);
  let theme = $state(document.documentElement.dataset.theme === "paper" ? "paper" : "dark");
  let removing = $state<string | null>(null);
  let countIn = $state(0);

  const BPM = [40, 200];
  const bpmNorm = (b: number) => (b - BPM[0]) / (BPM[1] - BPM[0]);
  const DEMOS: [string, string][] = [
    ["shadow-walk.mjolnir", "shadow walk"],
    ["fog.mjolnir", "fog"],
    ["signal.mjolnir", "signal"],
  ];

  async function scale(root: number, kind: string) {
    await act({ type: "set_scale", root, kind, track: null });
    await refreshPads();
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
  function when(ms: number) {
    const d = (Date.now() - ms) / 1000;
    if (d < 90) return "now";
    if (d < 3600) return `${Math.round(d / 60)} min ago`;
    if (d < 86400) return `${Math.round(d / 3600)} h ago`;
    return `${Math.round(d / 86400)} d ago`;
  }
  async function record(patch: { quantize?: boolean }) {
    if (!p) return;
    const r = { ...p.record, ...patch };
    await act({ type: "set_record", notes: r.notes, automation: r.automation, quantize: r.quantize, song: r.song });
  }
  // tap tempo: the pace of the last few taps
  let taps: number[] = [];
  function tap() {
    const now = performance.now();
    if (taps.length && now - taps[taps.length - 1] > 2000) taps = [];
    taps.push(now);
    if (taps.length > 5) taps.shift();
    if (taps.length < 2) return;
    const beat = (taps[taps.length - 1] - taps[0]) / (taps.length - 1);
    void act({ type: "set_bpm", bpm: Math.round(Math.min(200, Math.max(40, 60000 / beat))) });
  }
  async function setCountIn(bars: number) {
    countIn = bars;
    await act({ type: "set_count_in", bars });
  }
</script>

{#if p}
  <div class="set">
    <section>
      <div class="group">sets</div>
      <div class="sets" role="list">
        {#each app.sets as s (s.id)}
          <div class="row" class:open={s.id === app.set} role="listitem">
            <button class="plain pick" aria-current={s.id === app.set} onclick={() => openSet(s.id)}>
              <span class="sname">{s.name.toLowerCase()}</span>
              <span class="label">{s.bpm} bpm · {s.tracks} tracks · {when(s.updated)}</span>
            </button>
            {#if removing === s.id}
              <button
                onclick={() => {
                  removing = null;
                  void removeSet(s.id);
                }}>remove for good</button
              >
              <button onclick={() => (removing = null)}>keep</button>
            {:else}
              <button aria-label="copy {s.name}" onclick={() => duplicateSet(s.id)}>copy</button>
              <button aria-label="remove {s.name}" onclick={() => (removing = s.id)}>remove</button>
            {/if}
          </div>
        {/each}
      </div>
      <div class="line wrap">
        <button onclick={newSet}>new set</button>
        <button onclick={openFile}>open a file</button>
        <button onclick={saveFile}>save this set to a file</button>
      </div>
      <div class="line wrap">
        <span class="label">listen to a demo</span>
        {#each DEMOS as [file, name]}
          <button onclick={() => openDemo(file)}>{name}</button>
        {/each}
      </div>
    </section>

    <section>
      <div class="group">this set</div>
      <div class="line">
        <input type="text" aria-label="name" value={p.name} onchange={(e) => act({ type: "set_name", name: e.currentTarget.value })} />
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
          <button onclick={tap}>tap</button>
          <button aria-label="slower" onclick={() => act({ type: "set_bpm", bpm: p.bpm - 1 })}>−</button>
          <span class="big">{Math.round(p.bpm)}</span>
          <button aria-label="faster" onclick={() => act({ type: "set_bpm", bpm: p.bpm + 1 })}>+</button>
        </div>
      </div>
      <div class="roots">
        {#each NOTE_NAMES as n, i}
          <button aria-pressed={p.scale.root === i} onclick={() => scale(i, p.scale.kind)}>{n}</button>
        {/each}
      </div>
      <div class="line wrap">
        {#each kinds as k}
          <button aria-pressed={p.scale.kind === k.id} onclick={() => scale(p.scale.root, k.id)}>{k.label.toLowerCase()}</button>
        {/each}
      </div>
    </section>

    <section>
      <div class="group">recording</div>
      <div class="line wrap">
        <button aria-pressed={app.status?.metronome ?? false} onclick={() => live({ type: "set_metronome", on: !app.status?.metronome })}>
          metronome
        </button>
        <button aria-pressed={p.record.quantize} onclick={() => record({ quantize: !p.record.quantize })}>snap to the grid</button>
        <span class="label">count in</span>
        {#each [0, 1, 2] as b}
          <button aria-pressed={countIn === b} onclick={() => setCountIn(b)}>{b === 0 ? "none" : `${b} bar${b > 1 ? "s" : ""}`}</button>
        {/each}
      </div>
    </section>

    <section>
      <div class="group">this device</div>
      <div class="line wrap">
        <button aria-pressed={theme === "dark"} onclick={() => setTheme("dark")}>dark</button>
        <button aria-pressed={theme === "paper"} onclick={() => setTheme("paper")}>paper</button>
        <button onclick={() => (app.help = true)}>how to play</button>
      </div>
      <div class="line wrap readings">
        <span><span class="label">output</span> {Math.round(engine.sampleRate() / 100) / 10} kHz · {engine.latencyMs()} ms</span>
        <span><span class="label">midi</span> {app.midi.length ? app.midi.join(", ").toLowerCase() : "no controller"}</span>
      </div>
    </section>
  </div>
{/if}

<style>
  .set {
    display: flex;
    flex-direction: column;
    gap: 20px;
    padding: 8px 16px 16px;
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
  .readings {
    gap: 4px 20px;
  }
  .sets {
    display: flex;
    flex-direction: column;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 4px;
    border-bottom: 1px solid var(--line);
    min-height: 44px;
  }
  .row.open {
    box-shadow: inset 2px 0 0 var(--fg);
  }
  .pick {
    flex: 1;
    min-width: 0;
    height: auto;
    padding: 4px 8px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    text-align: left;
  }
  .sname {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .row.open .sname {
    font: var(--ui-strong);
  }
  .knobs {
    display: flex;
    flex-wrap: wrap;
    gap: 12px 20px;
    align-items: center;
    margin-top: 12px;
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
    margin-top: 12px;
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
