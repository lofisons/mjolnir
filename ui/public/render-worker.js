// Offline bounce, off the audio thread: its own copy of the engine renders a
// project file to WAV or MIDI bytes. A number asks for the mix (0), the MIDI
// (-1) or a stem (n); a word is a genre: the mix finished by Freya.
self.onmessage = (e) => {
  const { wasm, json, sampleRate, what } = e.data;
  const x = new WebAssembly.Instance(new WebAssembly.Module(wasm), {}).exports;
  x.mj_init(sampleRate);
  const p = x.mj_alloc(json.length);
  new Uint8Array(x.memory.buffer, p, json.length).set(json);
  const read = (n) => new Uint8Array(x.memory.buffer, x.mj_result_ptr(), n).slice();
  // what Freya answered about the mix it finished, when it was asked to
  let freya = null;
  const out = what.map((w) => {
    if (typeof w === "string") {
      // a genre: the mix, finished by Freya
      const g = new TextEncoder().encode(w);
      const gp = x.mj_alloc(g.length);
      new Uint8Array(x.memory.buffer, gp, g.length).set(g);
      const n = x.mj_render_freya(p, json.length, sampleRate, gp, g.length);
      const wav = n ? read(n) : null;
      freya = JSON.parse(new TextDecoder().decode(read(x.mj_freya_report())));
      return wav;
    }
    const n = x.mj_render(p, json.length, sampleRate, w);
    return n ? read(n) : null;
  });
  self.postMessage({ out, freya });
};
