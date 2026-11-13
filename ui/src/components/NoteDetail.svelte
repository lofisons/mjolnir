<script lang="ts">
  import { tx } from "../lib/i18n.svelte";
  // The note that is picked: how hard it is played, how often it sounds, how
  // many times it strikes.
  import Knob from "./Knob.svelte";
  import { app, act, pattern } from "../lib/app.svelte";
  import { noteName, type Note } from "../lib/api";

  let { named = true, inline = false }: { named?: boolean; inline?: boolean } = $props();
  const note = $derived(pattern()?.notes.find((n) => n.start === app.pick?.start && n.pitch === app.pick?.pitch) ?? null);

  // what the knobs show while they turn; committed when let go
  let draft = $state<Note | null>(null);
  const shown = $derived(draft ?? note);

  function turn(patch: Partial<Note>) {
    if (!note) return;
    draft = { ...(draft ?? note), ...patch };
  }
  async function commit() {
    if (!note || !draft) return;
    const next = draft;
    draft = null;
    await act({ type: "move_note", track: app.track, slot: app.section, start: note.start, pitch: note.pitch, note: next });
  }
  async function remove() {
    if (!note) return;
    await act({ type: "remove_note", track: app.track, slot: app.section, start: note.start, pitch: note.pitch });
    app.pick = null;
  }
  function key(e: KeyboardEvent) {
    if ((e.target as HTMLElement).tagName === "INPUT" || !note) return;
    if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      void remove();
    }
  }
</script>

<svelte:window onkeydown={key} />

{#if shown}
  <div class="detail" class:inline class:from-right={!inline}>
    {#if named}<span class="what">{noteName(shown.pitch).toLowerCase()}</span>{/if}
    <Knob size={28} value={shown.velocity} label={tx("level")} reset={0.8} onchange={(v) => turn({ velocity: Math.max(0.05, v) })} onend={commit} />
    <Knob
      size={28}
      value={(shown.chance ?? 100) / 100}
      label={tx("chance")}
      reset={1}
      text="{shown.chance ?? 100} %"
      onchange={(v) => turn({ chance: Math.max(5, Math.round(v * 20) * 5) })}
      onend={commit}
    />
    <Knob
      size={28}
      value={((shown.repeat ?? 1) - 1) / 3}
      label={tx("repeat")}
      reset={0}
      text="× {shown.repeat ?? 1}"
      onchange={(v) => turn({ repeat: 1 + Math.round(v * 3) })}
      onend={commit}
    />
    <span class="grow"></span>
    <button onclick={remove}>{tx("remove")}</button>
    <button class="plain" aria-label="done" onclick={() => (app.pick = null)}>×</button>
  </div>
{/if}

<style>
  .detail {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 4px 8px;
    border: 1px solid var(--line);
    min-height: 52px;
  }
  /* inside a bar: the bar is the frame */
  .detail.inline {
    flex: 1;
    min-width: 0;
    min-height: 0;
    padding: 0;
    border: 0;
    gap: 12px;
  }
  .detail.inline button {
    flex: none;
    height: 32px;
  }
  .what {
    min-width: 3ch;
    font: var(--ui-strong);
  }
  .grow {
    flex: 1;
  }
</style>
