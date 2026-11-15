//! Finished by Freya: a set written through the door, bounced through
//! Freya's engine in more than one genre.

use mjolnir_app::mcp::{self, NoHost};
use mjolnir_app::{freya, App};
use mjolnir_engine::engine::Engine;
use serde_json::{json, Value};

const SR: u32 = 44100;

fn tool(app: &mut App, name: &str, args: Value) -> Value {
    let msg = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let out: Value = serde_json::from_str(&mcp::handle(app, &mut NoHost, &msg.to_string()).unwrap()).unwrap();
    assert_eq!(out["result"]["isError"], false, "{name}: {out}");
    out
}

fn a_set() -> (App, Engine) {
    let (engine, endpoints) = Engine::new(SR as f32);
    let mut app = App::new(endpoints, SR as f32);
    tool(&mut app, "new_set", json!({ "name": "Piece", "bpm": 90, "key": "A", "scale": "minor" }));
    tool(&mut app, "add_track", json!({ "sound": "kit:shadow" }));
    tool(&mut app, "add_track", json!({ "sound": "preset:shadow_sub" }));
    tool(&mut app, "add_track", json!({ "sound": "preset:soft_piano" }));
    tool(&mut app, "drum_groove", json!({ "track": 0, "section": "A", "groove": "boom_bap" }));
    tool(&mut app, "write_loop", json!({ "track": 1, "section": "A", "bars": 1, "notes": [{ "pitch": "A1", "start": 0, "length": 2 }, { "pitch": "E1", "start": 2, "length": 2 }] }));
    tool(&mut app, "write_loop", json!({ "track": 2, "section": "A", "bars": 1, "notes": [{ "pitch": "A3", "start": 0, "length": 2 }, { "pitch": "C4", "start": 0, "length": 2 }, { "pitch": "E4", "start": 2, "length": 2 }] }));
    (app, engine)
}

#[test]
fn the_song_is_finished_in_the_genre_asked_for() {
    let (mut app, _engine) = a_set();
    // nothing arranged yet: it says so
    let err = freya::finish(&app.ctrl.project, app.ctrl.samples(), SR, "lofi").err().unwrap();
    assert!(err.contains("timeline"), "{err}");
    tool(&mut app, "arrange", json!({ "sections": ["A", "A"] }));

    let err = freya::finish(&app.ctrl.project, app.ctrl.samples(), SR, "polka").err().unwrap();
    assert!(err.contains("lofi") && err.contains("metalcore"), "{err}");

    for (genre, target) in [("lofi", -16.0), ("trap", -9.0)] {
        let done = freya::finish(&app.ctrl.project, app.ctrl.samples(), SR, genre).unwrap();
        let r = &done.report;
        assert_eq!(r["genre"], genre);
        assert_eq!(r["target_lufs"], target);
        let lufs = r["lufs"].as_f64().unwrap();
        assert!((lufs - target).abs() < 1.5, "{genre}: {lufs} LUFS");
        assert!(r["peak_db"].as_f64().unwrap() <= -0.9, "{genre}: {r}");
        assert!(done.audio.iter().all(|s| s.is_finite()));
        // about as long as the song: two sections of four bars at 90 bpm, and a tail
        let seconds = done.audio.len() as f64 / 2.0 / SR as f64;
        assert!((21.0..27.0).contains(&seconds), "{seconds}");
        let roles: Vec<&str> = r["tracks"].as_array().unwrap().iter().map(|t| t["role"].as_str().unwrap()).collect();
        assert_eq!(roles, ["drums", "sub", "keys"]);
    }

    // a muted track is not in it, and a name is believed over the sound's kind
    tool(&mut app, "set_track", json!({ "track": 0, "mute": true }));
    tool(&mut app, "set_track", json!({ "track": 2, "name": "lead" }));
    let done = freya::finish(&app.ctrl.project, app.ctrl.samples(), SR, "pop").unwrap();
    let roles: Vec<&str> = done.report["tracks"].as_array().unwrap().iter().map(|t| t["role"].as_str().unwrap()).collect();
    assert_eq!(roles, ["sub", "lead"]);
    assert_eq!(freya::genres().as_array().unwrap().len(), 9);
}
