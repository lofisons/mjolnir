<script lang="ts">
  // The punch strip: eight effects over the whole mix, each one sounding
  // only while its key is held (a finger, or the number keys).
  import { app, punch } from "../lib/app.svelte";
  import { capture } from "../lib/api";

  const NAMES: Record<string, string> = {
    loop: "loop",
    stutter: "stutter",
    tape: "tape",
    lpf: "low",
    hpf: "high",
    crush: "crush",
    dub: "dub",
    freeze: "freeze",
  };
  const keys = $derived(app.info?.catalog.punch ?? []);
  let held = $state<Record<string, boolean>>({});

  function down(e: PointerEvent, k: string) {
    capture(e);
    hold(k, true);
  }
  function hold(k: string, on: boolean) {
    if (!!held[k] === on) return;
    held[k] = on;
    punch(k, on);
  }
  function keydown(e: KeyboardEvent) {
    if (e.repeat || e.metaKey || e.ctrlKey || e.altKey || !e.shiftKey) return;
    if ((e.target as HTMLElement).tagName === "INPUT") return;
    const i = "12345678".indexOf(e.code.replace("Digit", ""));
    if (e.code.startsWith("Digit") && i >= 0 && keys[i]) {
      e.preventDefault();
      e.stopPropagation();
      hold(keys[i], true);
    }
  }
  function keyup(e: KeyboardEvent) {
    const i = "12345678".indexOf(e.code.replace("Digit", ""));
    if (e.code.startsWith("Digit") && i >= 0 && keys[i]) hold(keys[i], false);
    // letting go of shift lets go of everything
    if (e.key === "Shift") for (const k of keys) hold(k, false);
  }
  // nothing stays held when the strip goes away
  $effect(() => () => {
    for (const k of Object.keys(held)) if (held[k]) punch(k, false);
  });
</script>

<svelte:window onkeydowncapture={keydown} onkeyup={keyup} />

<div class="punch from-right" role="group" aria-label="punch effects">
  {#each keys as k, i}
    <button
      class="key"
      class:held={held[k]}
      aria-label="{NAMES[k] ?? k}: hold"
      onpointerdown={(e) => down(e, k)}
      onpointerup={() => hold(k, false)}
      onpointercancel={() => hold(k, false)}
      oncontextmenu={(e) => e.preventDefault()}
    >
      <span class="n">{i + 1}</span>
      {NAMES[k] ?? k}
    </button>
  {/each}
</div>

<style>
  .punch {
    display: grid;
    grid-template-columns: repeat(8, minmax(0, 1fr));
    gap: 4px;
    padding: 6px 16px;
    border-top: 1px solid var(--line);
    touch-action: none;
  }
  .key {
    position: relative;
    height: calc(var(--row) + 8px);
    padding: 8px 0 0;
    overflow: hidden;
    text-overflow: clip;
    touch-action: none;
    transition: none;
  }
  .key .n {
    position: absolute;
    top: 2px;
    right: 4px;
    font: var(--tick);
    color: var(--fg-muted);
  }
  .key.held {
    background: var(--fg);
    color: var(--bg);
    border-color: var(--fg);
  }
  .key.held .n {
    color: var(--bg);
  }
  @media (max-width: 520px) {
    .punch {
      grid-template-columns: repeat(4, minmax(0, 1fr));
    }
  }
</style>
