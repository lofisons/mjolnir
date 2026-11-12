//! What each part of the audio path costs, in nanoseconds per sample.
use std::time::Instant;

use mjolnir_engine::fx::{FxChain, MasterBus};
use mjolnir_engine::instruments::Instrument;
use mjolnir_engine::macro_synth::{MacroSynth, PRESETS};
use mjolnir_engine::model::{DrumKitId, InstrumentSpec, Macros, SynthPreset, TextureKind};
use mjolnir_engine::space::{Plate, Punch, Space};

fn time(name: &str, n: usize, mut f: impl FnMut()) {
    let t = Instant::now();
    f();
    println!(
        "{name:34} {:7.1} ns/sample",
        t.elapsed().as_nanos() as f64 / n as f64
    );
}

fn main() {
    let sr = 48000.0;
    let n = 48000 * 10;
    let mut buf = vec![0.0f32; 16];
    let chunks = n / 16;

    let mut fx = FxChain::new(sr, &Macros::default());
    let (mut l, mut r, mut a, mut b) = ([0.0f32; 16], [0.0f32; 16], [0.0f32; 16], [0.0f32; 16]);
    let mut m = Macros::default();
    time("track chain, everything open", n, || {
        for _ in 0..chunks {
            fx.process(&buf, &mut l, &mut r, &mut a, &mut b, &m);
        }
    });
    m.0 = [0.6, 0.3, 0.3, 0.3, 0.4, 0.3, 0.8];
    time("track chain, every knob up", n, || {
        for _ in 0..chunks {
            fx.process(&buf, &mut l, &mut r, &mut a, &mut b, &m);
        }
    });
    let mut master = MasterBus::new(sr);
    master.set(mjolnir_engine::model::Master {
        volume: 0.8,
        vinyl: 0.3,
        tape: 0.4,
        warmth: 0.5,
        space: 0.6,
        pump: 0.3,
    });
    time("master bus", n, || {
        for i in 0..n {
            std::hint::black_box(master.process((i as f32 * 0.01).sin() * 0.3, 0.1));
        }
    });
    let mut space = Space::new(sr);
    time("room and echo", n, || {
        for i in 0..n {
            std::hint::black_box(space.process((i as f32 * 0.01).sin() * 0.1, 0.05, 20000.0));
        }
    });
    let mut plate = Plate::new(sr);
    time("  of which the plate", n, || {
        for i in 0..n {
            std::hint::black_box(plate.process((i as f32 * 0.01).sin() * 0.1));
        }
    });
    let mut punch = Punch::new(sr);
    time("punch at rest", n, || {
        for i in 0..n {
            std::hint::black_box(punch.process((i as f32 * 0.01).sin() * 0.1, 0.0, 24000.0));
        }
    });

    let mut inst = |name: &str, spec: InstrumentSpec, notes: &[u8]| {
        let mut i = Instrument::build(&spec, None, sr);
        for p in notes {
            i.note_on(*p, 0.8);
        }
        time(name, n, || {
            for k in 0..chunks {
                if k % 3000 == 0 {
                    for p in notes {
                        i.note_on(*p, 0.8);
                    }
                }
                buf.fill(0.0);
                i.render(&mut buf);
            }
        });
    };
    inst(
        "shadow kit, 3 hits",
        InstrumentSpec::Drums {
            kit: DrumKitId::Shadow,
        },
        &[36, 37, 39],
    );
    inst(
        "lo-fi kit (synth), 3 hits",
        InstrumentSpec::Drums {
            kit: DrumKitId::LoFi,
        },
        &[36, 37, 39],
    );
    inst(
        "soft piano, 4 notes",
        InstrumentSpec::Synth {
            preset: SynthPreset::SoftPiano,
        },
        &[57, 60, 64, 69],
    );
    inst(
        "strings, 4 notes",
        InstrumentSpec::Synth {
            preset: SynthPreset::Strings,
        },
        &[57, 60, 64, 69],
    );
    inst(
        "tape pad, 4 notes",
        InstrumentSpec::Synth {
            preset: SynthPreset::Pad,
        },
        &[57, 60, 64, 69],
    );
    inst(
        "shadow air, 3 notes",
        InstrumentSpec::Granular {
            texture: TextureKind::ShadowAir,
            sample: None,
        },
        &[57, 60, 64],
    );
    inst(
        "silence (shadow air, no note)",
        InstrumentSpec::Granular {
            texture: TextureKind::ShadowAir,
            sample: None,
        },
        &[],
    );
    for p in PRESETS.iter() {
        let mut s = MacroSynth::new(p.patch, sr);
        let base = (p.octave as u8 + 1) * 12;
        time(&format!("macro {} (4 notes)", p.id), n, || {
            for k in 0..chunks {
                if k % 3000 == 0 {
                    for d in [9u8, 12, 16, 21] {
                        s.note_on(base + d, 0.8);
                    }
                }
                buf.fill(0.0);
                s.render(&mut buf);
            }
        });
    }
}
