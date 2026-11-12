//! An agent's session, through the protocol: every message a client would send.

use mjolnir_app::mcp::{self, NoHost};
use mjolnir_app::App;
use mjolnir_engine::engine::Engine;
use serde_json::{json, Value};

struct Client {
    app: App,
    _engine: Engine,
    id: u64,
}

impl Client {
    fn new() -> Client {
        let (engine, endpoints) = Engine::new(48000.0);
        Client { app: App::new(endpoints, 48000.0), _engine: engine, id: 0 }
    }
    fn send(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": self.id, "method": method, "params": params });
        let out = mcp::handle(&mut self.app, &mut NoHost, &msg.to_string()).expect("an answer");
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["id"], self.id);
        v
    }
    /// Call a tool; answer (its text parsed as JSON when it is JSON, is it an error).
    fn tool(&mut self, name: &str, args: Value) -> (Value, bool) {
        let v = self.send("tools/call", json!({ "name": name, "arguments": args }));
        let r = &v["result"];
        let text = r["content"][0]["text"].as_str().unwrap_or_else(|| panic!("{name}: {v}"));
        (serde_json::from_str(text).unwrap_or_else(|_| json!(text)), r["isError"] == true)
    }
    fn ok(&mut self, name: &str, args: Value) -> Value {
        let (v, err) = self.tool(name, args);
        assert!(!err, "{name} failed: {v}");
        v
    }
    fn fails(&mut self, name: &str, args: Value) -> String {
        let (v, err) = self.tool(name, args);
        assert!(err, "{name} should have failed: {v}");
        v.as_str().unwrap_or_default().to_string()
    }
}

#[test]
fn the_handshake_and_the_tools() {
    let mut c = Client::new();
    let init = c.send("initialize", json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }));
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "mjolnir");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    // a revision it does not know: it offers its newest
    let other = c.send("initialize", json!({ "protocolVersion": "1999-01-01" }));
    assert_eq!(other["result"]["protocolVersion"], "2025-06-18");
    // a notification is never answered
    assert!(mcp::handle(&mut c.app, &mut NoHost, r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
    assert_eq!(c.send("ping", json!({}))["result"], json!({}));

    let tools = c.send("tools/list", json!({}));
    let tools = tools["result"]["tools"].as_array().unwrap();
    assert!(tools.len() >= 18);
    for t in tools {
        assert!(t["name"].as_str().is_some_and(|n| !n.is_empty()));
        assert!(t["description"].as_str().is_some_and(|d| d.len() > 20), "{}", t["name"]);
        assert_eq!(t["inputSchema"]["type"], "object");
    }
    // what is not there says so, in the protocol's own terms
    assert_eq!(c.send("tools/call", json!({ "name": "nope", "arguments": {} }))["error"]["code"], -32602);
    assert_eq!(c.send("nope", json!({}))["error"]["code"], -32601);
    let bad = mcp::handle(&mut c.app, &mut NoHost, "{").unwrap();
    assert!(bad.contains("-32700"));
}

#[test]
fn an_agent_writes_a_piece_from_nothing() {
    let mut c = Client::new();
    let sounds = c.ok("list_sounds", json!({}));
    assert!(sounds["drum_kits"].as_array().unwrap().iter().any(|k| k["id"] == "kit:shadow"));
    assert!(sounds["synth"].as_array().unwrap().len() >= 24);
    assert!(sounds["drum_grooves"].as_array().unwrap().iter().any(|g| g["id"] == "boom_bap"));

    let set = c.ok("new_set", json!({ "name": "Night bus", "bpm": 78, "key": "F#", "scale": "dorian" }));
    assert_eq!(set["name"], "Night bus");
    assert_eq!(set["bpm"], 78.0);
    assert_eq!(set["key"], "F# dorian");
    assert_eq!(set["tracks"].as_array().unwrap().len(), 0);

    let drums = c.ok("add_track", json!({ "sound": "kit:shadow" }));
    assert_eq!(drums["track"], 0);
    assert_eq!(drums["kind"], "drums");
    let bass = c.ok("add_track", json!({ "sound": "preset:shadow_sub", "name": "low" }));
    assert_eq!(bass["track"], 1);
    let pad = c.ok("add_track", json!({ "sound": "macro:solina" }));
    assert_eq!(pad["track"], 2);

    // drums by pad name, in beats
    let w = c.ok("write_loop", json!({ "track": 0, "section": "A", "bars": 1, "notes": [
        { "pitch": "kick", "start": 0 }, { "pitch": "snare", "start": 1 }, { "pitch": "kick", "start": 2.5 }, { "pitch": "snare", "start": 3 },
        { "pitch": "hat", "start": 0.5, "velocity": 0.5 }, { "pitch": "hat", "start": 1.5, "velocity": 0.5, "repeat": 2 }, { "pitch": "hat", "start": 3.5, "chance": 50 },
    ] }));
    assert_eq!(w["notes"], 7);
    assert_eq!(w["beats"], 4.0);
    let r = c.ok("read_loop", json!({ "track": 0, "section": "a" }));
    let notes = r["notes"].as_array().unwrap();
    assert_eq!(notes[0]["pitch"], "kick");
    assert!(notes.iter().any(|n| n["pitch"] == "hat" && n["chance"] == 50));
    assert!(notes.iter().any(|n| n["repeat"] == 2));
    assert!(notes.iter().any(|n| n["pitch"] == "kick" && n["start"] == 2.5));

    // a bass line by note name, two bars
    let w = c.ok("write_loop", json!({ "track": 1, "section": "A", "bars": 2, "notes": [
        { "pitch": "F#1", "start": 0, "length": 1.5 }, { "pitch": "A1", "start": 2, "length": 1 }, { "pitch": "E1", "start": 4, "length": 3 }, { "pitch": 42, "start": 7.5, "length": 0.5 },
    ] }));
    assert_eq!(w["bars"], 2);
    let r = c.ok("read_loop", json!({ "track": 1, "section": "A" }));
    assert_eq!(r["notes"][0]["pitch"], "F#1");
    assert_eq!(r["notes"][0]["length"], 1.5);
    assert_eq!(r["notes"][3]["pitch"], "F#2");

    // adding instead of replacing, and notes past the end are told about
    let w = c.ok("write_loop", json!({ "track": 1, "section": "A", "add": true, "notes": [{ "pitch": "C#2", "start": 6 }, { "pitch": "C#2", "start": 9 }] }));
    assert_eq!(w["notes"], 5);
    assert!(w["left_out"].as_str().unwrap().contains("1 note"));

    // a chord on the pad, then a second section that varies the first
    c.ok("write_loop", json!({ "track": 2, "section": "A", "bars": 2, "notes": [
        { "pitch": "F#3", "start": 0, "length": 8 }, { "pitch": "A3", "start": 0, "length": 8 }, { "pitch": "C#4", "start": 0, "length": 8 },
    ] }));
    let copy = c.ok("copy_loop", json!({ "track": 0, "from": "A", "to": "B" }));
    assert_eq!(copy["notes"].as_array().unwrap().len(), 7);
    let g = c.ok("drum_groove", json!({ "track": 0, "section": "B", "groove": "hats_lazy" }));
    assert!(g["notes"].as_array().unwrap().len() > 7);
    c.ok("copy_loop", json!({ "track": 1, "from": "A", "to": "B" }));

    c.ok("set_track", json!({ "track": 2, "knobs": { "filter": 0.6, "reverb": 0.5 }, "pan": -0.2, "name": "strings" }));
    let m = c.ok("set_master", json!({ "tape": 0.4, "warmth": 0.5 }));
    assert_eq!(m["tape"], 0.4);

    let song = c.ok("arrange", json!({ "sections": ["A", "B", { "section": "A", "bars": 8 }], "loop": false }));
    assert_eq!(song["bars"], 16);
    assert_eq!(song["clips"].as_array().unwrap().len(), 3 + 2 + 3);

    let set = c.ok("get_set", json!({}));
    assert_eq!(set["tracks"][2]["name"], "strings");
    assert_eq!(set["tracks"][2]["knobs"]["filter"], 0.6);
    assert_eq!(set["tracks"][2]["pan"], -0.2);
    assert_eq!(set["tracks"][0]["loops"]["B"]["bars"], 1);
    assert_eq!(set["tracks"][1]["sound"], "preset:shadow_sub");
    assert_eq!(set["song"]["bars"], 16);

    // every tool call is one step: back past the timeline, then past the master
    let back = c.ok("undo", json!({}));
    assert_eq!(back["undone"], 1);
    assert_eq!(back["song"]["bars"], 0);
    assert_eq!(back["master"]["tape"], 0.4);
    let back = c.ok("undo", json!({}));
    assert_ne!(back["master"]["tape"], 0.4);
    assert_eq!(back["tracks"][0]["loops"]["A"]["notes"], 7);
    c.ok("play", json!({ "section": "B" }));
    c.ok("stop", json!({}));
}

#[test]
fn an_agent_shapes_sounds() {
    let mut c = Client::new();
    c.ok("new_set", json!({}));
    c.ok("add_track", json!({ "sound": "macro:solina" }));
    c.ok("add_track", json!({ "sound": "preset:soft_piano" }));
    c.ok("add_track", json!({ "sound": "kit:lo_fi" }));

    // the synth: an engine by name, knobs by key
    let s = c.ok("get_sound", json!({ "track": 0 }));
    assert_eq!(s["sound"], "macro:solina");
    let knobs = s["knobs"].as_array().unwrap();
    assert!(knobs.iter().any(|k| k["key"] == "engine" && k["value"] == "Strings"));
    assert!(knobs.iter().all(|k| k["does"].as_str().is_some_and(|d| !d.is_empty())));
    let s = c.ok("shape_sound", json!({ "track": 0, "params": { "engine": "fm", "harmonics": 0.7, "attack": 0.02 }, "name": "glass" }));
    let knobs = s["knobs"].as_array().unwrap();
    assert!(knobs.iter().any(|k| k["key"] == "engine" && k["value"] == "FM"));
    assert!(knobs.iter().any(|k| k["key"] == "harmonics" && (k["value"].as_f64().unwrap() - 0.7).abs() < 1e-6));
    assert!(s["sound"].as_str().unwrap().contains("glass"));
    assert!(c.fails("shape_sound", json!({ "track": 0, "params": { "wobble": 1 } })).contains("harmonics"));
    assert!(c.fails("shape_sound", json!({ "track": 0, "params": { "engine": "theremin" } })).contains("Pluck"));

    // a recorded sound opens into a designed synth
    let s = c.ok("get_sound", json!({ "track": 1 }));
    assert!(s["knobs"].is_null());
    let s = c.ok("shape_sound", json!({ "track": 1 }));
    assert!(s["sound"].as_str().unwrap().starts_with("designed synth"));
    let key = s["knobs"][0]["key"].as_str().unwrap().to_string();
    let max = s["knobs"][0]["max"].as_f64().unwrap();
    let s = c.ok("shape_sound", json!({ "track": 1, "params": { key.clone(): max } }));
    assert_eq!(s["knobs"][0]["value"].as_f64().unwrap(), max);

    // a kit opens pad by pad
    let s = c.ok("get_sound", json!({ "track": 2 }));
    let first = s["pads"][0]["name"].as_str().unwrap().to_string();
    assert!(c.fails("shape_sound", json!({ "track": 0, "pad": first.clone(), "params": {} })).contains("drum kit"));
    let s = c.ok("shape_sound", json!({ "track": 2 }));
    assert!(s["sound"].as_str().unwrap().starts_with("designed kit"));
    assert!(c.fails("shape_sound", json!({ "track": 2, "params": { "tone_level": 1 } })).contains("pad"));
    let s = c.ok("shape_sound", json!({ "track": 2, "pad": first, "params": { "tone_start": 180, "tone_end": 42 } }));
    assert_eq!(s["pads"][0]["knobs"]["tone_end"].as_f64().unwrap(), 42.0);
    // and it still plays by pad name
    let w = c.ok("write_loop", json!({ "track": 2, "section": "C", "notes": [{ "pitch": s["pads"][0]["name"], "start": 0 }] }));
    assert_eq!(w["notes"], 1);
}

#[test]
fn mistakes_are_explained() {
    let mut c = Client::new();
    c.ok("new_set", json!({}));
    assert!(c.fails("write_loop", json!({ "track": 0, "section": "A", "notes": [] })).contains("no track 0"));
    assert!(c.fails("add_track", json!({ "sound": "piano" })).contains("list_sounds"));
    assert!(c.fails("add_track", json!({ "sound": "kit:nope" })).contains("list_sounds"));
    c.ok("add_track", json!({ "sound": "kit:shadow" }));
    c.ok("add_track", json!({ "sound": "preset:harp" }));
    assert!(c.fails("write_loop", json!({ "track": 0, "section": "Z", "notes": [] })).contains("A to H"));
    assert!(c.fails("write_loop", json!({ "track": 0, "section": "A", "bars": 3, "notes": [] })).contains("1, 2, 4 or 8"));
    assert!(c.fails("write_loop", json!({ "track": 0, "section": "A", "notes": [{ "pitch": "cowbell", "start": 0 }] })).contains("kick"));
    assert!(c.fails("write_loop", json!({ "track": 1, "section": "A", "notes": [{ "pitch": "H2", "start": 0 }] })).contains("F#3"));
    assert!(c.fails("set_track", json!({ "track": 1, "sound": "kit:shadow" })).contains("drum kit"));
    assert!(c.fails("set_track", json!({ "track": 0, "knobs": { "cutoff": 0.5 } })).contains("filter"));
    assert!(c.fails("drum_groove", json!({ "track": 1, "section": "A", "groove": "boom_bap" })).contains("drum"));
    assert!(c.fails("drum_groove", json!({ "track": 0, "section": "A", "groove": "waltz" })).contains("boom_bap"));
    assert!(c.fails("arrange", json!({ "sections": ["A"] })).contains("no loops"));
    assert!(c.fails("play", json!({ "song": true })).contains("empty"));
    assert!(c.fails("set_song", json!({ "scale": "sad" })).contains("minor"));
    assert!(c.fails("copy_loop", json!({ "track": 0, "from": "A", "to": "B" })).contains("no loop"));
}
