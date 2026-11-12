// The door to the engine: an AudioWorklet running the WebAssembly module.
// `call` sends one action and resolves with its answer.

import type { Action, Project, Status } from "./api";

const enc = new TextEncoder();
const dec = new TextDecoder();
const asset = (name: string) => new URL(name, document.baseURI).href;

let ctx: AudioContext | null = null;
let node: AudioWorkletNode | null = null;
let wasm: ArrayBuffer | null = null;
let next = 1;
const waiting = new Map<number, { ok: (v: unknown) => void; err: (e: string) => void }>();
let listener: (s: Status, scope: Float32Array) => void = () => {};

/** Load the engine. The sound itself waits for the first touch (`wake`). */
export async function boot(): Promise<void> {
  wasm = await (await fetch(asset("mjolnir.wasm"))).arrayBuffer();
  // iOS: play even with the ring switch on silent
  const session = (navigator as { audioSession?: { type: string } }).audioSession;
  if (session) session.type = "playback";
  ctx = new AudioContext({ latencyHint: "interactive" });
  await ctx.audioWorklet.addModule(asset("worklet.js"));
  node = new AudioWorkletNode(ctx, "mjolnir", {
    numberOfInputs: 0,
    numberOfOutputs: 1,
    outputChannelCount: [2],
    processorOptions: { wasm },
  });
  node.port.onmessage = (e) => {
    const m = e.data;
    if (m.status) {
      listener(JSON.parse(dec.decode(m.status)), m.scope);
      return;
    }
    const w = waiting.get(m.id);
    if (!w) return;
    waiting.delete(m.id);
    const reply = JSON.parse(dec.decode(m.bytes));
    if ("err" in reply) w.err(reply.err);
    else w.ok(reply.ok);
  };
  node.connect(ctx.destination);
}

/** Browsers only let sound start from a gesture: call this on every one. */
export function wake(): void {
  if (ctx && ctx.state !== "running") void ctx.resume();
}

export const audioRunning = () => ctx?.state === "running";
export const sampleRate = () => ctx?.sampleRate ?? 48000;
/** What the browser reports between the engine and the speaker, in ms. */
export const latencyMs = () =>
  ctx ? Math.round(((ctx.baseLatency ?? 0) + (ctx.outputLatency ?? 0)) * 1000) : 0;

export function call<T = Project>(action: Action): Promise<T> {
  return new Promise<T>((ok, err) => {
    if (!node) return err("the engine is not loaded");
    const id = next++;
    waiting.set(id, { ok: ok as (v: unknown) => void, err });
    const bytes = enc.encode(JSON.stringify(action));
    node.port.postMessage({ id, bytes }, [bytes.buffer]);
  });
}

/** Transport, meters and the picture of the signal, about 30 times a second. */
export function onStatus(cb: (s: Status, scope: Float32Array) => void): void {
  listener = cb;
}

/** Bounce a project file offline: `what` is 0 for the mix, -1 for MIDI, n for track n's stem. */
export function render(json: string, what: number[]): Promise<(Uint8Array | null)[]> {
  return new Promise((ok, err) => {
    if (!wasm) return err("the engine is not loaded");
    const worker = new Worker(asset("render-worker.js"));
    worker.onmessage = (e) => {
      worker.terminate();
      ok(e.data.out);
    };
    worker.onerror = (e) => {
      worker.terminate();
      err(e.message);
    };
    worker.postMessage({ wasm, json: enc.encode(json), sampleRate: 44100, what });
  });
}

/** Hand a file to the person. */
export function download(name: string, bytes: Uint8Array | string, type: string): void {
  const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type }));
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
}

/** Ask for one file and read it as text. */
export function pickText(accept: string): Promise<string | null> {
  return new Promise((ok) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = accept;
    input.onchange = async () => ok(input.files?.[0] ? await input.files[0].text() : null);
    input.oncancel = () => ok(null);
    input.click();
  });
}
