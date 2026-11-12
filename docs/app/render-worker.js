// Offline bounce, off the audio thread: its own copy of the engine renders a
// project file to WAV or MIDI bytes.
self.onmessage = (e) => {
  const { wasm, json, sampleRate, what } = e.data;
  const x = new WebAssembly.Instance(new WebAssembly.Module(wasm), {}).exports;
  x.mj_init(sampleRate);
  const p = x.mj_alloc(json.length);
  new Uint8Array(x.memory.buffer, p, json.length).set(json);
  const out = what.map((w) => {
    const n = x.mj_render(p, json.length, sampleRate, w);
    return n ? new Uint8Array(x.memory.buffer, x.mj_result_ptr(), n).slice() : null;
  });
  self.postMessage({ out });
};
