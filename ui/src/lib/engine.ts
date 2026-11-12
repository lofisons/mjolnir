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
  await addWorklet(ctx, asset("worklet.js"));
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

/** Load the worklet's code; some embedding pages only accept it as a blob or as data. */
async function addWorklet(ctx: AudioContext, url: string): Promise<void> {
  try {
    await ctx.audioWorklet.addModule(url);
    return;
  } catch {
    /* try the other ways */
  }
  const code = await (await fetch(url)).text();
  const blob = URL.createObjectURL(new Blob([code], { type: "text/javascript" }));
  try {
    await ctx.audioWorklet.addModule(blob);
  } catch {
    await ctx.audioWorklet.addModule(`data:text/javascript;base64,${btoa(code)}`);
  } finally {
    URL.revokeObjectURL(blob);
  }
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

export interface OutFile {
  name: string;
  data: Uint8Array | string;
  type: string;
}

/** A host that saves files itself (a page embedded in another app), when there is one. */
interface HostSaver {
  save(r: { filename: string; data: Blob | string }): Promise<unknown>;
}
async function hostSaver(): Promise<HostSaver | null> {
  const host = (window as { claude?: { use?: (name: string) => Promise<unknown> } }).claude;
  if (!host?.use) return null;
  return ((await host.use("downloads").catch(() => null)) as HostSaver | null) ?? null;
}

/**
 * Hand files to the person. In a plain browser each one downloads. An
 * embedding host only takes a few formats: there a project goes as `.json`
 * and audio travels inside one `.zip`.
 */
export async function deliver(files: OutFile[], bundle: string): Promise<void> {
  const host = await hostSaver();
  if (!host) {
    for (const f of files) {
      const url = URL.createObjectURL(new Blob([f.data as BlobPart], { type: f.type }));
      const a = document.createElement("a");
      a.href = url;
      a.download = f.name;
      a.click();
      setTimeout(() => URL.revokeObjectURL(url), 10_000);
    }
    return;
  }
  try {
    if (files.length === 1 && files[0].type === "application/json") {
      await host.save({ filename: `${files[0].name}.json`, data: files[0].data as string });
    } else {
      await host.save({ filename: `${bundle}.zip`, data: zip(files) });
    }
  } catch (e) {
    const code = (e as { code?: string }).code;
    throw code === "declined" ? "not saved → the file was declined" : `could not save here (${code ?? "unavailable"})`;
  }
}

const CRC = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();
function crc32(b: Uint8Array): number {
  let c = 0xffffffff;
  for (let i = 0; i < b.length; i++) c = CRC[(c ^ b[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

/** A zip archive with the files stored as they are (audio does not compress). */
function zip(files: OutFile[]): Blob {
  const parts: BlobPart[] = [];
  const central: Uint8Array[] = [];
  let offset = 0;
  for (const f of files) {
    const data = typeof f.data === "string" ? enc.encode(f.data) : f.data;
    const name = enc.encode(f.name);
    const crc = crc32(data);
    const head = new DataView(new ArrayBuffer(30));
    head.setUint32(0, 0x04034b50, true);
    head.setUint16(4, 20, true);
    head.setUint16(6, 0x0800, true); // names are UTF-8
    head.setUint32(14, crc, true);
    head.setUint32(18, data.length, true);
    head.setUint32(22, data.length, true);
    head.setUint16(26, name.length, true);
    parts.push(head.buffer, name as BlobPart, data as BlobPart);
    const entry = new DataView(new ArrayBuffer(46));
    entry.setUint32(0, 0x02014b50, true);
    entry.setUint16(4, 20, true);
    entry.setUint16(6, 20, true);
    entry.setUint16(8, 0x0800, true);
    entry.setUint32(16, crc, true);
    entry.setUint32(20, data.length, true);
    entry.setUint32(24, data.length, true);
    entry.setUint16(28, name.length, true);
    entry.setUint32(42, offset, true);
    central.push(new Uint8Array(entry.buffer), name);
    offset += 30 + name.length + data.length;
  }
  const size = central.reduce((n, c) => n + c.length, 0);
  const end = new DataView(new ArrayBuffer(22));
  end.setUint32(0, 0x06054b50, true);
  end.setUint16(8, files.length, true);
  end.setUint16(10, files.length, true);
  end.setUint32(12, size, true);
  end.setUint32(16, offset, true);
  return new Blob([...parts, ...(central as BlobPart[]), end.buffer], { type: "application/zip" });
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
