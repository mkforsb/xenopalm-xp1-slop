// AudioWorkletProcessor hosting the Xenopalm DSP (xenopalm-worklet.wasm).
//
// The main thread compiles the wasm module and hands it over through
// processorOptions; the module has no imports, so instantiation is synchronous.
//
// Messages on `port` are small arrays:
//   [0, drum, paramId, value]                        set a voice parameter
//   [1, paramId, value]                              set a global parameter
//   [2, drum, stroke, velocity, position, ornament]  play a note now
//   [3]                                              snap smoothed parameters
//   [4, step, drum, stroke, vel, pos, orn, chance]   write a pattern cell (stroke < 0 clears)
//   [5, on]                                          start/stop the sequencer
//
// Whenever the engine status changes (playhead step, hits, mutated patches)
// the processor posts it back as a Float32Array.

const STATUS_LEN = 66;

class XenopalmProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    this.dsp = new WebAssembly.Instance(options.processorOptions.module, {}).exports;
    this.handle = this.dsp.xp_new(sampleRate);
    this.views = null;
    this.lastStatus = new Float32Array(STATUS_LEN);
    this.port.onmessage = (e) => this.onMessage(e.data);
  }

  onMessage(m) {
    const d = this.dsp;
    const h = this.handle;
    switch (m[0]) {
      case 0: d.xp_set_param(h, m[1], m[2], m[3]); break;
      case 1: d.xp_set_global(h, m[1], m[2]); break;
      case 2: d.xp_live(h, m[1], m[2], m[3], m[4], m[5]); break;
      case 3: d.xp_snap(h); break;
      case 4: d.xp_set_note(h, m[1], m[2], m[3], m[4], m[5], m[6], m[7]); break;
      case 5: d.xp_play(h, m[1] ? 1 : 0); break;
    }
  }

  // (Re)create the Float32Array views if wasm memory was replaced by a grow.
  outputViews(frames) {
    const buffer = this.dsp.memory.buffer;
    const v = this.views;
    if (v === null || v.buffer !== buffer || v.frames !== frames) {
      this.views = {
        buffer,
        frames,
        left: new Float32Array(buffer, this.dsp.xp_left(this.handle), frames),
        right: new Float32Array(buffer, this.dsp.xp_right(this.handle), frames),
        status: new Float32Array(buffer, this.dsp.xp_status(this.handle), STATUS_LEN),
      };
    }
    return this.views;
  }

  process(_inputs, outputs) {
    const out = outputs[0];
    const frames = out[0].length;
    this.dsp.xp_render(this.handle, frames);
    const v = this.outputViews(frames);
    out[0].set(v.left);
    if (out.length > 1) out[1].set(v.right);
    let changed = false;
    for (let i = 0; i < STATUS_LEN; i++) {
      if (v.status[i] !== this.lastStatus[i]) { changed = true; break; }
    }
    if (changed) {
      this.lastStatus.set(v.status);
      this.port.postMessage(this.lastStatus.slice());
    }
    return true;
  }
}

registerProcessor("xenopalm", XenopalmProcessor);
