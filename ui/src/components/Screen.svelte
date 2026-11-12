<script lang="ts">
  // The screen: the signal itself, drawn as sand. Every point is a real
  // sample of what is sounding; older frames fade like a long exposure.
  import { onMount } from "svelte";
  import { app, signal, track, playing } from "../lib/app.svelte";

  let canvas: HTMLCanvasElement;
  let box: HTMLDivElement;

  const position = $derived.by(() => {
    const s = app.status;
    if (!s) return "1.1";
    return `${s.bar + 1}.${s.beat + 1}`;
  });
  const sound = $derived.by(() => {
    const t = track();
    if (!t) return "";
    const i = t.instrument;
    const c = app.info?.catalog;
    if (i.type === "drums") return c?.kits.find((k) => k.id === i.kit)?.label ?? i.kit;
    if (i.type === "synth") return c?.presets.find((k) => k.id === i.preset)?.label ?? i.preset;
    if (i.type === "granular") return c?.textures.find((k) => k.id === i.texture)?.label ?? i.texture;
    if (i.type === "custom_synth" || i.type === "custom_kit") return i.name;
    return "sample";
  });

  onMount(() => {
    const g = canvas.getContext("2d")!;
    let w = 0;
    let h = 0;
    let dpr = 1;
    const size = () => {
      dpr = Math.min(2, devicePixelRatio || 1);
      w = box.clientWidth;
      h = box.clientHeight;
      canvas.width = Math.round(w * dpr);
      canvas.height = Math.round(h * dpr);
    };
    size();
    const ro = new ResizeObserver(size);
    ro.observe(box);
    const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
    let raf = 0;
    let seed = 1;
    const rnd = () => ((seed = (seed * 1664525 + 1013904223) >>> 0) / 4294967296);
    const draw = () => {
      raf = requestAnimationFrame(draw);
      const css = getComputedStyle(canvas);
      const bg = css.getPropertyValue("--bg").trim();
      const fg = css.getPropertyValue("--fg").trim();
      const faint = css.getPropertyValue("--fg-faint").trim();
      g.setTransform(dpr, 0, 0, dpr, 0, 0);
      // the long exposure: the last frames stay, each a little fainter
      g.globalAlpha = still ? 1 : 0.22;
      g.fillStyle = bg;
      g.fillRect(0, 0, w, h);
      g.globalAlpha = 1;
      // a ruler: the zero line in ticks
      g.fillStyle = faint;
      for (let x = 0; x < w; x += 8) g.fillRect(x, Math.round(h / 2), 1, 1);
      const s = signal.scope;
      const n = s.length;
      // start on a rising zero crossing so the picture holds still
      let from = 0;
      for (let i = 1; i < n / 2; i++) {
        if (s[i - 1] <= 0 && s[i] > 0) {
          from = i;
          break;
        }
      }
      const span = n / 2;
      const gain = h * 0.42 * 1.6;
      g.fillStyle = fg;
      const grains = Math.max(1, Math.round(w / span) + 1);
      for (let i = 0; i < span; i++) {
        const a = s[from + i];
        const b = s[Math.min(n - 1, from + i + 1)];
        for (let k = 0; k < grains; k++) {
          const t = k / grains;
          const v = a + (b - a) * t;
          const x = ((i + t) / span) * w;
          const y = h / 2 - Math.max(-1, Math.min(1, v * 1.6)) * (gain / 1.6);
          // sand: uneven weight, a few grains fall out
          const r = rnd();
          if (r < 0.17) continue;
          g.globalAlpha = 0.35 + 0.65 * r;
          g.fillRect(x, y, 1, 1);
        }
      }
      g.globalAlpha = 1;
    };
    draw();
    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  });
</script>

<div class="screen" bind:this={box}>
  <canvas bind:this={canvas}></canvas>
  <div class="tl">
    <span class="label">{String(app.track + 1).padStart(2, "0")}</span>
    <span>{sound.toLowerCase()}</span>
  </div>
  <div class="tr">
    <span class="label">bar</span>
    <span>{position}</span>
  </div>
  <div class="bl">
    <span class="led" class:on={playing()} class:rec={app.project?.record.notes}></span>
    <span class:quiet={!playing()}>
      {app.project?.record.notes && playing() ? "recording" : playing() ? "playing" : "stopped"}
    </span>
  </div>
  <div class="br">
    <span>{Math.round(app.project?.bpm ?? 0)}</span>
    <span class="label">bpm</span>
  </div>
</div>

<style>
  .screen {
    position: relative;
    border-bottom: 1px solid var(--line);
    min-height: 72px;
    overflow: hidden;
  }
  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    display: block;
  }
  .tl,
  .tr,
  .bl,
  .br {
    position: absolute;
    display: flex;
    gap: 6px;
    align-items: baseline;
    pointer-events: none;
  }
  .tl {
    top: 8px;
    left: 16px;
  }
  .tr {
    top: 8px;
    right: 16px;
  }
  .bl {
    bottom: 8px;
    left: 16px;
    align-items: center;
  }
  .br {
    bottom: 8px;
    right: 16px;
  }
  .quiet {
    color: var(--fg-muted);
  }
  .led {
    width: 6px;
    height: 6px;
    background: var(--fg-faint);
  }
  .led.on {
    background: var(--gr);
  }
  .led.on.rec {
    background: var(--rd);
  }
</style>
