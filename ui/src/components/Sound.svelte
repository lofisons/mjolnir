<script lang="ts">
  import { tx } from "../lib/i18n.svelte";
  // The sound face: which sound, its seven macros, and, once opened, every
  // part of the synth as a knob.
  import Knob from "./Knob.svelte";
  import { app, act, live, track, setInstrument, selectTrack, sampleFromFile, addSample, engine } from "../lib/app.svelte";
  import { MACROS, type InstrumentSpec, type ParamSpec } from "../lib/api";

  const t = $derived(track());
  const catalog = $derived(app.info?.catalog);
  const drums = $derived(t?.kind === "drums");
  interface Choice {
    label: string;
    category: string;
    spec: InstrumentSpec;
    octave?: number;
    open: boolean;
  }
  /** Every sound this kind of track can wear, filed by what it is. */
  const choices = $derived.by((): Choice[] => {
    if (!catalog) return [];
    if (drums)
      return catalog.kits.map((k) => ({
        label: k.label,
        category: k.category ?? "kits",
        spec: { type: "drums", kit: k.id },
        open: k.category === "synth kits",
      }));
    return [
      ...(catalog.macro_sounds ?? []).map((k) => ({
        label: k.label,
        category: k.category ?? "keys",
        spec: { type: "macro", name: k.label, patch: { ...k.patch } } as InstrumentSpec,
        octave: k.octave,
        open: true,
      })),
      ...catalog.presets.map((k) => ({
        label: k.label,
        category: k.category ?? "keys",
        spec: { type: "synth", preset: k.id } as InstrumentSpec,
        octave: k.octave,
        open: k.open ?? false,
      })),
      ...catalog.textures.map((k) => ({
        label: k.label,
        category: k.category ?? "pads",
        spec: { type: "granular", texture: k.id, sample: null } as InstrumentSpec,
        octave: k.octave,
        open: false,
      })),
    ];
  });
  const ORDER = ["from the tracks", "synth kits", "chops", "bass", "keys", "pads", "leads", "yours"];
  const categories = $derived([
    ...[...new Set(choices.map((c) => c.category))].sort((a, b) => ORDER.indexOf(a) - ORDER.indexOf(b)),
    "yours",
  ]);
  const current = $derived.by(() => {
    const i = t?.instrument;
    if (!i) return -1;
    return choices.findIndex(
      (c) =>
        (i.type === "macro" && c.spec.type === "macro" && c.spec.name === i.name) ||
        (i.type === "drums" && c.spec.type === "drums" && c.spec.kit === i.kit) ||
        (i.type === "synth" && c.spec.type === "synth" && c.spec.preset === i.preset) ||
        (i.type === "granular" && c.spec.type === "granular" && c.spec.texture === i.texture),
    );
  });
  const name = $derived.by(() => {
    const i = t?.instrument;
    if (!i) return "";
    if (i.type === "custom_synth" || i.type === "custom_kit" || i.type === "macro") return i.name;
    if (i.type === "sample_melodic" || i.type === "sample_kit" || (i.type === "granular" && i.sample != null)) return t?.name ?? "sample";
    return choices[current]?.label ?? i.type;
  });
  const mine = $derived.by(() => {
    const i = t?.instrument;
    return i?.type === "sample_melodic" || i?.type === "sample_kit" || (i?.type === "granular" && i.sample != null);
  });
  /** Only a synth has parts to open; a recording is what it is. */
  const openable = $derived((choices[current]?.open ?? false) && t?.instrument.type !== "macro");
  const designed = $derived(
    t?.instrument.type === "custom_synth" || t?.instrument.type === "custom_kit" || t?.instrument.type === "macro",
  );

  // the shelf on screen: the current sound's own, until another is picked
  let picked = $state<string | null>(null);
  const shelf = $derived(
    picked && categories.includes(picked) ? picked : mine ? "yours" : (choices[current]?.category ?? categories[0]),
  );
  $effect(() => {
    // another track: back to its own shelf
    void app.track;
    picked = null;
  });

  function choose(c: Choice) {
    void setInstrument(c.spec, c.octave);
  }

  const macroValue = (i: number) => app.status?.tracks[app.track]?.macros[i] ?? t?.macros[i] ?? 0;
  const macroLabel = (id: string) => (catalog?.macros.find((m) => m.id === id)?.label ?? id).toLowerCase();

  // your own sounds: a file, or the microphone
  type Mode = "melodic" | "kit" | "granular";
  let mode = $state<Mode>("melodic");
  $effect(() => {
    mode = drums ? "kit" : "melodic";
  });
  let taking = $state<{ stop: () => Promise<AudioBuffer> } | null>(null);
  const canRecord = typeof navigator !== "undefined" && !!navigator.mediaDevices?.getUserMedia;
  async function recordToggle() {
    if (taking) {
      const t = taking;
      taking = null;
      try {
        await addSample(await t.stop(), mode, "recording", app.track);
      } catch {
        app.note = "the recording could not be read";
      }
      return;
    }
    try {
      taking = await engine.record();
    } catch {
      app.note = "the microphone is not available here → use a file";
    }
  }

  // the designed sound: its parameters, from the engine's own table
  const specs = $derived(
    (t?.instrument.type === "macro" ? catalog?.macro_params : drums ? catalog?.drum_params : catalog?.synth_params) ?? [],
  );
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
    if (i?.type === "custom_synth" || i?.type === "macro") return i.patch;
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
    <div class="name">{name.toLowerCase()}</div>

    <div class="shelves" role="tablist" aria-label="kinds of sound">
      {#each categories as c}
        <button role="tab" aria-selected={shelf === c} onclick={() => (picked = c)}>{tx(c)}</button>
      {/each}
    </div>
    {#if shelf === "yours"}
      <div class="yours">
        <div class="tools">
          <span class="label">{tx("play it")}</span>
          {#if !drums}
            <button aria-pressed={mode === "melodic"} onclick={() => (mode = "melodic")}>{tx("across the keys")}</button>
            <button aria-pressed={mode === "granular"} onclick={() => (mode = "granular")}>{tx("as grains")}</button>
          {:else}
            <button aria-pressed={mode === "kit"} onclick={() => (mode = "kit")}>{tx("sliced on the pads")}</button>
          {/if}
        </div>
        <div class="tools">
          <button onclick={() => sampleFromFile(mode, app.track)}>{tx("from a file")}</button>
          {#if canRecord}
            <button class="lit" aria-pressed={!!taking} onclick={recordToggle}>
              <span class="led" class:rd={!!taking} class:blink={!!taking}></span>{taking ? tx("stop and keep") : tx("record the microphone")}
            </button>
          {/if}
        </div>
        <span class="label">{tx("up to twenty seconds; it replaces this track's sound")}</span>
      </div>
    {:else}
      <div class="shelf" role="listbox" aria-label="sounds">
        {#each choices.filter((c) => c.category === shelf) as c}
          <button role="option" aria-selected={c.label === name} onclick={() => choose(c)}>{c.label.toLowerCase()}</button>
        {/each}
      </div>
    {/if}

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
      <input
        type="text"
        class="tname"
        aria-label="name of the track"
        maxlength="24"
        value={t.name}
        onchange={(e) => act({ type: "rename_track", track: app.track, name: e.currentTarget.value.trim() || name })}
      />
      <button aria-pressed={t.mute} onclick={() => act({ type: "set_mute", track: app.track, mute: !t.mute })}>mute</button>
      <button aria-pressed={t.solo} onclick={() => act({ type: "set_solo", track: app.track, solo: !t.solo })}>solo</button>

      <Knob
        size={28}
        value={t.humanize}
        label="loose"
        reset={0.3}
        onchange={(v) => {
          t.humanize = v;
          live({ type: "set_humanize", track: app.track, amount: v });
        }}
      />
      <span class="grow"></span>
      <button disabled={(app.project?.tracks.length ?? 0) < 2} onclick={removeTrack}>{tx("remove track")}</button>
    </div>

    {#if !designed && openable}
      <div class="tools">
        <button onclick={() => act({ type: "design_sound", track: app.track })}>{tx("open the sound")}</button>
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
  .name {
    font: var(--title);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .shelves {
    display: flex;
    flex-wrap: wrap;
    border-bottom: 1px solid var(--line);
    margin-bottom: -8px;
  }
  .shelves button {
    border: 0;
    color: var(--fg-muted);
    padding: 0 10px;
  }
  .shelves button[aria-selected="true"] {
    background: none;
    color: var(--fg);
    box-shadow: inset 0 -1px 0 var(--fg);
  }
  .shelf {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(128px, 1fr));
    gap: 4px;
  }
  .tname {
    flex: 1 1 120px;
    min-width: 96px;
  }
  .yours {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .lit {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .shelf button {
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
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
