<script lang="ts">
  // The sound face: which sound, its seven macros, and, once opened, every
  // part of the synth as a knob.
  import Knob from "./Knob.svelte";
  import { app, act, live, track, setInstrument, refreshPads, selectTrack } from "../lib/app.svelte";
  import { MACROS, type InstrumentSpec, type ParamSpec } from "../lib/api";

  const t = $derived(track());
  const catalog = $derived(app.info?.catalog);
  const drums = $derived(t?.kind === "drums");
  /** Every sound this kind of track can wear, in the order the arrows step. */
  const choices = $derived.by((): { label: string; spec: InstrumentSpec }[] => {
    if (!catalog) return [];
    if (drums) return catalog.kits.map((k) => ({ label: k.label, spec: { type: "drums", kit: k.id } }));
    return [
      ...catalog.presets.map((k) => ({ label: k.label, spec: { type: "synth", preset: k.id } as InstrumentSpec })),
      ...catalog.textures.map((k) => ({
        label: k.label,
        spec: { type: "granular", texture: k.id, sample: null } as InstrumentSpec,
      })),
    ];
  });
  const current = $derived.by(() => {
    const i = t?.instrument;
    if (!i) return -1;
    return choices.findIndex(
      (c) =>
        (i.type === "drums" && c.spec.type === "drums" && c.spec.kit === i.kit) ||
        (i.type === "synth" && c.spec.type === "synth" && c.spec.preset === i.preset) ||
        (i.type === "granular" && c.spec.type === "granular" && c.spec.texture === i.texture),
    );
  });
  const name = $derived.by(() => {
    const i = t?.instrument;
    if (!i) return "";
    if (i.type === "custom_synth" || i.type === "custom_kit") return i.name;
    return choices[current]?.label ?? i.type;
  });
  /** A texture is a recording played as grains: there is no synth inside to open. */
  const openable = $derived(t?.instrument.type === "drums" || t?.instrument.type === "synth");
  const designed = $derived(t?.instrument.type === "custom_synth" || t?.instrument.type === "custom_kit");

  function step(d: number) {
    if (!choices.length) return;
    const i = (((current < 0 ? 0 : current + d) % choices.length) + choices.length) % choices.length;
    void setInstrument(choices[i].spec);
  }

  const macroValue = (i: number) => app.status?.tracks[app.track]?.macros[i] ?? t?.macros[i] ?? 0;
  const macroLabel = (id: string) => (catalog?.macros.find((m) => m.id === id)?.label ?? id).toLowerCase();

  // the designed sound: its parameters, from the engine's own table
  const specs = $derived((drums ? catalog?.drum_params : catalog?.synth_params) ?? []);
  const groups = $derived.by(() => {
    const out: { name: string; params: ParamSpec[] }[] = [];
    for (const p of specs) {
      const g = out.find((x) => x.name === p.group);
      if (g) g.params.push(p);
      else out.push({ name: p.group, params: [p] });
    }
    return out;
  });
  const patch = $derived.by((): Record<string, number> | null => {
    const i = t?.instrument;
    if (i?.type === "custom_synth") return i.patch;
    if (i?.type === "custom_kit") return i.pads[app.pad] ?? null;
    return null;
  });

  const norm = (p: ParamSpec, v: number) =>
    p.log && p.min > 0 ? Math.log(v / p.min) / Math.log(p.max / p.min) : (v - p.min) / (p.max - p.min);
  function denorm(p: ParamSpec, n: number) {
    let v = p.log && p.min > 0 ? p.min * Math.pow(p.max / p.min, n) : p.min + n * (p.max - p.min);
    if (p.step > 0) v = Math.round(v / p.step) * p.step;
    return Math.min(p.max, Math.max(p.min, v));
  }
  function words(p: ParamSpec, v: number) {
    if (p.choices.length) return (p.choices[Math.round(v - p.min)] ?? "").toLowerCase();
    const n = Math.abs(v) >= 100 ? Math.round(v) : Math.abs(v) >= 10 ? v.toFixed(1) : v.toFixed(2);
    return p.unit ? `${n} ${p.unit}` : `${n}`;
  }
  function turn(p: ParamSpec, n: number) {
    if (!patch) return;
    const v = denorm(p, n);
    patch[p.key] = v;
    live({ type: "set_sound_param", track: app.track, pad: drums ? app.pad : null, key: p.key, value: v });
  }

  async function removeTrack() {
    await act({ type: "remove_track", track: app.track });
    await selectTrack(Math.min(app.track, (app.project?.tracks.length ?? 1) - 1));
  }
</script>

{#if t}
  <div class="sound">
    <div class="pick">
      <button aria-label="previous sound" onclick={() => step(-1)}>←</button>
      <span class="name">{name.toLowerCase()}</span>
      <button aria-label="next sound" onclick={() => step(1)}>→</button>
    </div>

    <div class="knobs">
      {#each MACROS as m, i}
        <Knob
          value={macroValue(i)}
          label={macroLabel(m)}
          reset={m === "cutoff" ? 1 : m === "volume" ? 0.8 : m === "resonance" ? 0.1 : 0}
          onstart={() => live({ type: "touch", track: app.track, param: m, touching: true })}
          onchange={(v) => {
            t.macros[i] = v;
            live({ type: "set_macro", track: app.track, param: m, value: v });
          }}
          onend={() => void act({ type: "touch", track: app.track, param: m, touching: false })}
        />
      {/each}
    </div>

    <div class="tools">
      <button aria-pressed={t.mute} onclick={() => act({ type: "set_mute", track: app.track, mute: !t.mute })}>mute</button>
      <button aria-pressed={t.solo} onclick={() => act({ type: "set_solo", track: app.track, solo: !t.solo })}>solo</button>
      {#if !drums}
        <span class="label">octave</span>
        <button
          aria-label="octave down"
          onclick={async () => {
            await act({ type: "set_octave", track: app.track, octave: t.octave - 1 });
            await refreshPads();
          }}>−</button
        >
        <span>{t.octave}</span>
        <button
          aria-label="octave up"
          onclick={async () => {
            await act({ type: "set_octave", track: app.track, octave: t.octave + 1 });
            await refreshPads();
          }}>+</button
        >
      {/if}
      <span class="grow"></span>
      <button disabled={(app.project?.tracks.length ?? 0) < 2} onclick={removeTrack}>remove track</button>
    </div>

    {#if !designed && openable}
      <div class="tools">
        <button onclick={() => act({ type: "design_sound", track: app.track })}>open the sound</button>
      </div>
    {:else if patch}
      {#if drums}
        <div class="label">pad {String(app.pad + 1).padStart(2, "0")} · {(app.pads[app.pad]?.label ?? "").toLowerCase()}</div>
      {/if}
      {#each groups as g}
        <section>
          <div class="group">{g.name.toLowerCase()}</div>
          <div class="knobs small">
            {#each g.params as p}
              {@const v = patch[p.key] ?? p.min}
              <Knob
                size={36}
                value={norm(p, v)}
                label={p.label.toLowerCase()}
                text={words(p, v)}
                onstart={() => void act({ type: "checkpoint" })}
                onchange={(n) => turn(p, n)}
              />
            {/each}
          </div>
        </section>
      {/each}
    {/if}
  </div>
{/if}

<style>
  .sound {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 8px 16px 16px;
    height: 100%;
    overflow-y: auto;
  }
  .pick {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .pick .name {
    flex: 1;
    font: var(--title);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .knobs {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(64px, 1fr));
    gap: 12px 4px;
  }
  .knobs.small {
    grid-template-columns: repeat(auto-fill, minmax(72px, 1fr));
  }
  .tools {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 8px;
  }
  .grow {
    flex: 1;
  }
  .group {
    font: var(--ui-strong);
    padding-bottom: 4px;
    margin-bottom: 12px;
    border-bottom: 1px solid var(--line);
  }
</style>
