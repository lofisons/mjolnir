<script lang="ts">
  // A horizontal fader: a groove, the travel so far, a cap. Drag or click
  // sets the value; double click returns to the default.
  import { capture } from "../lib/api";

  let {
    value,
    label,
    reset = undefined as number | undefined,
    level = undefined as number | undefined,
    onchange,
    onstart = () => {},
    onend = () => {},
  }: {
    value: number;
    label: string;
    reset?: number;
    /** A live reading drawn under the groove (a meter). */
    level?: number;
    onchange: (v: number) => void;
    onstart?: () => void;
    onend?: () => void;
  } = $props();

  let dragging = $state(false);
  let last = 0;

  const at = (e: PointerEvent) => {
    const r = (e.currentTarget as Element).getBoundingClientRect();
    return Math.min(1, Math.max(0, (e.clientX - r.left - 9) / (r.width - 18)));
  };
  function down(e: PointerEvent) {
    const now = performance.now();
    if (now - last < 300 && reset !== undefined) {
      onstart();
      onchange(reset);
      onend();
      last = 0;
      return;
    }
    last = now;
    capture(e);
    dragging = true;
    onstart();
    onchange(at(e));
  }
  function move(e: PointerEvent) {
    if (dragging) onchange(at(e));
  }
  function up() {
    if (!dragging) return;
    dragging = false;
    onend();
  }
  function key(e: KeyboardEvent) {
    const step = e.shiftKey ? 0.1 : 0.01;
    let v: number | null = null;
    if (e.key === "ArrowUp" || e.key === "ArrowRight") v = value + step;
    if (e.key === "ArrowDown" || e.key === "ArrowLeft") v = value - step;
    if (e.key === "Home") v = 0;
    if (e.key === "End") v = 1;
    if (v === null) return;
    e.preventDefault();
    onstart();
    onchange(Math.min(1, Math.max(0, v)));
    onend();
  }
</script>

<div
  class="fader"
  role="slider"
  tabindex="0"
  aria-label={label}
  aria-valuemin="0"
  aria-valuemax="1"
  aria-valuenow={value}
  aria-valuetext={`${Math.round(value * 100)}`}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
  onkeydown={key}
>
  <span class="groove"></span>
  <span class="travel" style="width: calc({value} * (100% - 18px))"></span>
  {#if level !== undefined}
    <span class="level" style="transform: scaleX({Math.min(1, level)})"></span>
  {/if}
  <span class="cap" style="left: calc(9px + {value} * (100% - 18px))"></span>
</div>

<style>
  .fader {
    position: relative;
    height: var(--row);
    min-width: 64px;
    touch-action: none;
    cursor: ew-resize;
    outline: none;
  }
  .fader:focus-visible .cap {
    outline: 2px solid var(--fg);
    outline-offset: 2px;
  }
  .groove {
    position: absolute;
    left: 9px;
    right: 9px;
    top: 50%;
    height: 1px;
    background: var(--edge);
    /* a tick every tenth */
    box-shadow: 0 0 0 0 transparent;
  }
  .travel {
    position: absolute;
    left: 9px;
    top: calc(50% - 1px);
    height: 2px;
    background: var(--fg);
  }
  .level {
    position: absolute;
    left: 9px;
    right: 9px;
    top: calc(50% + 5px);
    height: 2px;
    background: var(--id, var(--fg-muted));
    transform-origin: left;
  }
  .cap {
    position: absolute;
    top: calc(50% - 10px);
    width: 3px;
    height: 20px;
    margin-left: -1px;
    background: var(--fg);
  }
</style>
