<script lang="ts">
  import { tx, lang, setLang } from "../lib/i18n.svelte";
  // The first time Mjolnir opens: two ways in, and nothing else to read.
  import { app, newSet, playRow } from "../lib/app.svelte";

  let busy = $state(false);
  // asked once
  $effect(() => {
    try {
      localStorage.setItem("mjolnir.welcomed", "1");
    } catch {
      /* private window */
    }
  });
  async function blank() {
    busy = true;
    await newSet();
    app.welcome = false;
  }
  async function listen() {
    app.welcome = false;
    // the finished piece is already open: let it be heard
    await playRow(2);
  }
</script>

<div class="veil">
  <div class="sheet" role="dialog" aria-modal="true" aria-label="mjolnir">
    <div class="name">mjolnir</div>
    <p class="say">{tx("An instrument for sketching beats.")}</p>
    <div class="ways">
      <button class="way primary" disabled={busy} onclick={blank}>
        <span class="do">{tx("start a beat")}</span>
        <span class="what">{tx("an empty set: drums, a bass, an air and a piano, ready")}</span>
      </button>
      <button class="way" disabled={busy} onclick={listen}>
        <span class="do">{tx("listen first")}</span>
        <span class="what">{tx("a finished piece, to hear what it does and take apart")}</span>
      </button>
    </div>
    <div class="foot">
      <span class="label">{tx("language")}</span>
      <button class="mini" aria-pressed={lang.code === "en"} onclick={() => setLang("en")}>english</button>
      <button class="mini" aria-pressed={lang.code === "es"} onclick={() => setLang("es")}>español</button>
    </div>
  </div>
</div>

<style>
  .veil {
    position: fixed;
    inset: 0;
    z-index: 55;
    background: var(--bg);
    display: grid;
    place-items: center;
    padding: 24px 16px;
    overflow-y: auto;
  }
  .sheet {
    width: min(440px, 100%);
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .name {
    font: var(--ui-strong);
  }
  .say {
    margin: 0 0 8px;
    font: var(--title);
    max-width: 16ch;
  }
  .ways {
    display: grid;
    gap: 8px;
  }
  .way {
    height: auto;
    padding: 14px 16px;
    display: grid;
    gap: 2px;
    text-align: left;
    white-space: normal;
  }
  .way.primary {
    background: var(--fg);
    border-color: var(--fg);
    color: var(--bg);
  }
  .do {
    font: var(--ui-strong);
  }
  .what {
    opacity: 0.72;
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 8px;
  }
  .mini {
    height: 28px;
    padding: 0 8px;
    border-color: var(--line);
  }
</style>
