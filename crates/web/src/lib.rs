//! Mjolnir for the browser: the engine and the app behind a plain C interface,
//! so an AudioWorklet can run it with no generated glue.
//!
//! Text crosses as UTF-8 bytes. The caller writes its request into memory from
//! `mj_alloc`, and reads the answer at `mj_result_ptr` for the length returned.
//! Everything runs on the worklet's one thread.

use std::cell::UnsafeCell;

use mjolnir_app::{Action, App};
use mjolnir_engine::controller::Controller;
use mjolnir_engine::engine::Engine;
use mjolnir_engine::export;
use serde_json::{json, Value};

/// The most frames one `mj_process` call renders (a worklet block is 128).
const MAX_FRAMES: usize = 1024;

struct State {
    engine: Engine,
    app: App,
    out: Vec<f32>,
    result: Vec<u8>,
}

struct Slot(UnsafeCell<Option<State>>);
// One thread only: a worklet (or a worker) owns its own instance of the module.
unsafe impl Sync for Slot {}
static STATE: Slot = Slot(UnsafeCell::new(None));

fn state() -> &'static mut State {
    // SAFETY: single-threaded, and no call re-enters another.
    unsafe { (*STATE.0.get()).as_mut().expect("mj_init was not called") }
}

fn answer(bytes: Vec<u8>) -> usize {
    let s = state();
    s.result = bytes;
    s.result.len()
}

fn input<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    // SAFETY: the caller wrote `len` bytes at a pointer from `mj_alloc`.
    unsafe { std::slice::from_raw_parts(ptr, len) }
}

#[no_mangle]
pub extern "C" fn mj_init(sample_rate: f32) {
    let (engine, endpoints) = Engine::new(sample_rate);
    let app = App::new(endpoints, sample_rate);
    // SAFETY: single-threaded.
    unsafe {
        *STATE.0.get() = Some(State {
            engine,
            app,
            out: vec![0.0; MAX_FRAMES * 2],
            result: Vec::new(),
        });
    }
}

#[no_mangle]
pub extern "C" fn mj_alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `ptr` must come from `mj_alloc(len)` and not be used again.
#[no_mangle]
pub unsafe extern "C" fn mj_free(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len.max(1)))
}

#[no_mangle]
pub extern "C" fn mj_result_ptr() -> *const u8 {
    state().result.as_ptr()
}

/// Run one action (JSON). The answer is `{"ok": …}` or `{"err": "…"}`.
///
/// # Safety
/// `ptr` must point at `len` bytes written into memory from `mj_alloc`.
#[no_mangle]
pub unsafe extern "C" fn mj_dispatch(ptr: *const u8, len: usize) -> usize {
    let reply = match serde_json::from_slice::<Action>(input(ptr, len)) {
        Ok(action) => match state().app.handle(action) {
            Ok(v) => json!({ "ok": v }),
            Err(e) => json!({ "err": e }),
        },
        Err(e) => json!({ "err": format!("unknown action: {e}") }),
    };
    answer(serde_json::to_vec(&reply).unwrap_or_default())
}

/// Transport, meters and automated values, as JSON.
#[no_mangle]
pub extern "C" fn mj_status() -> usize {
    let v: Value = state().app.status();
    answer(serde_json::to_vec(&v).unwrap_or_default())
}

/// Render `frames` of interleaved stereo; read them at the returned pointer.
#[no_mangle]
pub extern "C" fn mj_process(frames: usize) -> *const f32 {
    let s = state();
    let n = frames.min(MAX_FRAMES);
    s.engine.process(&mut s.out[..n * 2], 2);
    s.out.as_ptr()
}

/// Bounce a project file (JSON text) offline. `what`: 0 the mix as WAV, -1 the
/// MIDI file, n the stem of track n-1 as WAV. Returns 0 when there is nothing.
/// Meant for a worker's own instance: it blocks until the song is rendered.
///
/// # Safety
/// `ptr` must point at `len` bytes written into memory from `mj_alloc`.
#[no_mangle]
pub unsafe extern "C" fn mj_render(
    ptr: *const u8,
    len: usize,
    sample_rate: u32,
    what: i32,
) -> usize {
    let Ok(text) = std::str::from_utf8(input(ptr, len)) else {
        return 0;
    };
    let (_engine, endpoints) = Engine::new(sample_rate as f32);
    let mut ctrl = Controller::new(endpoints, sample_rate as f32);
    if ctrl.open_json(text).is_err() || ctrl.project.song.length_bars() == 0 {
        return answer(Vec::new());
    }
    let bytes = match what {
        -1 => export::to_midi(&ctrl.project),
        0 => {
            let mix = export::render(&ctrl.project, ctrl.samples(), sample_rate, None, 2.0);
            export::wav_bytes(&mix, sample_rate).unwrap_or_default()
        }
        n => {
            let track = (n - 1) as usize;
            if track >= ctrl.project.tracks.len()
                || export::flatten_track(&ctrl.project, track).is_empty()
            {
                Vec::new()
            } else {
                let stem =
                    export::render(&ctrl.project, ctrl.samples(), sample_rate, Some(track), 2.0);
                export::wav_bytes(&stem, sample_rate).unwrap_or_default()
            }
        }
    };
    answer(bytes)
}
