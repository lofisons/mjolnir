//! Measure every macro sound: level, peak and the pitch it really plays.
//! `cargo run --release -p mjolnir-engine --example sounds`
use mjolnir_engine::macro_synth::{MacroSynth, ENGINES, PRESETS};

fn main() {
    let sr = 48000.0;
    println!(
        "{:14} {:10} {:>8} {:>7} {:>8} {:>8}",
        "sound", "engine", "rms dB", "peak", "Hz", "cents"
    );
    for p in PRESETS.iter() {
        let mut s = MacroSynth::new(p.patch, sr);
        let pitch = (p.octave as u8 + 1) * 12 + 9; // the A of its home octave
        s.note_on(pitch, 0.8);
        let mut buf = vec![0.0f32; 48000 * 2];
        for c in buf.chunks_mut(16) {
            s.render(c);
        }
        let body = &buf[4800..];
        let rms = (body.iter().map(|x| x * x).sum::<f32>() / body.len() as f32).sqrt();
        let peak = buf.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        // pitch by autocorrelation around the expected period
        let want = 440.0 * 2f32.powf((pitch as f32 - 69.0) / 12.0);
        let seg = &buf[9600..9600 + 16384];
        let (mut best, mut best_lag) = (f32::MIN, 0.0f32);
        let centre = sr / want;
        let mut lag = centre * 0.94;
        while lag < centre * 1.06 {
            let mut acc = 0.0;
            for i in 0..8000 {
                let j = i as f32 + lag;
                let k = j as usize;
                let f = j - k as f32;
                acc += seg[i] * (seg[k] + (seg[k + 1] - seg[k]) * f);
            }
            if acc > best {
                best = acc;
                best_lag = lag;
            }
            lag += 0.02;
        }
        let hz = sr / best_lag;
        println!(
            "{:14} {:10} {:8.1} {:7.2} {:8.1} {:8.0}",
            p.id,
            ENGINES[p.patch.engine as usize],
            20.0 * rms.max(1e-9).log10(),
            peak,
            hz,
            1200.0 * (hz / want).log2()
        );
    }
}
