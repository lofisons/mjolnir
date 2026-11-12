<script lang="ts">
  import { tx } from "../lib/i18n.svelte";
  // The export sheet: what to bounce, and into which files.
  import { app, exportAudio, type ExportChoice } from "../lib/app.svelte";

  const hasSong = $derived((app.project?.song.clips.length ?? 0) > 0);
  let choice = $state<ExportChoice>({ scope: "song", mix: true, stems: false, midi: false });
  $effect(() => {
    if (!hasSong) choice.scope = "row";
  });
  const row = $derived((app.project?.scenes[app.section]?.name ?? "").toLowerCase());
</script>

<div class="veil" role="presentation" onclick={() => (app.exporting = false)}>
  <div class="sheet" role="dialog" aria-label="export" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
    <div class="top">
      <span class="title">{tx("export")}</span>
      <button onclick={() => (app.exporting = false)}>{tx("close")}</button>
    </div>
    <div class="line">
      <span class="label">{tx("what")}</span>
      <button aria-pressed={choice.scope === "song"} disabled={!hasSong} onclick={() => (choice.scope = "song")}>{tx("the song")}</button>
      <button aria-pressed={choice.scope === "row"} onclick={() => (choice.scope = "row")}>{tx("row")} {row} {tx("as a loop")}</button>
    </div>
    <div class="line">
      <span class="label">{tx("as")}</span>
      <button aria-pressed={choice.mix} onclick={() => (choice.mix = !choice.mix)}>{tx("the mix")}</button>
      <button aria-pressed={choice.stems} onclick={() => (choice.stems = !choice.stems)}>{tx("one file per track")}</button>
      <button aria-pressed={choice.midi} onclick={() => (choice.midi = !choice.midi)}>midi</button>
    </div>
    <div class="line end">
      <span class="note" role="status">{app.busy ? tx("rendering") : tx(app.note) || "24-bit wav, 44.1 kHz"}</span>
      <button disabled={!!app.busy || !(choice.mix || choice.stems || choice.midi)} onclick={() => exportAudio(choice)}>
        {app.busy ? tx("rendering") : tx("export")}
      </button>
    </div>
  </div>
</div>

<style>
  .veil {
    position: fixed;
    inset: 0;
    z-index: 50;
    background: color-mix(in srgb, var(--bg) 72%, transparent);
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding: 15vh 16px 24px;
  }
  .sheet {
    width: min(480px, 100%);
    background: var(--bg);
    border: 1px solid var(--edge);
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .title {
    font: var(--title);
  }
  .line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
  }
  .line .label {
    width: 40px;
  }
  .end {
    justify-content: space-between;
    gap: 12px;
  }
  .note {
    color: var(--fg-muted);
  }
</style>
