<script lang="ts">
  // The piano roll: the loop's notes, pitch up the side and time across.
  // Touch empty space to write a note (a chord, when chords are on); drag a
  // note to move it, its right edge to stretch it; touch a note to pick it,
  // double click it to remove it.
  import { onMount } from "svelte";
  import { app, act, live, pattern, track, writeNote, trackColor } from "../lib/app.svelte";
  import { TICKS_PER_STEP, noteName, scalePitches, chordOn, type Note, capture } from "../lib/api";

  const ROW = 18;
  const pat = $derived(pattern());
  const bars = $derived(pat?.bars ?? 1);
  const steps = $derived(bars * 16);
  const t = $derived(track());
  const scale = $derived(t?.scale ?? app.project?.scale ?? { root: 0, kind: "minor" });
  const base = $derived(((t?.octave ?? 4) + 1) * 12);

  /** Rows: the key's own notes around the track's octave, plus any note already written outside them. */
  const rows = $derived.by(() => {
    const set = new Set(scalePitches(scale, Math.max(0, base - 12), Math.min(127, base + 31)));
    for (const n of pat?.notes ?? []) set.add(n.pitch);
    return [...set].sort((a, b) => b - a);
  });
  const rowOf = (pitch: number) => rows.indexOf(pitch);

  let box: HTMLDivElement;
  let width = $state(320);
  const stepW = $derived(Math.max(9, (width - 34) / steps));

  onMount(() => {
    const ro = new ResizeObserver(() => (width = box.clientWidth));
    ro.observe(box);
    width = box.clientWidth;
    // open on the track's own octave
    const home = rows.findIndex((p) => p <= base + 12);
    box.scrollTop = Math.max(0, home * ROW - 8);
    return () => ro.disconnect();
  });

  const head = $derived.by(() => {
    const st = app.status?.tracks[app.track];
    if (!st || !pat || !app.status?.playing || app.status.song_mode || st.active !== app.section) return -1;
    return st.progress * steps * stepW;
  });

  /** With chords on, the notes a root stands for. */
  const spell = (pitch: number): number[] => (t?.chord_mode ? chordOn(scale, pitch, t.chord_mode) : [pitch]);

  function at(e: PointerEvent, el: Element) {
    const r = el.getBoundingClientRect();
    return {
      step: Math.max(0, Math.min(steps - 1, Math.floor((e.clientX - r.left) / stepW))),
      row: Math.max(0, Math.min(rows.length - 1, Math.floor((e.clientY - r.top) / ROW))),
    };
  }

  function blip(pitches: number[]) {
    for (const p of pitches) live({ type: "note_on", track: app.track, pitch: p, velocity: 0.7 });
    setTimeout(() => {
      for (const p of pitches) live({ type: "note_off", track: app.track, pitch: p });
    }, 220);
  }

  // ---- writing a note: only a touch that stays put. A finger that travels is
  // scrolling the sheet, and the browser says so by cancelling the pointer.
  const TAP_SLOP = 8;
  let tap: { id: number; x: number; y: number; left: number; top: number; step: number; row: number } | null = null;

  function gridDown(e: PointerEvent) {
    if (e.target !== e.currentTarget || (e.pointerType === "mouse" && e.button !== 0)) return;
    const { step, row } = at(e, e.currentTarget as Element);
    tap = { id: e.pointerId, x: e.clientX, y: e.clientY, left: box.scrollLeft, top: box.scrollTop, step, row };
  }
  function gridMove(e: PointerEvent) {
    if (tap && e.pointerId === tap.id && Math.hypot(e.clientX - tap.x, e.clientY - tap.y) > TAP_SLOP) tap = null;
  }
  async function gridUp(e: PointerEvent) {
    const held = tap;
    tap = null;
    if (!held || e.pointerId !== held.id) return;
    if (Math.hypot(e.clientX - held.x, e.clientY - held.y) > TAP_SLOP) return;
    if (Math.abs(box.scrollLeft - held.left) > 2 || Math.abs(box.scrollTop - held.top) > 2) return;
    const pitches = spell(rows[held.row]);
    blip(pitches);
    await writeNote(pitches, held.step, Math.min(app.noteLen, steps - held.step));
  }

  // ---- moving and stretching a note
  let drag = $state<{ note: Note; x0: number; y0: number; mode: "move" | "stretch"; start: number; length: number; pitch: number; moved: boolean } | null>(null);

  function noteDown(e: PointerEvent, n: Note) {
    e.stopPropagation();
    const el = e.currentTarget as HTMLElement;
    capture(e);
    const r = el.getBoundingClientRect();
    const mode = e.clientX > r.right - 7 && r.width > 12 ? "stretch" : "move";
    drag = { note: n, x0: e.clientX, y0: e.clientY, mode, start: n.start, length: n.length, pitch: n.pitch, moved: false };
  }
  function noteMove(e: PointerEvent) {
    if (!drag) return;
    const dx = Math.round((e.clientX - drag.x0) / stepW) * TICKS_PER_STEP;
    const total = steps * TICKS_PER_STEP;
    if (drag.mode === "stretch") {
      const length = Math.max(TICKS_PER_STEP, Math.min(total - drag.note.start, drag.note.length + dx));
      if (length !== drag.length) drag.moved = true;
      drag.length = length;
    } else {
      const dy = Math.round((e.clientY - drag.y0) / ROW);
      const start = Math.max(0, Math.min(total - drag.note.length, drag.note.start + dx));
      const row = Math.max(0, Math.min(rows.length - 1, rowOf(drag.note.pitch) + dy));
      const pitch = rows[row];
      if (start !== drag.start || pitch !== drag.pitch) {
        if (pitch !== drag.pitch) blip([pitch]);
        drag.moved = true;
      }
      drag.start = start;
      drag.pitch = pitch;
    }
  }
  async function noteUp() {
    if (!drag) return;
    const d = drag;
    drag = null;
    const where = { track: app.track, slot: app.section, start: d.note.start, pitch: d.note.pitch };
    if (!d.moved) {
      // a touch picks the note (its detail shows below); a second touch lets it go
      const same = app.pick?.start === d.note.start && app.pick?.pitch === d.note.pitch;
      app.pick = same ? null : { start: d.note.start, pitch: d.note.pitch };
      if (!same) blip([d.note.pitch]);
      return;
    }
    app.noteLen = Math.max(1, Math.round(d.length / TICKS_PER_STEP));
    await act({ type: "move_note", ...where, note: { ...d.note, start: d.start, length: d.length, pitch: d.pitch } });
    app.pick = { start: d.start, pitch: d.pitch };
  }
  const shown = (n: Note) => (drag && drag.note === n ? { ...n, start: drag.start, length: drag.length, pitch: drag.pitch } : n);
</script>

<div class="roll" bind:this={box} onscroll={() => (tap = null)} style="--id: {trackColor(t)}">
  <div class="sheet" style="height: {rows.length * ROW}px; width: {34 + steps * stepW}px">
    <div class="names">
      {#each rows as p}
        <span class="name" class:root={(((p - scale.root) % 12) + 12) % 12 === 0} style="height: {ROW}px">{noteName(p).toLowerCase()}</span>
      {/each}
    </div>
    <div
      class="grid"
      role="application"
      aria-label="notes of the loop"
      style="background-size: {stepW}px {ROW}px, {stepW * 4}px 100%, {stepW * 16}px 100%"
      onpointerdown={gridDown}
      onpointermove={gridMove}
      onpointerup={gridUp}
      onpointercancel={() => (tap = null)}
      onpointerleave={() => (tap = null)}
    >
      {#each pat?.notes ?? [] as n}
        {@const v = shown(n)}
        {@const row = rowOf(v.pitch)}
        {#if row >= 0}
          <div
            class="note"
            class:held={drag?.note === n || (app.pick?.start === n.start && app.pick?.pitch === n.pitch)}
            ondblclick={() => act({ type: "remove_note", track: app.track, slot: app.section, start: n.start, pitch: n.pitch })}
            role="button"
            tabindex="-1"
            aria-label="{noteName(v.pitch)} at step {Math.round(v.start / TICKS_PER_STEP) + 1}"
            style="left: {(v.start / TICKS_PER_STEP) * stepW}px; width: {Math.max(4, (v.length / TICKS_PER_STEP) * stepW - 1)}px; top: {row * ROW + 1}px; height: {ROW - 2}px; opacity: {0.55 + v.velocity * 0.45}"
            onpointerdown={(e) => noteDown(e, n)}
            onpointermove={noteMove}
            onpointerup={noteUp}
            onpointercancel={() => (drag = null)}
          ></div>
        {/if}
      {/each}
      {#if head >= 0}<span class="head" style="transform: translateX({head}px)"></span>{/if}
    </div>
  </div>
</div>

<style>
  .roll {
    height: 100%;
    overflow: auto;
    border: 1px solid var(--line);
    overscroll-behavior: contain;
  }
  .sheet {
    display: flex;
    min-width: 100%;
  }
  .names {
    position: sticky;
    left: 0;
    z-index: 2;
    width: 34px;
    flex: none;
    background: var(--bg);
    border-right: 1px solid var(--line);
  }
  .name {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    padding-right: 4px;
    font: var(--tick);
    color: var(--fg-faint);
  }
  .name.root {
    color: var(--fg);
  }
  .grid {
    position: relative;
    flex: 1;
    touch-action: pan-x pan-y;
    cursor: crosshair;
    /* a hairline under every row, a line on every beat, a stronger one on every bar */
    background-image:
      linear-gradient(to bottom, transparent calc(100% - 1px), var(--line) calc(100% - 1px)),
      linear-gradient(to right, var(--line) 1px, transparent 1px),
      linear-gradient(to right, var(--edge) 1px, transparent 1px);
  }
  .note {
    position: absolute;
    background: var(--id);
    cursor: grab;
    touch-action: none;
    /* the right edge stretches */
    border-right: 3px solid color-mix(in srgb, var(--id) 60%, var(--bg));
  }
  .note.held {
    outline: 1px solid var(--fg);
  }
  .head {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    width: 1px;
    background: var(--fg);
    pointer-events: none;
  }
</style>
