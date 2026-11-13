<script lang="ts">
  import { tx } from "../lib/i18n.svelte";
  // The loops face, where everything starts: tracks across, sections down.
  // A cell is a loop in its track's colour. One click plays it, another
  // stops it; a double click steps inside. An empty cell has nothing to
  // play, so one click steps in.
  import { app, act, selectSection, selectTrack, openCell, toggleCell, startCell, sounding, playRow, playing, pattern, pasteLoop, trackColor } from "../lib/app.svelte";
  import NotePreview from "./NotePreview.svelte";
  import Menu, { type MenuItem } from "./Menu.svelte";
  import { hold } from "../lib/hold";

  const tracks = $derived(app.project?.tracks ?? []);
  const scenes = $derived(app.project?.scenes ?? []);

  let last = { t: -1, s: -1, at: 0 };
  async function cell(t: number, s: number) {
    if (!pattern(t, s)) return openCell(t, s);
    const now = performance.now();
    const double = last.t === t && last.s === s && now - last.at < 350;
    if (double) {
      // the first click of the pair already answered: if it stopped the
      // loop, bring it back, then step in with it playing
      last = { t: -1, s: -1, at: 0 };
      if (sounding(t) !== s) await startCell(t, s);
      return openCell(t, s);
    }
    last = { t, s, at: now };
    await toggleCell(t, s);
  }

  // ---- only the rows in use show, and one more to grow into: an empty set is one quiet row
  const lastUsed = $derived.by(() => {
    let last = -1;
    scenes.forEach((_, s) => {
      if (tracks.some((t) => t.slots[s])) last = s;
    });
    return last;
  });
  const empty = $derived(lastUsed < 0);
  const shown = $derived(Math.min(scenes.length, Math.max(lastUsed + 2, app.section + 1, 1)));
  let renaming = $state(-1);
  function rename(s: number, name: string) {
    renaming = -1;
    void act({ type: "rename_scene", scene: s, name: name.trim() || String.fromCharCode(65 + s) });
  }

  // ---- a long press, or the right button: what can be done with this loop or this track
  let menu = $state<{ x: number; y: number; title: string; items: MenuItem[] } | null>(null);

  async function choose(t: number, s: number) {
    app.section = s;
    if (app.track !== t) await selectTrack(t);
  }
  function cellMenu(t: number, s: number, at: { x: number; y: number }) {
    const filled = !!pattern(t, s);
    const c = app.clip;
    const here = c?.track === t && c?.slot === s;
    void choose(t, s);
    menu = {
      ...at,
      title: `${tracks[t]?.name.toLowerCase() ?? ""} · ${(scenes[s]?.name ?? "").toLowerCase()}`,
      items: [
        { label: tx("open"), pick: () => void openCell(t, s) },
        { label: tx("copy"), disabled: !filled, pick: () => (app.clip = { track: t, slot: s }) },
        { label: tx("paste"), disabled: !c || here || !pattern(c.track, c.slot), pick: () => void choose(t, s).then(pasteLoop) },
        {
          label: tx("copy to the next free row"),
          disabled: !filled || freeBelow(t, s) < 0,
          pick: () => void act({ type: "duplicate_pattern", track: t, from: s, to: freeBelow(t, s) }),
        },
        { label: tx("delete loop"), disabled: !filled, danger: true, pick: () => void act({ type: "delete_pattern", track: t, slot: s }) },
      ],
    };
  }
  function trackMenu(t: number, at: { x: number; y: number }) {
    const tr = tracks[t];
    if (!tr) return;
    void choose(t, app.section);
    menu = {
      ...at,
      title: tr.name.toLowerCase(),
      items: [
        { label: tx("open"), pick: () => void openCell(t, app.section) },
        { label: tr.mute ? tx("unmute") : "mute", pick: () => void act({ type: "set_mute", track: t, mute: !tr.mute }) },
        { label: tx("remove track"), disabled: tracks.length < 2, danger: true, pick: () => void act({ type: "remove_track", track: t }) },
      ],
    };
  }

  async function addTrack(kind: "drums" | "melodic") {
    await act({ type: "add_track", kind });
    await openCell((app.project?.tracks.length ?? 1) - 1, app.section);
    // the first thing a new track wants is its sound
    app.pane = "sound";
  }
  function addMenu(e: MouseEvent) {
    const b = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menu = {
      x: b.right - 168,
      y: b.bottom + 4,
      title: tx("new track"),
      items: [
        { label: tx("drums"), pick: () => void addTrack("drums") },
        { label: tx("keys"), pick: () => void addTrack("melodic") },
      ],
    };
  }

  /** The next section down where this track has no loop yet. */
  function freeBelow(t: number, s: number): number {
    for (let k = s + 1; k < scenes.length; k++) if (!pattern(t, k)) return k;
    return -1;
  }
  async function clearRow(s: number) {
    for (let t = 0; t < tracks.length; t++) if (pattern(t, s)) await act({ type: "delete_pattern", track: t, slot: s });
  }
  /** Copy a whole row into the next empty one (or a new one), to vary it there. */
  async function copyRow(from: number) {
    let to = -1;
    for (let k = from + 1; k < scenes.length; k++)
      if (!tracks.some((_, t) => pattern(t, k))) {
        to = k;
        break;
      }
    if (to < 0) {
      if (scenes.length >= 8) {
        app.note = "there is no empty section left to copy into";
        return;
      }
      await act({ type: "add_section" });
      to = (app.project?.scenes.length ?? 1) - 1;
    }
    for (let t = 0; t < tracks.length; t++) if (pattern(t, from)) await act({ type: "duplicate_pattern", track: t, from, to });
    await selectSection(to);
  }
  function rowMenu(s: number, at: { x: number; y: number }) {
    const any = tracks.some((_, t) => pattern(t, s));
    app.section = s;
    menu = {
      ...at,
      title: `${tx("section")} ${(scenes[s]?.name ?? "").toLowerCase()}`,
      items: [
        { label: tx("play the row"), disabled: !any, pick: () => void playRow(s) },
        { label: tx("copy to the next empty row"), disabled: !any, pick: () => void copyRow(s) },
        { label: tx("rename"), pick: () => (renaming = s) },
        { label: tx("clear the row"), disabled: !any, danger: true, pick: () => void clearRow(s) },
      ],
    };
  }

</script>

<div class="loops">
  <div class="grid" style="grid-template-columns: 40px repeat({tracks.length}, minmax(0, 1fr)){tracks.length < 8 ? ' 40px' : ''}">
    <span></span>
    {#each tracks as t, i}
      <button
        class="plain th"
        class:sel={app.track === i}
        class:muted={t.mute}
        style="--id: {trackColor(t)}"
        aria-label="{t.name}: open"
        onclick={() => openCell(i, app.section)}
        use:hold={(at) => trackMenu(i, at)}
      >
        <span class="tname">{t.name.toLowerCase()}</span>
        <span class="meter" style="transform: scaleX({Math.min(1, (app.status?.tracks[i]?.level ?? 0) * 2.5)})"></span>
      </button>
    {/each}
    {#if tracks.length < 8}
      <button class="plain th add" aria-label="add a track" aria-haspopup="menu" onclick={addMenu}>+</button>
    {/if}

    {#each scenes.slice(0, shown) as scene, s}
      {#if renaming === s}
        <!-- svelte-ignore a11y_autofocus -->
        <input
          class="rename"
          type="text"
          aria-label="name of the section"
          maxlength="12"
          value={scene.name}
          autofocus
          onblur={(e) => rename(s, e.currentTarget.value)}
          onkeydown={(e) => {
            if (e.key === "Enter") e.currentTarget.blur();
            if (e.key === "Escape") renaming = -1;
          }}
        />
      {:else}
        <button
          class="row"
          aria-pressed={app.section === s}
          aria-label="section {scene.name}: play the row"
          onclick={() => playRow(s)}
          use:hold={(at) => rowMenu(s, at)}
        >
          {scene.name.length <= 3 ? scene.name.toLowerCase() : s + 1}
        </button>
      {/if}
      {#each tracks as t, i}
        {@const p = t.slots[s]}
        {@const st = app.status?.tracks[i]}
        {@const live = playing() && !app.status?.song_mode && st?.active === s}
        {@const queued = playing() && st?.queued === s}
        {@const leaving = live && !!st?.queued_stop}
        <button
          class="cell"
          class:filled={!!p}
          class:invite={empty}
          class:sel={app.track === i && app.section === s}
          class:live
          class:queued
          class:leaving
          style="--id: {trackColor(t)}"
          aria-label="{t.name}, section {scene.name}{p ? (leaving ? ', stopping' : live ? ', playing' : '') : ', empty'}"
          title={p ? (leaving ? "stops at the bar · click: stop now" : "click: play or stop · double click: open · right click: more") : "open"}
          aria-haspopup="menu"
          onclick={() => cell(i, s)}
          use:hold={(at) => cellMenu(i, s, at)}
        >
          {#if p}
            <span class="notes"><NotePreview pattern={p} drums={t.kind === "drums"} /></span>
            {#if live}
              <span class="sweep" style="transform: scaleX({st?.progress ?? 0})"></span>
            {/if}
            {#if queued}<span class="led yl blink corner"></span>{/if}
            {#if leaving}<span class="led rd blink corner"></span>{/if}
          {:else}
            <span class="plus">+</span>
          {/if}
        </button>
      {/each}
      {#if tracks.length < 8}<span></span>{/if}
    {/each}
  </div>

  {#if menu}<Menu {...menu} onclose={() => (menu = null)} />{/if}

</div>

<style>
  .loops {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 8px 16px 12px;
    height: 100%;
    overflow-y: auto;
  }
  .grid {
    display: grid;
    gap: 4px;
    grid-auto-rows: minmax(48px, 92px);
    grid-template-rows: 36px;
    flex: 0 1 auto;
  }
  .th {
    position: relative;
    height: 36px;
    white-space: normal;
    line-height: 14px;
    padding: 0 4px;
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    color: var(--fg-muted);
    text-align: left;
  }
  .th.sel {
    color: var(--fg);
  }
  .th.muted .tname {
    color: var(--fg-faint);
  }
  /* the track's colour sits under its name; the level grows along it */
  .th::after {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 2px;
    background: color-mix(in srgb, var(--id, transparent) 35%, transparent);
  }
  .th.add::after {
    display: none;
  }
  .th.add {
    justify-content: center;
  }
  .tname {
    overflow: hidden;
    max-height: 28px;
    overflow-wrap: anywhere;
  }
  /* narrow columns: a smaller hand, so a word stays in one piece */
  @media (max-width: 520px) {
    .tname {
      font-size: 11px;
      line-height: 14px;
      letter-spacing: 0;
    }
  }
  .meter {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 2px;
    background: var(--id);
    transform-origin: left;
    transform: scaleX(0);
    z-index: 1;
  }
  .row {
    height: auto;
    padding: 0;
    border-color: var(--line);
    color: var(--fg-muted);
  }
  .row[aria-pressed="true"] {
    color: var(--bg);
  }
  .cell,
  .th {
    -webkit-touch-callout: none;
    user-select: none;
    -webkit-user-select: none;
  }
  .cell {
    position: relative;
    height: auto;
    padding: 6px;
    border-color: var(--line);
    overflow: hidden;
  }
  /* an empty place is quiet: no frame, no sign, until the hand is over it.
     Only a set with nothing in it says where to begin. */
  .cell:not(.filled) {
    border-color: transparent;
    background: color-mix(in srgb, var(--fg) 3%, transparent);
  }
  .cell:not(.filled):hover,
  .cell:not(.filled):focus-visible,
  .cell:not(.filled).sel {
    border-color: var(--line);
  }
  .cell .plus {
    color: var(--fg-faint);
    opacity: 0;
  }
  .cell:hover .plus,
  .cell.invite .plus,
  .cell.sel .plus {
    opacity: 1;
  }
  .rename {
    width: 100%;
    height: 100%;
    min-width: 0;
    padding: 0 4px;
    text-align: center;
  }
  .cell.filled {
    border-color: color-mix(in srgb, var(--id) 45%, transparent);
    background: color-mix(in srgb, var(--id) 9%, transparent);
  }
  .cell.sel {
    border-color: var(--fg);
  }
  .cell.live {
    background: color-mix(in srgb, var(--id) 26%, transparent);
  }
  /* told to stop, playing out its bar */
  .cell.leaving .notes {
    opacity: 0.45;
  }
  .cell.queued {
    border-style: dashed;
    border-color: var(--yl);
  }
  .notes {
    position: absolute;
    inset: 0;
    color: var(--id);
  }
  /* the part of the loop already played, as a wash that grows with it */
  .sweep {
    position: absolute;
    inset: 0;
    background: color-mix(in srgb, var(--id) 22%, transparent);
    transform-origin: left;
    pointer-events: none;
  }
  .corner {
    position: absolute;
    top: 4px;
    right: 4px;
  }
</style>
