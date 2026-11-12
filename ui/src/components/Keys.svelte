<script lang="ts">
  // A keyboard: two octaves from the track's own. The notes of the key carry
  // a dot; its root, two.
  import { app, live, track } from "../lib/app.svelte";
  import { noteName, scalePitches, capture } from "../lib/api";

  const t = $derived(track());
  const scale = $derived(t?.scale ?? app.project?.scale ?? { root: 0, kind: "minor" });
  const base = $derived(((t?.octave ?? 4) + 1) * 12);
  const OCTAVES = 2;
  const WHITE = [0, 2, 4, 5, 7, 9, 11];
  // each black key sits between two white ones: its place, in white keys
  const BLACK: [number, number][] = [[1, 1], [3, 2], [6, 4], [8, 5], [10, 6]];
  const whites = $derived(Array.from({ length: OCTAVES * 7 + 1 }, (_, i) => base + Math.floor(i / 7) * 12 + WHITE[i % 7]));
  const blacks = $derived(
    Array.from({ length: OCTAVES }, (_, o) => BLACK.map(([semi, at]) => ({ pitch: base + o * 12 + semi, at: o * 7 + at }))).flat(),
  );
  const inKey = $derived(new Set(scalePitches(scale, 0, 127)));
  const isRoot = (p: number) => (((p - scale.root) % 12) + 12) % 12 === 0;
  let held = $state<Record<number, boolean>>({});

  function down(e: PointerEvent, p: number) {
    capture(e);
    held[p] = true;
    live({ type: "note_on", track: app.track, pitch: p, velocity: 0.85 });
  }
  function up(p: number) {
    if (!held[p]) return;
    held[p] = false;
    live({ type: "note_off", track: app.track, pitch: p });
  }

  // the computer keyboard, laid out like a piano: a row of white keys, the black ones above
  const ROW = "awsedftgyhujkolp;";
  function keydown(e: KeyboardEvent) {
    if (e.repeat || e.metaKey || e.ctrlKey || e.altKey) return;
    if ((e.target as HTMLElement).tagName === "INPUT") return;
    const i = ROW.indexOf(e.key.toLowerCase());
    if (i < 0) return;
    held[base + i] = true;
    live({ type: "note_on", track: app.track, pitch: base + i, velocity: 0.85 });
  }
  function keyup(e: KeyboardEvent) {
    const i = ROW.indexOf(e.key.toLowerCase());
    if (i >= 0) up(base + i);
  }
</script>

<svelte:window onkeydown={keydown} onkeyup={keyup} />

<div class="keys" role="group" aria-label="keyboard">
  {#each whites as p}
    <button
      class="white"
      class:hit={held[p]}
      aria-label={noteName(p)}
      onpointerdown={(e) => down(e, p)}
      onpointerup={() => up(p)}
      onpointercancel={() => up(p)}
      oncontextmenu={(e) => e.preventDefault()}
    >
      <span class="mark">{inKey.has(p) ? (isRoot(p) ? "··" : "·") : ""}</span>
      <span class="n">{p % 12 === 0 || isRoot(p) ? noteName(p).toLowerCase() : ""}</span>
    </button>
  {/each}
  {#each blacks as b}
    <button
      class="black"
      class:hit={held[b.pitch]}
      aria-label={noteName(b.pitch)}
      style="left: calc({(b.at / whites.length) * 100}% - {(0.32 / whites.length) * 100}%); width: {(0.64 / whites.length) * 100}%"
      onpointerdown={(e) => down(e, b.pitch)}
      onpointerup={() => up(b.pitch)}
      onpointercancel={() => up(b.pitch)}
      oncontextmenu={(e) => e.preventDefault()}
    >
      <span class="mark">{inKey.has(b.pitch) ? (isRoot(b.pitch) ? "··" : "·") : ""}</span>
    </button>
  {/each}
</div>

<style>
  .keys {
    position: relative;
    display: flex;
    height: 100%;
    min-height: 96px;
    touch-action: none;
  }
  .white {
    flex: 1 1 0;
    min-width: 0;
    height: 100%;
    padding: 0 0 6px;
    margin-right: -1px;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    align-items: center;
    gap: 2px;
    transition:
      background var(--dur-base) var(--ease),
      color var(--dur-base) var(--ease);
    touch-action: none;
  }
  .black {
    position: absolute;
    top: 0;
    height: 58%;
    padding: 0 0 4px;
    background: var(--bg-subtle);
    border-color: var(--edge);
    display: flex;
    align-items: flex-end;
    justify-content: center;
    z-index: 1;
    transition: background var(--dur-base) var(--ease);
    touch-action: none;
  }
  .black:hover {
    background: var(--bg-subtle);
  }
  .white.hit,
  .black.hit {
    background: var(--fg);
    color: var(--bg);
    transition: none;
  }
  .mark {
    font: var(--tick);
    height: 12px;
  }
  .n {
    font: var(--tick);
    color: var(--fg-muted);
    height: 12px;
  }
  .white.hit .n {
    color: var(--bg);
  }
</style>
