//! Stress test: thousands of random user actions interleaved with audio.
//! Nothing may panic, the output must stay finite and bounded, and the
//! project must stay internally consistent. Deterministic (fixed seed).

use mjolnir_engine::controller::Controller;
use mjolnir_engine::engine::Engine;
use mjolnir_engine::model::TICKS_PER_4_4_BAR as TICKS_PER_BAR;
use mjolnir_engine::model::*;
use mjolnir_engine::theory::{ChordKind, Scale, ScaleKind};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn unit(&mut self) -> f32 {
        (self.next() % 10_000) as f32 / 10_000.0
    }
}

fn check_project(p: &Project) {
    let ids: Vec<u32> = p.tracks.iter().map(|t| t.id).collect();
    for s in &p.scenes {
        assert_eq!(s.slots.len(), p.tracks.len(), "scene/track mismatch");
    }
    for c in &p.song.clips {
        assert!(ids.contains(&c.track), "clip on a missing track");
        assert!(c.bars >= 1 && c.slot < SLOTS_PER_TRACK);
    }
    // lanes never overlap
    for &id in &ids {
        let mut lane: Vec<_> = p.song.clips.iter().filter(|c| c.track == id).collect();
        lane.sort_by_key(|c| c.start);
        for w in lane.windows(2) {
            assert!(w[0].end() <= w[1].start, "overlapping clips");
        }
    }
    let rows = p.scenes.len();
    assert!((1..=SLOTS_PER_TRACK).contains(&rows), "section count");
    for t in &p.tracks {
        assert_eq!(t.slots.len(), SLOTS_PER_TRACK);
        assert!(
            t.slots[rows..].iter().all(|s| s.is_none()),
            "loop in a hidden row"
        );
        for pat in t.slots.iter().flatten() {
            let len = pat.length_ticks(TICKS_PER_BAR);
            assert!(pat.notes.iter().all(|n| n.start < len));
            assert!(pat.notes.windows(2).all(|w| w[0].start <= w[1].start));
        }
    }
}

#[test]
fn random_actions_never_break_anything() {
    let (mut engine, ep) = Engine::new(44100.0);
    let mut c = Controller::new(ep, 44100.0);
    // STRESS_SEED=<n> explores other sequences (CI uses the fixed default)
    let seed = std::env::var("STRESS_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0x5eed_1234_abcd_ef01u64);
    let mut rng = Rng(seed | 1);
    let mut buf = vec![0.0f32; 2 * 128];
    let mut peak = 0.0f32;

    for step in 0..5000 {
        let tracks = c.project.tracks.len();
        let t = rng.below(tracks);
        let slot = rng.below(SLOTS_PER_TRACK);
        // errors (empty slot, no tracks…) are fine; panics are not
        let _ = match rng.below(34) {
            0 => {
                c.play();
                Ok(())
            }
            1 => {
                c.stop();
                Ok(())
            }
            2 => c.launch(t, Some(slot)),
            3 => c.launch_scene(rng.below(8)),
            4 => c.pad_on(t, rng.below(16), rng.unit()),
            5 => c.pad_off(t, rng.below(16)),
            6 => c.create_pattern(t, slot, [1, 2, 4, 8][rng.below(4)]),
            7 => c
                .toggle_step(
                    t,
                    slot,
                    rng.below(32) as u32,
                    36 + rng.below(40) as u8,
                    rng.unit(),
                )
                .map(|_| ()),
            8 => c.set_pattern_bars(t, slot, [1, 2, 4, 8][rng.below(4)]),
            9 => c.delete_pattern(t, slot),
            10 => c.set_macro(t, MacroParam::ALL[rng.below(7)], rng.unit()),
            11 => {
                c.set_bpm(40.0 + rng.unit() * 200.0);
                Ok(())
            }
            12 => c.set_scale(
                Scale::new(rng.below(12) as u8, ScaleKind::ALL[rng.below(13)]),
                None,
            ),
            13 => {
                let spec = match rng.below(3) {
                    0 => InstrumentSpec::Synth {
                        preset: SynthPreset::ALL[rng.below(SynthPreset::ALL.len())],
                    },
                    1 => InstrumentSpec::Drums {
                        kit: DrumKitId::ALL[rng.below(DrumKitId::ALL.len())],
                    },
                    _ => InstrumentSpec::Granular {
                        texture: TextureKind::ALL[rng.below(TextureKind::ALL.len())],
                        sample: None,
                    },
                };
                c.swap_instrument(t, spec)
            }
            14 if tracks < 10 => c
                .add_track(
                    TrackKind::Melodic,
                    InstrumentSpec::Synth {
                        preset: SynthPreset::Keys,
                    },
                )
                .map(|_| ()),
            15 if tracks > 1 => c.remove_track(t),
            16 => c
                .add_clip(t, slot, rng.below(40) as u32, Some(1 + rng.below(8) as u32))
                .map(|_| ()),
            17 if !c.project.song.clips.is_empty() => {
                let i = rng.below(c.project.song.clips.len());
                c.edit_clip(i, t, rng.below(40) as u32, 1 + rng.below(8) as u32)
                    .map(|_| ())
            }
            18 if !c.project.song.clips.is_empty() => {
                c.delete_clip(rng.below(c.project.song.clips.len()))
            }
            19 => {
                let on = rng.below(2) == 0;
                c.set_song_mode(on);
                Ok(())
            }
            20 => {
                c.seek(rng.below(40) as u32);
                Ok(())
            }
            21 => {
                c.checkpoint();
                Ok(())
            }
            22 => c.undo(),
            23 => c.redo(),
            24 => c.capture(t, None).map(|_| ()),
            25 => c.set_chord_mode(
                t,
                [None, Some(ChordKind::Triad), Some(ChordKind::Seventh)][rng.below(3)],
            ),
            26 => c.set_automation(
                t,
                slot,
                MacroParam::Cutoff,
                (0..4)
                    .map(|k| AutoPoint {
                        tick: k * 96,
                        value: rng.unit(),
                    })
                    .collect(),
            ),
            27 => {
                c.record_notes = rng.below(2) == 0;
                c.record_song = rng.below(2) == 0;
                c.record_automation = rng.below(2) == 0;
                Ok(())
            }
            28 => c.set_humanize(t, rng.unit()),
            29 => {
                c.set_song_fades(rng.below(4) as u32, rng.below(4) as u32);
                Ok(())
            }
            30 => c.append_scene(rng.below(8), None),
            31 => c.add_section(None).map(|_| ()),
            32 => c.delete_section(rng.below(c.project.scenes.len() + 1)),
            _ => c.set_solo(t, rng.below(4) == 0),
        };
        for _ in 0..1 + rng.below(4) {
            engine.process(&mut buf, 2);
            for &x in &buf {
                assert!(x.is_finite(), "non-finite sample at step {step}");
                peak = peak.max(x.abs());
            }
        }
        c.poll();
        if step % 50 == 0 {
            check_project(&c.project);
        }
    }
    check_project(&c.project);
    assert!(peak <= 1.0, "output exceeded full scale: {peak}");
    // and it still saves and opens
    let json = c.save_json().unwrap();
    c.open_json(&json).unwrap();
}
