<script lang="ts">
  import { capture } from "../lib/api";
  // VSGARD Knob: a ring of 11 ticks, a hairline circle, the travel so far as
  // an arc and an indicator line. Drag up or down; double click resets.
  let {
    value,
    label,
    size = 44,
    reset = undefined as number | undefined,
    text = undefined as string | undefined,
    onchange,
    onstart = () => {},
    onend = () => {},
  }: {
    /** 0..1 */
    value: number;
    label: string;
    size?: number;
    reset?: number;
    /** The value in words, shown while turning. */
    text?: string;
    onchange: (v: number) => void;
    onstart?: () => void;
    onend?: () => void;
  } = $props();

  let turning = $state(false);
  let from = 0;
  let fromValue = 0;
  let lastTap = 0;

  const r = $derived(size / 2 - 7);
  const c = $derived(size / 2);
  const angle = $derived(-135 + 270 * Math.min(1, Math.max(0, value)));
  const polar = (deg: number, rad: number) => {
    const a = ((deg - 90) * Math.PI) / 180;
    return [c + rad * Math.cos(a), c + rad * Math.sin(a)];
  };
  const arc = $derived.by(() => {
    const [x0, y0] = polar(-135, r);
    const [x1, y1] = polar(angle, r);
    return `M${x0} ${y0} A${r} ${r} 0 ${angle > 45 ? 1 : 0} 1 ${x1} ${y1}`;
  });
  const ticks = Array.from({ length: 11 }, (_, i) => i);

  function down(e: PointerEvent) {
    const now = performance.now();
    if (now - lastTap < 300 && reset !== undefined) {
      onstart();
      onchange(reset);
      onend();
      lastTap = 0;
      return;
    }
    lastTap = now;
    capture(e);
    turning = true;
    from = e.clientY;
    fromValue = value;
    onstart();
  }
  function move(e: PointerEvent) {
    if (!turning) return;
    const travel = e.shiftKey ? 600 : 150;
    onchange(Math.min(1, Math.max(0, fromValue + (from - e.clientY) / travel)));
  }
  function up() {
    if (!turning) return;
    turning = false;
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
  class="knob"
  role="slider"
  tabindex="0"
  aria-label={label}
  aria-valuemin="0"
  aria-valuemax="1"
  aria-valuenow={value}
  aria-valuetext={text ?? `${Math.round(value * 100)}`}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
  onkeydown={key}
>
  <svg width={size} height={size} viewBox="0 0 {size} {size}" aria-hidden="true">
    {#each ticks as i}
      {@const a = -135 + 27 * i}
      {@const long = i === 0 || i === 5 || i === 10}
      {@const [x0, y0] = polar(a, r + 3)}
      {@const [x1, y1] = polar(a, r + (long ? 7 : 5.5))}
      <line x1={x0} y1={y0} x2={x1} y2={y1} class="tick" />
    {/each}
    <circle cx={c} cy={c} r={r} class="ring" />
    <path d={arc} class="travel" />
    <line
      x1={polar(angle, r * 0.2)[0]}
      y1={polar(angle, r * 0.2)[1]}
      x2={polar(angle, r - 3)[0]}
      y2={polar(angle, r - 3)[1]}
      class="hand"
    />
  </svg>
  <span class="name" class:turning>{turning ? (text ?? Math.round(value * 100)) : label}</span>
</div>

<style>
  .knob {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    touch-action: none;
    cursor: ns-resize;
    user-select: none;
    outline: none;
    min-width: 0;
  }
  .knob:focus-visible svg {
    outline: 2px solid var(--fg);
    outline-offset: 2px;
  }
  .tick {
    stroke: var(--edge);
    stroke-width: 1;
  }
  .ring {
    fill: none;
    stroke: var(--edge);
    stroke-width: 1;
  }
  .travel {
    fill: none;
    stroke: var(--fg);
    stroke-width: 1.5;
  }
  .hand {
    stroke: var(--fg);
    stroke-width: 2;
  }
  .name {
    font: var(--tick);
    color: var(--fg-muted);
    white-space: nowrap;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .name.turning {
    color: var(--fg);
  }
</style>
