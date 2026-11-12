// The audio thread: the whole engine, as WebAssembly, inside an AudioWorklet.
// Requests arrive as UTF-8 bytes (this scope has no TextDecoder) and answers
// go back the same way.

const SCOPE = 512; // points of the picture sent to the screen
const DECIMATE = 4; // one point every 4 frames: about 43 ms across

class Mjolnir extends AudioWorkletProcessor {
  constructor(options) {
    super();
    const module = new WebAssembly.Module(options.processorOptions.wasm);
    this.x = new WebAssembly.Instance(module, {}).exports;
    this.x.mj_init(sampleRate);
    this.blocks = 0;
    this.every = Math.max(1, Math.round(sampleRate / 128 / 30)); // status at ~30 Hz
    this.scope = new Float32Array(SCOPE);
    this.at = 0;
    this.phase = 0;
    this.port.onmessage = (e) => {
      const { id, bytes } = e.data;
      const x = this.x;
      const p = x.mj_alloc(bytes.length);
      new Uint8Array(x.memory.buffer, p, bytes.length).set(bytes);
      const n = x.mj_dispatch(p, bytes.length);
      x.mj_free(p, bytes.length);
      const out = new Uint8Array(x.memory.buffer, x.mj_result_ptr(), n).slice();
      this.port.postMessage({ id, bytes: out }, [out.buffer]);
    };
  }

  process(_inputs, outputs) {
    const x = this.x;
    const left = outputs[0][0];
    const right = outputs[0][1] ?? left;
    const n = left.length;
    const v = new Float32Array(x.memory.buffer, x.mj_process(n), n * 2);
    for (let i = 0; i < n; i++) {
      const l = v[2 * i];
      const r = v[2 * i + 1];
      left[i] = l;
      right[i] = r;
      if (++this.phase === DECIMATE) {
        this.phase = 0;
        this.scope[this.at] = (l + r) * 0.5;
        this.at = (this.at + 1) % SCOPE;
      }
    }
    if (++this.blocks >= this.every) {
      this.blocks = 0;
      const len = x.mj_status();
      const status = new Uint8Array(x.memory.buffer, x.mj_result_ptr(), len).slice();
      // the ring unrolled, oldest point first
      const scope = new Float32Array(SCOPE);
      scope.set(this.scope.subarray(this.at));
      scope.set(this.scope.subarray(0, this.at), SCOPE - this.at);
      this.port.postMessage({ status, scope }, [status.buffer, scope.buffer]);
    }
    return true;
  }
}

registerProcessor("mjolnir", Mjolnir);
