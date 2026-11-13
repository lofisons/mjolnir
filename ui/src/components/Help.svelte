<script lang="ts">
  // How to play: one sheet, the few things worth knowing.
  import { app } from "../lib/app.svelte";
  import { tx, lang } from "../lib/i18n.svelte";

  type Parts = [string, [string, string][]][];
  const EN: Parts = [
    [
      "loops",
      [
        ["click a loop", "play it; again to stop it"],
        ["double click", "step inside"],
        ["click +", "step into an empty loop"],
        ["a letter on the left", "play the whole row"],
      ],
    ],
    [
      "inside a track",
      [
        ["pads, keys", "play; the higher on a pad, the harder. rec keeps what you play"],
        ["capture", "keep what you just played, even if nothing was recording"],
        ["a step", "click to switch; hold (or right click) to see its level, chance and repeats"],
        ["the roll", "click to write, drag to move, the right edge to stretch, click a note to pick it"],
        ["chords", "every note becomes the chord of the key"],
        ["loop", "copy, paste, snap to the grid, clear"],
      ],
    ],
    [
      "the whole mix",
      [
        ["fx", "eight effects that sound while held (shift 1 to 8)"],
        ["mix", "levels, place, room and echo; the pump makes room for the kick"],
        ["timeline", "click a lane to place the loop in hand; drag clips to move them"],
        ["export", "the timeline or one row; the mix, one file per track, midi"],
      ],
    ],
    [
      "keys",
      [
        ["space", "play, stop"],
        ["z x c v · a s d f · q w e r · 1 2 3 4", "the sixteen pads"],
        ["a w s e d f t g y h u j k", "a piano, in a melodic track"],
        ["esc", "back to the loops"],
        ["tab", "the next face"],
        ["ctrl z, ctrl shift z", "undo, redo"],
        ["delete", "remove what is picked"],
        ["?", "this sheet"],
      ],
    ],
  ];
  const ES: Parts = [
    [
      "loops",
      [
        ["clic en un loop", "lo toca; otro clic lo para"],
        ["doble clic", "entra en él"],
        ["clic en +", "entra en un loop vacío"],
        ["una letra a la izquierda", "toca toda la fila"],
      ],
    ],
    [
      "dentro de una pista",
      [
        ["pads, teclas", "tocan; cuanto más arriba en el pad, más fuerte. rec conserva lo que tocas"],
        ["capturar", "conserva lo que acabas de tocar, aunque no estuviera grabando"],
        ["un paso", "clic para cambiarlo; mantén (o clic derecho) para ver su nivel, probabilidad y repeticiones"],
        ["el roll", "clic para escribir, arrastra para mover, el borde derecho para estirar, clic en una nota para elegirla"],
        ["acordes", "cada nota se vuelve el acorde de la tonalidad"],
        ["loop", "copiar, pegar, ajustar a la rejilla, vaciar"],
      ],
    ],
    [
      "toda la mezcla",
      [
        ["fx", "ocho efectos que suenan mientras los mantienes (shift 1 a 8)"],
        ["mix", "niveles, lugar, sala y eco; el pump hace sitio al bombo"],
        ["timeline", "clic en un carril para poner el loop en mano; arrastra los clips para moverlos"],
        ["exportar", "el timeline o una fila; la mezcla, un archivo por pista, midi"],
      ],
    ],
    [
      "teclas",
      [
        ["espacio", "play, stop"],
        ["z x c v · a s d f · q w e r · 1 2 3 4", "los dieciséis pads"],
        ["a w s e d f t g y h u j k", "un piano, en una pista melódica"],
        ["esc", "vuelve a los loops"],
        ["tab", "la siguiente cara"],
        ["ctrl z, ctrl shift z", "deshacer, rehacer"],
        ["supr", "quita lo elegido"],
        ["?", "esta hoja"],
      ],
    ],
  ];
  const PARTS = $derived(lang.code === "es" ? ES : EN);
</script>

<div class="veil" role="presentation" onclick={() => (app.help = false)}>
  <div class="sheet" role="dialog" aria-label="how to play" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
    <div class="top">
      <span class="title">{tx("how to play")}</span>
      <button onclick={() => (app.help = false)}>{tx("close")}</button>
    </div>
    {#each PARTS as [name, rows]}
      <section>
        <div class="group">{name}</div>
        {#each rows as [what, does]}
          <div class="row"><span class="what">{what}</span><span class="does">{does}</span></div>
        {/each}
      </section>
    {/each}
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
    padding: 24px 16px;
    overflow-y: auto;
  }
  .sheet {
    width: min(560px, 100%);
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
  .group {
    font: var(--ui-strong);
    padding-bottom: 4px;
    margin-bottom: 4px;
    border-bottom: 1px solid var(--line);
  }
  .row {
    display: grid;
    grid-template-columns: minmax(0, 0.9fr) minmax(0, 1.4fr);
    gap: 12px;
    padding: 3px 0;
  }
  .does {
    color: var(--fg-muted);
  }
</style>
