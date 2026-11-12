//! Real-time safety: the audio callback (`Engine::process`) must never
//! allocate or free memory, whatever the user does in the meantime. A
//! counting global allocator records every (de)allocation made while the
//! audio "thread" (here: a flag) is inside `process`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use mjolnir_engine::controller::Controller;
use mjolnir_engine::engine::Engine;
use mjolnir_engine::model::*;

struct Counting;

// Per-thread, so tests running in parallel cannot disturb each other.
thread_local! {
    static IN_AUDIO: Cell<bool> = const { Cell::new(false) };
    static AUDIO_ALLOCS: Cell<usize> = const { Cell::new(0) };
}

fn record() {
    if IN_AUDIO.try_with(|f| f.get()).unwrap_or(false) {
        let _ = AUDIO_ALLOCS.try_with(|c| c.set(c.get() + 1));
    }
}

fn allocs() -> usize {
    AUDIO_ALLOCS.with(|c| c.get())
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record();
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record();
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record();
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn audio(engine: &mut Engine, buf: &mut [f32], blocks: usize) {
    for _ in 0..blocks {
        IN_AUDIO.with(|f| f.set(true));
        engine.process(buf, 2);
        IN_AUDIO.with(|f| f.set(false));
    }
}

#[test]
fn audio_callback_never_allocates() {
    let (mut engine, ep) = Engine::new(48000.0);
    let mut c = Controller::new(ep, 48000.0);
    let mut buf = vec![0.0f32; 2 * 256];

    // session: launch, play, pads, live notes on every instrument type
    c.launch_scene(1).unwrap();
    c.play();
    audio(&mut engine, &mut buf, 50);
    for t in 0..c.project.tracks.len() {
        for pad in 0..8 {
            c.pad_on(t, pad, 0.9).unwrap();
            audio(&mut engine, &mut buf, 2);
            c.pad_off(t, pad).unwrap();
        }
    }
    // swap every instrument (old ones must leave through the garbage queue)
    for preset in SynthPreset::ALL {
        c.swap_instrument(2, InstrumentSpec::Synth { preset })
            .unwrap();
        c.pad_on(2, 0, 1.0).unwrap();
        audio(&mut engine, &mut buf, 20);
    }
    for kit in DrumKitId::ALL {
        c.swap_instrument(0, InstrumentSpec::Drums { kit }).unwrap();
        audio(&mut engine, &mut buf, 20);
    }
    for texture in TextureKind::ALL {
        c.swap_instrument(
            3,
            InstrumentSpec::Granular {
                texture,
                sample: None,
            },
        )
        .unwrap();
        c.pad_on(3, 0, 1.0).unwrap();
        audio(&mut engine, &mut buf, 40);
    }
    // edit patterns while they play, macros, automation, mixer, master
    c.toggle_step(0, 1, 3, DRUM_BASE_NOTE + 2, 1.0).unwrap();
    c.set_pattern_bars(0, 1, 2).unwrap();
    c.set_macro(1, MacroParam::Cutoff, 0.3).unwrap();
    c.set_automation(
        1,
        1,
        MacroParam::Cutoff,
        vec![
            AutoPoint {
                tick: 0,
                value: 0.1,
            },
            AutoPoint {
                tick: 300,
                value: 0.9,
            },
        ],
    )
    .unwrap();
    c.set_pan(1, -0.5).unwrap();
    c.set_solo(2, true).unwrap();
    c.set_master(Master {
        volume: 0.7,
        vinyl: 0.5,
        tape: 0.5,
        warmth: 0.5,
        ..Master::default()
    });
    c.set_humanize(0, 1.0).unwrap();
    audio(&mut engine, &mut buf, 200);
    // tracks come and go, undo/redo
    c.checkpoint();
    c.add_track(
        TrackKind::Melodic,
        InstrumentSpec::Synth {
            preset: SynthPreset::Pad,
        },
    )
    .unwrap();
    audio(&mut engine, &mut buf, 20);
    c.undo().unwrap();
    audio(&mut engine, &mut buf, 20);
    c.redo().unwrap();
    c.remove_track(4).unwrap();
    audio(&mut engine, &mut buf, 20);
    // song mode, seeking, fades, end of song
    c.set_song_fades(1, 1);
    c.set_song_mode(true);
    c.seek(10);
    audio(&mut engine, &mut buf, 400);
    c.set_song_looping(false);
    audio(&mut engine, &mut buf, 2000);
    c.stop();
    audio(&mut engine, &mut buf, 20);
    c.poll();
    // Pre-roll and its transition to playback must also stay allocation-free.
    c.count_in_bars = 1;
    c.record_notes = true;
    c.play();
    audio(&mut engine, &mut buf, 600);
    c.stop();
    audio(&mut engine, &mut buf, 1);

    assert_eq!(allocs(), 0, "the audio callback allocated or freed memory");
}

#[test]
fn the_counter_catches_allocations() {
    // sanity check of the harness itself
    IN_AUDIO.with(|f| f.set(true));
    let v: Vec<u64> = std::hint::black_box(vec![1, 2, 3]);
    drop(v);
    IN_AUDIO.with(|f| f.set(false));
    assert_eq!(allocs(), 2, "one allocation and one free");
}
