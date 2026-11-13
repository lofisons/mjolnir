<script lang="ts">
  import { tx } from "../lib/i18n.svelte";
  // The play face: sixteen step keys for the selected pad, and sixteen pads.
  // The whole beat shows at once: one lane of sixteen steps for every pad the
  // loop uses, so a rhythm is read and written without paging through sounds.
  import { app, act, live, pattern, playhead, toggleStep, setBars, pasteLoop, capture as keep } from "../lib/app.svelte";
  import NoteDetail from "./NoteDetail.svelte";
  import { TICKS_PER_STEP, capture } from "../lib/api";

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

  /** The lanes: the kit's first four pads, every pad with steps in this loop, and the pad in hand. */
  const lanes = $derived.by(() => {
    const show = new Set<number>();
    app.pads.forEach((pad, i) => {
      if (used.has(pad.pitches[0])) show.add(i);
    });
    // the kit's first four stay, so a lane never leaves while a beat is being written
    for (let i = 0; i < Math.min(4, app.pads.length); i++) show.add(i);
    if (app.pads[app.pad]) show.add(app.pad);
    return [...show].sort((a, b) => a - b).map((i) => ({ i, name: app.pads[i].label.toLowerCase(), pitch: app.pads[i].pitches[0] }));
  });
  /** The steps lit on this page, by pitch. */
  const lit = $derived.by(() => {
    const map = new Map<number, Set<number>>();
    const base = app.page * 16;
    for (const n of pat?.notes ?? []) {
      const i = Math.round(n.start / TICKS_PER_STEP) - base;
      if (i < 0 || i > 15) continue;
      if (!map.has(n.pitch)) map.set(n.pitch, new Set());
      map.get(n.pitch)!.add(i);
    }
    return map;
  });

  // ---- one sheet at a time over the lanes, as in the roll
  type Sheet = "bars" | "edit";
  let sheet = $state<Sheet | null>(null);
  const toggle = (which: Sheet) => (sheet = sheet === which ? null : which);
  // Escape closes an open sheet, and only that: it is answered before the app hears it
  $effect(() => {
    if (!sheet) return;
    const esc = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      sheet = null;
    };
    addEventListener("keydown", esc, true);
    return () => removeEventListener("keydown", esc, true);
  });

  const copied = $derived(app.clip?.track === app.track && app.clip?.slot === app.section);
  const grooves = $derived(app.info?.catalog.grooves ?? []);
  async function groove(id: string) {
    sheet = null;
    await act({ type: "load_groove", track: app.track, slot: app.section, groove: id });
  }

  // ---- the pads fold away, like the keyboard
  const KEPT = "mjolnir.pads";
  let padsShown = $state(
    (() => {
      try {
        return localStorage.getItem(KEPT) !== "hidden";
      } catch {
        return true;
      }
    })(),
  );
  function showPads(on: boolean) {
    padsShown = on;
    try {
      localStorage.setItem(KEPT, on ? "shown" : "hidden");
    } catch {
      /* private window */
    }
  }

  // a long press (or a right click) on a lit step picks it without switching it off
  let pressing: ReturnType<typeof setTimeout> | null = null;
  let picked = false;
  const pickStep = (i: number, pitch: number) => (app.pick = { start: (app.page * 16 + i) * TICKS_PER_STEP, pitch });
  function stepDown(e: PointerEvent, i: number, pitch: number, lit: boolean) {
    // the right button asks about a step; it never writes one
    if (e.pointerType === "mouse" && e.button !== 0) return void (picked = true);
    picked = false;
    if (!lit) return;
    pressing = setTimeout(() => {
      picked = true;
      pickStep(i, pitch);
    }, 320);
  }
  function stepUp(i: number, pitch: number) {
    if (pressing) clearTimeout(pressing);
    pressing = null;
    if (!picked) void toggleStep(app.page * 16 + i, pitch);
  }
  const isPicked = (i: number, pitch: number) => app.pick?.pitch === pitch && app.pick?.start === (app.page * 16 + i) * TICKS_PER_STEP;

  /** A lane's name plays its pad and takes it in hand. */
  function laneDown(e: PointerEvent, i: number) {
    capture(e);
    app.pad = i;
    held[i] = true;
    live({ type: "pad_on", track: app.track, pad: i, velocity: 0.85 });
  }

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
  <!-- one line of tools; a held step takes it over with its own -->
  <div class="bar">
    {#if app.pick && pat}
      <NoteDetail named={false} inline />
    {:else}
      <button class="tool" aria-expanded={sheet === "bars"} aria-pressed={sheet === "bars"} aria-label="bars: {bars}" onclick={() => toggle("bars")}>
        <span class="label">bars</span>{bars}
      </button>
      {#if bars > 1}
        <span class="pages" role="group" aria-label={tx("showing bar")}>
          {#each Array.from({ length: bars }, (_, i) => i) as b}
            <button
              class="tool sq"
              class:now={head >= 0 && Math.floor(head / 16) === b}
              aria-pressed={app.page === b}
              aria-label="{tx('showing bar')} {b + 1}"
              onclick={() => {
                app.page = b;
                follow = false;
              }}>{b + 1}</button
            >
          {/each}
        </span>
      {/if}
      <span class="grow"></span>
      <button class="tool" title="keep what you just played as this loop" onclick={keep}>{tx("capture")}</button>
      <button class="tool" aria-expanded={sheet === "edit"} aria-pressed={sheet === "edit"} onclick={() => toggle("edit")}>{tx("edit")}</button>
    {/if}
  </div>

  <div class="stage" class:tall={!padsShown}>
    <div class="lanes" role="group" aria-label="steps">
      {#each lanes as lane (lane.i)}
        {@const on = lit.get(lane.pitch)}
        <div class="lane">
          <button
            class="who"
            aria-pressed={app.pad === lane.i}
            aria-label="{lane.name}: play"
            onpointerdown={(e) => laneDown(e, lane.i)}
            onpointerup={() => up(lane.i)}
            onpointercancel={() => up(lane.i)}
            oncontextmenu={(e) => e.preventDefault()}>{lane.name}</button
          >
          {#each { length: 16 } as _, i}
            {@const isOn = !!on?.has(i)}
            <button
              class="step"
              class:beat={i % 4 === 0}
              class:head={head === app.page * 16 + i}
              class:picked={isPicked(i, lane.pitch)}
              aria-pressed={isOn}
              aria-label="{lane.name}, step {i + 1}"
              onpointerdown={(e) => stepDown(e, i, lane.pitch, isOn)}
              onpointerup={() => stepUp(i, lane.pitch)}
              onpointerleave={() => pressing && clearTimeout(pressing)}
              oncontextmenu={(e) => {
                e.preventDefault();
                if (isOn) pickStep(i, lane.pitch);
              }}
              onkeydown={(e) => (e.key === "Enter" || e.key === " ") && toggleStep(app.page * 16 + i, lane.pitch)}
            ></button>
          {/each}
        </div>
      {/each}
      <p class="hint label">{tx("a pad you play gets a lane of its own")}</p>
    </div>

  </div>

  {#if sheet}
    <button class="veil" aria-label="close" onclick={() => (sheet = null)}></button>
    <div class="sheet" role="group" aria-label={sheet}>
      {#if sheet === "bars"}
        <div class="line">
          <span class="label">bars</span>
          {#each [1, 2, 4, 8] as b}
            <button
              class="opt"
              aria-pressed={bars === b}
              onclick={() => {
                void setBars(b);
                sheet = null;
              }}>{b}</button
            >
          {/each}
          {#if bars > 1}
            <span class="sep"></span>
            <button class="opt" aria-pressed={follow} onclick={() => (follow = !follow)}>{tx("follow")}</button>
          {/if}
        </div>
      {:else}
        <div class="line" role="group" aria-label={tx("a beat to start from")}>
          <span class="label">{tx("a beat to start from")}</span>
          {#each grooves.filter((g) => g.kind !== "hats") as g}
            <button class="opt" onclick={() => groove(g.id)}>{g.label.toLowerCase()}</button>
          {/each}
        </div>
        <div class="line" role="group" aria-label={tx("hats over it")}>
          <span class="label">{tx("hats over it")}</span>
          {#each grooves.filter((g) => g.kind === "hats") as g}
            <button class="opt" onclick={() => groove(g.id)}>{g.label.toLowerCase().replace(/^hats: /, "")}</button>
          {/each}
        </div>
        <div class="line" role="group" aria-label="loop">
          <span class="label">loop</span>
          <button class="opt" disabled={!pat} aria-pressed={copied} onclick={() => (app.clip = { track: app.track, slot: app.section })}>
            {copied ? tx("copied") : tx("copy")}
          </button>
          <button class="opt" disabled={!app.clip || copied} onclick={pasteLoop}>{tx("paste")}</button>
          <button class="opt" disabled={!pat} onclick={() => act({ type: "quantize", track: app.track, slot: app.section })}>{tx("snap")}</button>
          {#if pat?.automation.length}
            <button class="opt" onclick={() => act({ type: "clear_automation", track: app.track, slot: app.section, param: null })}>
              {tx("forget knob moves")}
            </button>
          {/if}
          <button class="opt" disabled={!pat} onclick={() => act({ type: "delete_pattern", track: app.track, slot: app.section })}>{tx("clear")}</button>
        </div>
      {/if}
    </div>
  {/if}

  <div class="playbar">
    <button class="tool" aria-expanded={padsShown} aria-pressed={padsShown} onclick={() => showPads(!padsShown)}>pads</button>
    <span class="grow"></span>
    <span class="label what">{(app.pads[app.pad]?.label ?? "").toLowerCase()}</span>
  </div>

  {#if padsShown}
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
  {/if}
</div>

<style>
  .play {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .bar,
  .playbar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 52px;
    padding: 0 16px;
    min-width: 0;
  }
  .bar {
    border-bottom: 1px solid var(--line);
  }
  .playbar {
    border-top: 1px solid var(--line);
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .tool {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 10px;
    border-color: var(--line);
    font-variant-numeric: tabular-nums;
  }
  .tool .label {
    color: inherit;
    opacity: 0.62;
  }
  .tool.sq {
    width: 32px;
    padding: 0;
    justify-content: center;
  }
  /* the bar being played, when another is on screen */
  .tool.now {
    box-shadow: inset 0 -2px 0 var(--fg);
  }
  .pages {
    display: inline-flex;
    gap: 4px;
  }
  .what {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .stage {
    position: relative;
    flex: 0 1 auto;
    min-height: 96px;
    max-height: 48%;
    display: flex;
  }
  .stage.tall {
    flex: 1 1 0;
    max-height: none;
  }
  .lanes {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 8px 16px;
    display: flex;
    flex-direction: column;
    gap: 3px;
    overscroll-behavior: contain;
  }
  .lane {
    display: grid;
    grid-template-columns: 64px repeat(16, minmax(0, 1fr));
    gap: 2px;
    align-items: stretch;
  }
  .who {
    height: 32px;
    padding: 0 6px;
    border-color: transparent;
    color: var(--fg-muted);
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    touch-action: none;
  }
  .who[aria-pressed="true"] {
    background: none;
    color: var(--fg);
    border-color: var(--line);
  }
  .step {
    position: relative;
    height: 32px;
    min-width: 0;
    padding: 0;
    border-color: var(--line);
  }
  /* the beats are told apart by their first step, and by a breath before it */
  .step.beat {
    border-color: var(--edge);
    margin-left: 3px;
  }
  .lane .step:nth-child(2) {
    margin-left: 0;
  }
  /* the step whose detail is on screen */
  .step.picked::after {
    content: "";
    position: absolute;
    right: 2px;
    top: 2px;
    width: 4px;
    height: 4px;
    background: var(--bg);
    outline: 1px solid var(--fg);
  }
  /* the step being played: a line at its foot */
  .step.head {
    box-shadow: inset 0 -2px 0 var(--fg);
  }
  .step.head[aria-pressed="true"] {
    box-shadow: inset 0 -2px 0 var(--bg);
  }
  .hint {
    margin: 6px 0 0;
    color: var(--fg-faint);
  }

  .veil {
    position: absolute;
    inset: 52px 0 0;
    z-index: 3;
    width: 100%;
    height: calc(100% - 52px);
    border: 0;
    padding: 0;
    background: color-mix(in srgb, var(--bg) 55%, transparent);
    cursor: default;
  }
  .sheet {
    position: absolute;
    left: 0;
    right: 0;
    top: 52px;
    z-index: 4;
    max-height: calc(100% - 52px);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    border-bottom: 1px solid var(--edge);
    animation: drop var(--dur-fast) var(--ease);
  }
  @keyframes drop {
    from {
      clip-path: inset(0 0 100% 0);
    }
    to {
      clip-path: inset(0 0 0 0);
    }
  }
  .line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 10px 16px;
  }
  .line + .line {
    border-top: 1px solid var(--line);
  }
  .line > .label {
    flex: none;
    width: 100%;
  }
  .opt {
    height: 36px;
    min-width: 40px;
    padding: 0 10px;
    border-color: var(--line);
  }
  .sep {
    width: 1px;
    height: 20px;
    margin: 0 4px;
    background: var(--line);
  }

  .pads {
    flex: 1 0 auto;
    /* the pads keep a size a finger can play, however many lanes there are */
    min-height: min(42%, 220px);
    padding: 0 16px 12px;
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
  @media (pointer: coarse) {
    .tool {
      height: 36px;
    }
    .tool.sq {
      width: 36px;
    }
    .opt {
      height: 44px;
      min-width: 44px;
    }
    .who,
    .step {
      height: 38px;
    }
  }
  @media (max-width: 420px) {
    .bar,
    .playbar {
      padding: 0 12px;
    }
    .lanes {
      padding: 8px 12px;
    }
    .lane {
      grid-template-columns: 52px repeat(16, minmax(0, 1fr));
      gap: 1px;
    }
    .who {
      padding: 0 2px;
    }
    .pads {
      padding: 0 12px 12px;
    }
    .line {
      padding: 10px 12px;
    }
  }
  @media (min-width: 860px) {
    .play {
      max-width: 860px;
      margin: 0 auto;
      width: 100%;
    }
  }
</style>
