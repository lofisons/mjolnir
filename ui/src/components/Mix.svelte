<script lang="ts">
  // The mix face: every track's level, place, and how much of it goes to the
  // echo and to the room; then the master: the room itself, the pump, and
  // the tape.
  import Knob from "./Knob.svelte";
  import Fader from "./Fader.svelte";
  import { app, act, live, openCell, trackColor } from "../lib/app.svelte";
  import type { MacroParam } from "../lib/api";

  const tracks = $derived(app.project?.tracks ?? []);
  const p = $derived(app.project);
  const M = { volume: 6, reverb: 4, delay: 5 };

  function macro(t: number, param: MacroParam, index: number, v: number) {
    tracks[t].macros[index] = v;
    live({ type: "set_macro", track: t, param, value: v });
  }
  const touch = (t: number, param: MacroParam, on: boolean) =>
    on ? live({ type: "touch", track: t, param, touching: true }) : void act({ type: "touch", track: t, param, touching: false });
  const shown = (t: number, i: number) => app.status?.tracks[t]?.macros[i] ?? tracks[t].macros[i];

  // on a phone a strip is its name, its fader and its gates; the rest opens
  // for one strip at a time
  let more = $state<number | null>(null);

  function master(key: "volume" | "tape" | "vinyl" | "warmth" | "space" | "pump", v: number) {
    if (!p) return;
    p.master[key] = v;
    live({ type: "set_master", master: { ...p.master } });
  }
</script>

{#if p}
  <div class="mix">
    {#each tracks as t, i}
      <div class="strip" class:sel={app.track === i} class:open={more === i} style="--id: {trackColor(t)}">
        <button class="plain name" class:off={t.mute} onclick={() => openCell(i, app.section)} title="open">
          <span class="chip"></span>{t.name.toLowerCase()}
        </button>
        <div class="fade">
          <Fader
            value={shown(i, M.volume)}
            label="{t.name} level"
            reset={0.8}
            level={(app.status?.tracks[i]?.level ?? 0) * 1.6}
            onstart={() => touch(i, "volume", true)}
            onchange={(v) => macro(i, "volume", M.volume, v)}
            onend={() => touch(i, "volume", false)}
          />
        </div>
        <div class="knobs">
          <Knob
            size={32}
            value={(t.pan + 1) / 2}
            label="pan"
            reset={0.5}
            text={Math.abs(t.pan) < 0.02 ? "centre" : `${Math.round(Math.abs(t.pan) * 100)} ${t.pan < 0 ? "left" : "right"}`}
            onchange={(v) => {
              t.pan = v * 2 - 1;
              live({ type: "set_pan", track: i, pan: t.pan });
            }}
          />
          <Knob
            size={32}
            value={shown(i, M.reverb)}
            label="room"
            reset={0}
            onstart={() => touch(i, "reverb", true)}
            onchange={(v) => macro(i, "reverb", M.reverb, v)}
            onend={() => touch(i, "reverb", false)}
          />
          <Knob
            size={32}
            value={shown(i, M.delay)}
            label="echo"
            reset={0}
            onstart={() => touch(i, "delay", true)}
            onchange={(v) => macro(i, "delay", M.delay, v)}
            onend={() => touch(i, "delay", false)}
          />
        </div>
        <div class="gates">
          <button
            class="fx"
            aria-expanded={more === i}
            aria-label="{t.name}: pan, room, echo"
            onclick={() => (more = more === i ? null : i)}>···</button
          >
          <button aria-pressed={t.mute} onclick={() => act({ type: "set_mute", track: i, mute: !t.mute })}>mute</button>
          <button aria-pressed={t.solo} onclick={() => act({ type: "set_solo", track: i, solo: !t.solo })}>solo</button>
        </div>
      </div>
    {/each}

    <div class="strip master">
      <span class="name">master</span>
      <div class="fade">
        <Fader
          value={p.master.volume}
          label="master level"
          reset={0.8}
          level={(app.status?.master_level ?? 0) * 1.1}
          onchange={(v) => master("volume", v)}
        />
      </div>
      <div class="knobs wide">
        <Knob size={32} value={p.master.space} label="room size" reset={0.6} onchange={(v) => master("space", v)} />
        <Knob size={32} value={p.master.pump} label="pump" reset={0} onchange={(v) => master("pump", v)} />
        <Knob size={32} value={p.master.tape} label="tape" reset={0} onchange={(v) => master("tape", v)} />
        <Knob size={32} value={p.master.vinyl} label="vinyl" reset={0} onchange={(v) => master("vinyl", v)} />
        <Knob size={32} value={p.master.warmth} label="warmth" reset={0} onchange={(v) => master("warmth", v)} />
      </div>
    </div>
  </div>
{/if}

<style>
  .mix {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow-y: auto;
    padding: 0 16px 12px;
  }
  .strip {
    display: grid;
    grid-template-columns: minmax(72px, 0.9fr) minmax(88px, 2fr) auto auto;
    align-items: center;
    gap: 4px 12px;
    padding: 8px 0;
    border-bottom: 1px solid var(--line);
  }
  .strip.sel .name {
    color: var(--fg);
  }
  .name {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0;
    text-align: left;
    color: var(--fg-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .name.off {
    color: var(--fg-faint);
  }
  .chip {
    width: 6px;
    height: 6px;
    flex: none;
    background: var(--id);
  }
  .knobs {
    display: flex;
    gap: 6px;
  }
  .gates {
    display: flex;
    gap: 4px;
  }
  .gates button {
    padding: 0 8px;
  }
  .fx {
    display: none;
  }
  .master {
    grid-template-columns: minmax(72px, 0.9fr) minmax(88px, 2fr);
    border-bottom: 0;
  }
  .master .name {
    color: var(--fg);
    font: var(--ui-strong);
  }
  .knobs.wide {
    grid-column: 1 / -1;
    gap: 10px 14px;
    flex-wrap: wrap;
  }
  /* a phone: the name and the gates, then the fader, then the knobs */
  @media (max-width: 620px) {
    .strip {
      grid-template-columns: minmax(0, 1fr) auto;
      grid-template-areas: "name gates" "fade fade" "knobs knobs";
    }
    .strip .name {
      grid-area: name;
    }
    .strip .fade {
      grid-area: fade;
    }
    .strip .knobs {
      grid-area: knobs;
      justify-content: flex-start;
      gap: 14px;
    }
    .strip .gates {
      grid-area: gates;
    }
    .fx {
      display: inline-flex;
      align-items: center;
      letter-spacing: 0.1em;
    }
    .strip:not(.master):not(.open) {
      grid-template-areas: "name gates" "fade fade";
    }
    .strip:not(.master):not(.open) .knobs {
      display: none;
    }
    .master {
      grid-template-areas: "name name" "fade fade" "knobs knobs";
    }
  }
</style>
