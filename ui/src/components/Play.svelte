<script lang="ts">
  import { tx } from "../lib/i18n.svelte";
  // The play face: sixteen step keys for the selected pad, and sixteen pads.
  import { app, live, pattern, stepsOn, playhead, toggleStep, padPitch } from "../lib/app.svelte";
  import NoteDetail from "./NoteDetail.svelte";
  import LoopTools from "./LoopTools.svelte";
  import { TICKS_PER_STEP, capture } from "../lib/api";

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

  // a long press (or a right click) on a lit step picks it without switching it off
  let pressing: ReturnType<typeof setTimeout> | null = null;
  let picked = false;
  const pickStep = (i: number) => (app.pick = { start: (app.page * 16 + i) * TICKS_PER_STEP, pitch: padPitch() });
  function stepDown(e: PointerEvent, i: number, lit: boolean) {
    // the right button asks about a step; it never writes one
    if (e.pointerType === "mouse" && e.button !== 0) return void (picked = true);
    picked = false;
    if (!lit) return;
    pressing = setTimeout(() => {
      picked = true;
      pickStep(i);
    }, 320);
  }
  function stepUp(i: number) {
    if (pressing) clearTimeout(pressing);
    pressing = null;
    if (!picked) void toggleStep(app.page * 16 + i);
  }
  const isPicked = (i: number) =>
    app.pick?.pitch === padPitch() && app.pick?.start === (app.page * 16 + i) * TICKS_PER_STEP;

  function down(e: PointerEvent, i: number) {
    capture(e);
    app.pad = i;
    held[i] = true;
    // the higher on the pad, the harder the hit
    const r = (e.currentTarget as Element).getBoundingClientRect();
    const height = 1 - Math.min(1, Math.max(0, (e.clientY - r.top) / r.height));
    live({ type: "pad_on", track: app.track, pad: i, velocity: 0.5 + 0.5 * height });
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

  // while playing, the step keys follow the bar being played
  $effect(() => {
    if (head >= 0 && bars > 1 && follow) app.page = Math.floor(head / 16);
  });
  let follow = $state(true);
</script>

<svelte:window onkeydown={keydown} onkeyup={keyup} />

<div class="play">
  <div class="head top">
    {#if app.pick && pat}
      <NoteDetail named={false} inline />
    {:else}
      <span class="what">{(app.pads[app.pad]?.label ?? "").toLowerCase()}</span>
      <LoopTools />
    {/if}
  </div>
  {#if bars > 1}
    <div class="head">
      <span class="label">{tx("showing bar")}</span>
      {#each Array.from({ length: bars }, (_, i) => i) as b}
        <button
          class="mini"
          class:now={head >= 0 && Math.floor(head / 16) === b}
          aria-pressed={app.page === b}
          onclick={() => {
            app.page = b;
            follow = false;
          }}>{b + 1}</button
        >
      {/each}
      <button class="mini" aria-pressed={follow} onclick={() => (follow = !follow)}>{tx("follow")}</button>
    </div>
  {/if}

  <div class="steps" role="group" aria-label="steps">
    {#each on as lit, i}
      <button
        class="step"
        class:beat={i % 4 === 0}
        class:head={head === app.page * 16 + i}
        class:picked={isPicked(i)}
        aria-pressed={lit}
        aria-label="step {i + 1}"
        onpointerdown={(e) => stepDown(e, i, lit)}
        onpointerup={() => stepUp(i)}
        onpointerleave={() => pressing && clearTimeout(pressing)}
        oncontextmenu={(e) => {
          e.preventDefault();
          if (lit) pickStep(i);
        }}
        onkeydown={(e) => (e.key === "Enter" || e.key === " ") && toggleStep(app.page * 16 + i)}
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
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 6px;
    min-height: 24px;
  }
  /* one height whatever it holds: the steps and the pads never move */
  .head.top {
    flex-wrap: nowrap;
    height: 44px;
  }
  .what {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mini {
    height: 24px;
    min-width: 24px;
    padding: 0 6px;
    border-color: var(--line);
  }
  /* the bar being played, when another is on screen */
  .mini.now {
    box-shadow: inset 0 -2px 0 var(--fg);
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
  /* the step whose detail is on screen */
  .step.picked::after {
    content: "";
    position: absolute;
    right: 4px;
    top: 4px;
    width: 5px;
    height: 5px;
    background: var(--bg);
    outline: 1px solid var(--fg);
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
    /* a hit lights at once and lets go over a moment */
    transition:
      background var(--dur-base) var(--ease),
      color var(--dur-base) var(--ease);
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
    transition: none;
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
