<script lang="ts">
  // The play face: sixteen step keys for the selected pad, and sixteen pads.
  import { app, live, pattern, stepsOn, playhead, toggleStep, act, padPitch, track } from "../lib/app.svelte";
  import { TICKS_PER_STEP } from "../lib/api";

  const on = $derived(stepsOn());
  const head = $derived(playhead());
  const pat = $derived(pattern());
  const bars = $derived(pat?.bars ?? 1);
  let held = $state<Record<number, boolean>>({});

  /** Pads whose sound already has steps in this loop. */
  const used = $derived.by(() => {
    const set = new Set<number>();
    for (const n of pat?.notes ?? []) set.add(n.pitch);
    return set;
  });
  /** What sounds at the playhead, to light the pads the loop is hitting. */
  const sounding = $derived.by(() => {
    const set = new Set<number>();
    if (head < 0 || !pat) return set;
    for (const n of pat.notes) if (Math.round(n.start / TICKS_PER_STEP) === head) set.add(n.pitch);
    return set;
  });

  function down(e: PointerEvent, i: number) {
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    app.pad = i;
    held[i] = true;
    live({ type: "pad_on", track: app.track, pad: i, velocity: 0.85 });
  }
  function up(i: number) {
    if (!held[i]) return;
    held[i] = false;
    live({ type: "pad_off", track: app.track, pad: i });
  }

  // the computer keyboard plays the pads, laid out as they are on screen
  const KEYS = "zxcvasdfqwer1234";
  function keydown(e: KeyboardEvent) {
    if (e.repeat || e.metaKey || e.ctrlKey || e.altKey) return;
    if ((e.target as HTMLElement).tagName === "INPUT") return;
    const i = KEYS.indexOf(e.key.toLowerCase());
    if (i < 0 || i >= app.pads.length) return;
    app.pad = i;
    held[i] = true;
    live({ type: "pad_on", track: app.track, pad: i, velocity: 0.85 });
  }
  function keyup(e: KeyboardEvent) {
    const i = KEYS.indexOf(e.key.toLowerCase());
    if (i >= 0) up(i);
  }

  async function setBars(b: number) {
    if (!pat) await act({ type: "create_pattern", track: app.track, slot: app.section, bars: b });
    else await act({ type: "set_bars", track: app.track, slot: app.section, bars: b });
    if (app.page >= b) app.page = b - 1;
  }

  // while playing, the step keys follow the bar being played
  $effect(() => {
    if (head >= 0 && bars > 1 && follow) app.page = Math.floor(head / 16);
  });
  let follow = $state(true);
</script>

<svelte:window onkeydown={keydown} onkeyup={keyup} />

<div class="play">
  <div class="head">
    <span class="label">steps</span>
    <span class="what">{(app.pads[app.pad]?.label ?? "").toLowerCase()}</span>
    <span class="grow"></span>
    <span class="label">bar</span>
    {#each Array.from({ length: bars }, (_, i) => i) as b}
      <button
        class="mini"
        aria-pressed={app.page === b}
        onclick={() => {
          app.page = b;
          follow = false;
        }}>{b + 1}</button
      >
    {/each}
    <span class="label">of</span>
    {#each [1, 2, 4] as b}
      <button class="mini" aria-pressed={bars === b} onclick={() => setBars(b)}>{b}</button>
    {/each}
  </div>

  <div class="steps" role="group" aria-label="steps">
    {#each on as lit, i}
      <button
        class="step"
        class:beat={i % 4 === 0}
        class:head={head === app.page * 16 + i}
        aria-pressed={lit}
        aria-label="step {i + 1}"
        onclick={() => toggleStep(app.page * 16 + i)}
      >
        <span class="n">{i + 1}</span>
      </button>
    {/each}
  </div>

  <div class="pads" role="group" aria-label="pads">
    {#each app.pads as pad, i}
      <button
        class="pad"
        class:hit={held[i] || sounding.has(pad.pitches[0])}
        class:sel={app.pad === i}
        class:root={pad.root}
        style="grid-row: {4 - Math.floor(i / 4)}; grid-column: {(i % 4) + 1}"
        onpointerdown={(e) => down(e, i)}
        onpointerup={() => up(i)}
        onpointercancel={() => up(i)}
        oncontextmenu={(e) => e.preventDefault()}
      >
        <span class="n">{String(i + 1).padStart(2, "0")}{used.has(pad.pitches[0]) ? " ·" : ""}</span>
        <span class="name">{pad.label.toLowerCase()}</span>
      </button>
    {/each}
  </div>
</div>

<style>
  .play {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 8px 16px 12px;
    height: 100%;
    min-height: 0;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 24px;
  }
  .what {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .grow {
    flex: 1;
  }
  .mini {
    height: 24px;
    min-width: 24px;
    padding: 0 6px;
    border-color: var(--line);
  }
  .steps {
    display: grid;
    grid-template-columns: repeat(8, 1fr);
    gap: 4px;
  }
  .step {
    position: relative;
    height: auto;
    aspect-ratio: 1;
    max-height: 56px;
    width: 100%;
    padding: 0;
    border-color: var(--line);
  }
  .step.beat {
    border-color: var(--edge);
  }
  .step .n {
    position: absolute;
    top: 3px;
    left: 5px;
    font: var(--tick);
    color: var(--fg-muted);
  }
  .step[aria-pressed="true"] .n {
    color: var(--bg);
  }
  /* the step being played: its frame, and a line at the foot when it is off */
  .step.head {
    border-color: var(--fg);
    box-shadow: inset 0 -2px 0 var(--fg);
  }
  .step.head[aria-pressed="true"] {
    box-shadow: inset 0 -2px 0 var(--bg);
  }
  .pads {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    grid-template-rows: repeat(4, 1fr);
    gap: 4px;
    touch-action: none;
  }
  .pad {
    height: auto;
    min-height: 0;
    padding: 8px;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    align-items: flex-start;
    text-align: left;
    overflow: hidden;
    transition: none;
    touch-action: none;
  }
  .pad .n {
    font: var(--tick);
    color: var(--fg-muted);
  }
  .pad .name {
    font: var(--ui-strong);
    white-space: normal;
    overflow: hidden;
    max-height: 36px;
  }
  .pad.root .name::after {
    content: " ·";
  }
  .pad.sel {
    border-color: var(--fg);
  }
  .pad.hit {
    background: var(--fg);
    color: var(--bg);
    border-color: var(--fg);
  }
  .pad.hit .n {
    color: var(--bg);
  }
  @media (min-width: 860px) {
    .play {
      max-width: 760px;
      margin: 0 auto;
    }
    .steps {
      grid-template-columns: repeat(16, 1fr);
    }
  }
</style>
