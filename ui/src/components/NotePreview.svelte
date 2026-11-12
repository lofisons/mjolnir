<script lang="ts">
  // A loop at a glance: its notes drawn small, repeated for as long as the
  // clip lasts.
  import { barTicks, type Pattern } from "../lib/api";
  import { app } from "../lib/app.svelte";

  let { pattern, drums = false, repeat = 1 }: { pattern: Pattern; drums?: boolean; repeat?: number } = $props();

  const len = $derived(pattern.bars * barTicks(app.project?.meter));
  const range = $derived.by(() => {
    if (drums) return { lo: 36, hi: 51 };
    if (!pattern.notes.length) return { lo: 60, hi: 72 };
    let lo = 127;
    let hi = 0;
    for (const n of pattern.notes) {
      lo = Math.min(lo, n.pitch);
      hi = Math.max(hi, n.pitch);
    }
    // a minimum span, so a one-note line does not fill the whole height
    const pad = Math.max(0, 8 - (hi - lo)) / 2;
    return { lo: lo - pad - 1, hi: hi + pad + 1 };
  });
  const rows = $derived(range.hi - range.lo + 1);
  const copies = $derived(Math.max(1, Math.ceil(repeat)));
</script>

<svg class="preview" viewBox="0 0 {len * repeat} {rows}" preserveAspectRatio="none" aria-hidden="true">
  {#each { length: copies } as _, k}
    {#each pattern.notes as n}
      <rect
        x={k * len + n.start}
        y={range.hi - n.pitch}
        width={Math.max(drums ? len / 64 : n.length, len / 128)}
        height="1"
        opacity={0.4 + n.velocity * 0.6}
      />
    {/each}
  {/each}
</svg>

<style>
  .preview {
    position: absolute;
    inset: 3px 2px;
    width: calc(100% - 4px);
    height: calc(100% - 6px);
    pointer-events: none;
  }
  rect {
    fill: currentColor;
  }
</style>
