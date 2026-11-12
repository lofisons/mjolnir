//! Mjolnir for an agent: the Model Context Protocol over the same door every
//! host uses. An agent sees a small set of musical tools (write a loop, shape
//! a sound, arrange a song) and each one is carried out as the actions a
//! person's hands would send, so undo, saving and the screen behave the same.
//!
//! This module only speaks JSON-RPC: a message in, a message out. A host
//! carries the messages (a pipe, a local HTTP port) and may add tools of its
//! own for what only it can do (files, rendering audio).

use serde_json::{json, Map, Value};

use crate::{Action, App};

/// The protocol revisions this server can speak, newest first.
const PROTOCOLS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
const PPQ: f64 = 96.0;
const KNOBS: [&str; 7] = ["filter", "resonance", "drive", "saturation", "reverb", "delay", "volume"];
const KNOB_IDS: [&str; 7] = ["cutoff", "resonance", "drive", "saturation", "reverb", "delay", "volume"];
const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// What only a host can do. `tools` are listed after Mjolnir's own; `call`
/// answers `None` for a name that is not the host's.
pub trait Host {
    fn tools(&self) -> Vec<Value> {
        Vec::new()
    }
    fn call(&mut self, _app: &mut App, _name: &str, _args: &Value) -> Option<Result<Value, String>> {
        None
    }
}

/// A host with nothing of its own to add.
pub struct NoHost;
impl Host for NoHost {}

/// Answer one JSON-RPC message (or a batch). `None` when there is nothing to
/// say back: a notification.
pub fn handle(app: &mut App, host: &mut dyn Host, message: &str) -> Option<String> {
    let parsed: Value = match serde_json::from_str(message) {
        Ok(v) => v,
        Err(e) => return Some(error(Value::Null, -32700, &format!("not JSON: {e}")).to_string()),
    };
    if let Value::Array(batch) = parsed {
        let out: Vec<Value> = batch.iter().filter_map(|m| one(app, host, m)).collect();
        return if out.is_empty() { None } else { Some(Value::Array(out).to_string()) };
    }
    one(app, host, &parsed).map(|v| v.to_string())
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn one(app: &mut App, host: &mut dyn Host, m: &Value) -> Option<Value> {
    let id = m.get("id").cloned();
    let method = m.get("method").and_then(Value::as_str).unwrap_or("");
    let params = m.get("params").cloned().unwrap_or(Value::Null);
    // a message with no id is a notification: never answered
    let id = id?;
    let result = match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
            let version = PROTOCOLS.iter().find(|v| **v == asked).unwrap_or(&PROTOCOLS[0]);
            Ok(json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "mjolnir", "title": "Mjolnir", "version": env!("CARGO_PKG_VERSION") },
                "instructions": INSTRUCTIONS,
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => {
            let mut all = tools();
            all.extend(host.tools());
            Ok(json!({ "tools": all }))
        }
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            let known = tools().iter().chain(host.tools().iter()).any(|t| t["name"] == name);
            if !known {
                Err((-32602, format!("no such tool: {name}")))
            } else {
                let out = match host.call(app, name, &args) {
                    Some(r) => r,
                    // undo walks the steps itself and looking changes nothing; everything else is one step, whatever it took
                    None if matches!(name, "undo" | "get_set" | "list_sounds" | "read_loop" | "get_sound" | "play" | "stop") => call(app, name, &args),
                    None => app.as_one_step(|app| call(app, name, &args)),
                };
                // a tool that fails is still an answer: the agent reads why and tries again
                Ok(match out {
                    Ok(v) => json!({ "content": [{ "type": "text", "text": text(&v) }], "isError": false }),
                    Err(e) => json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
                })
            }
        }
        _ => Err((-32601, format!("no such method: {method}"))),
    };
    Some(match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err((code, message)) => error(id, code, &message),
    })
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    }
}

const INSTRUCTIONS: &str = "Mjolnir is an instrument for sketching beats. A set has tracks (a drum kit or a melodic sound each) and up to eight sections, A to H. A loop is one track's notes in one section, 1, 2, 4 or 8 bars long. Sections played in a row make the song. \
Start with get_set to see what is there and list_sounds to see what can be played. Write music with write_loop: time is in beats (quarter notes) from the start of the loop, pitches are note names like \"A2\" or MIDI numbers, and on a drum track a pitch can be a pad's name like \"kick\". \
Shape a sound with get_sound and shape_sound. Then arrange sections into a song. Tracks are numbered from 0. Everything you do can be undone with undo.";

// ---------------------------------------------------------------- the tools

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": { "type": "object", "properties": properties, "required": required, "additionalProperties": false },
    })
}

fn tools() -> Vec<Value> {
    let track = json!({ "type": "integer", "minimum": 0, "description": "The track's number, from 0, as get_set lists it." });
    let section = json!({ "type": "string", "description": "A section: a letter A to H (or its number from 0)." });
    vec![
        tool("get_set", "The whole set at a glance: name, tempo, key, every track with its sound, knobs and which sections have a loop, the song's timeline and the master. Read this first, and again whenever you are unsure what is there.", json!({}), &[]),
        tool("list_sounds", "Every sound a track can play (drum kits, recorded instruments, airs, leads and the synth's own sounds), the ready-made drum grooves, and the scales. Sound ids look like \"kit:shadow\" or \"macro:solina\".", json!({
            "kind": { "type": "string", "enum": ["all", "drums", "melodic"], "description": "Only drum kits, only melodic sounds, or everything (default)." }
        }), &[]),
        tool("new_set", "Start over with an empty set: no tracks, eight empty sections. Unsaved work in the current set is lost.", json!({
            "name": { "type": "string" },
            "bpm": { "type": "number", "minimum": 20, "maximum": 300 },
            "key": { "type": "string", "description": "The root note, like \"A\" or \"F#\"." },
            "scale": { "type": "string", "description": "A scale id from list_sounds, like \"minor\"." }
        }), &[]),
        tool("set_song", "Change what belongs to the whole set: its name, tempo, swing or key. Only what you give changes.", json!({
            "name": { "type": "string" },
            "bpm": { "type": "number", "minimum": 20, "maximum": 300 },
            "swing": { "type": "number", "minimum": 0, "maximum": 1, "description": "0 straight; around 0.1 to 0.2 for a lo-fi lean." },
            "key": { "type": "string", "description": "The root note, like \"A\" or \"F#\"." },
            "scale": { "type": "string", "description": "A scale id from list_sounds." }
        }), &[]),
        tool("add_track", "Add a track playing a sound. A kit makes a drum track, anything else a melodic one. Answers the new track's number.", json!({
            "sound": { "type": "string", "description": "A sound id from list_sounds." },
            "name": { "type": "string", "description": "A name for the track; the sound's own name if left out." }
        }), &["sound"]),
        tool("remove_track", "Remove a track and its loops. Tracks after it move down one number.", json!({ "track": track }), &["track"]),
        tool("set_track", "Change a track: its sound, name, knobs, pan, mute or solo. Only what you give changes. Changing the sound keeps the notes; a drum track takes kits, a melodic one anything else.", json!({
            "track": track,
            "sound": { "type": "string", "description": "A sound id from list_sounds." },
            "name": { "type": "string" },
            "knobs": { "type": "object", "description": "Any of filter, resonance, drive, saturation, reverb, delay, volume, each 0 to 1. filter 1 is fully open.", "additionalProperties": { "type": "number", "minimum": 0, "maximum": 1 } },
            "pan": { "type": "number", "minimum": -1, "maximum": 1 },
            "mute": { "type": "boolean" },
            "solo": { "type": "boolean" }
        }), &["track"]),
        tool("write_loop", "Write a loop: the notes one track plays in one section. By default it replaces what was there. Time is in beats (quarter notes) from the start of the loop, so in 4/4 a bar is 4 beats and a sixteenth is 0.25.", json!({
            "track": track,
            "section": section,
            "bars": { "type": "integer", "enum": [1, 2, 4, 8], "description": "The loop's length. Default: what it already is, or 1." },
            "notes": { "type": "array", "description": "The notes.", "items": { "type": "object", "properties": {
                "pitch": { "description": "A note name (\"A2\", \"F#3\", \"Bb4\"; C4 is middle C, MIDI 60), a MIDI number, or on a drum track a pad's name (\"kick\", \"snare\", \"hat\": see get_sound).", "type": ["string", "integer"] },
                "start": { "type": "number", "minimum": 0, "description": "Where it starts, in beats from the start of the loop." },
                "length": { "type": "number", "exclusiveMinimum": 0, "description": "How long it lasts, in beats. Default 1 (a drum hit: 0.25)." },
                "velocity": { "type": "number", "minimum": 0, "maximum": 1, "description": "How hard, default 0.8." },
                "chance": { "type": "integer", "minimum": 1, "maximum": 100, "description": "Percent of passes it sounds on, default 100." },
                "repeat": { "type": "integer", "minimum": 1, "maximum": 4, "description": "Strikes inside its first sixteenth (a roll), default 1." }
            }, "required": ["pitch", "start"], "additionalProperties": false } },
            "add": { "type": "boolean", "description": "true adds the notes to what is there instead of replacing it." }
        }), &["track", "section", "notes"]),
        tool("read_loop", "The notes of one loop, in the terms write_loop takes.", json!({ "track": track, "section": section }), &["track", "section"]),
        tool("clear_loop", "Empty one loop: the track plays nothing in that section until it is written again.", json!({ "track": track, "section": section }), &["track", "section"]),
        tool("copy_loop", "Copy a track's loop from one section to another, to vary it there.", json!({ "track": track, "from": section, "to": section }), &["track", "from", "to"]),
        tool("drum_groove", "Load a ready-made drum pattern into a drum track's loop. A whole beat replaces the loop; a hat line is laid over what is there.", json!({
            "track": track,
            "section": section,
            "groove": { "type": "string", "description": "A groove id from list_sounds, like \"boom_bap\" or \"hats_lazy\"." }
        }), &["track", "section", "groove"]),
        tool("get_sound", "A track's sound in detail: what kind it is, every knob that shapes it with its current value, range and what it does, and on a drum track the pads by name.", json!({ "track": track }), &["track"]),
        tool("shape_sound", "Shape a track's sound from the inside. A synth sound (macro:…) has an engine and ten knobs. Any other sound is first turned into an editable copy of itself (a designed synth, or a kit whose every pad can be shaped): call get_sound afterwards to see its knobs. Only what you give changes.", json!({
            "track": track,
            "params": { "type": "object", "description": "Knob keys and values, as get_sound lists them. The synth's `engine` may be a name (\"FM\", \"Pluck\").", "additionalProperties": { "type": ["number", "string"] } },
            "pad": { "description": "On a drum track: the pad to shape, by name or number.", "type": ["string", "integer"] },
            "name": { "type": "string", "description": "A new name for the sound (or the pad)." }
        }), &["track"]),
        tool("set_master", "The whole mix: its volume, the lo-fi character (tape wobble, vinyl crackle, warmth), the size of the room every reverb send goes to, and how much the kick pushes everything else down (pump). Each 0 to 1; only what you give changes.", json!({
            "volume": { "type": "number", "minimum": 0, "maximum": 1 },
            "tape": { "type": "number", "minimum": 0, "maximum": 1 },
            "vinyl": { "type": "number", "minimum": 0, "maximum": 1 },
            "warmth": { "type": "number", "minimum": 0, "maximum": 1 },
            "space": { "type": "number", "minimum": 0, "maximum": 1 },
            "pump": { "type": "number", "minimum": 0, "maximum": 1 }
        }), &[]),
        tool("arrange", "Lay the song out: the sections to play, in order. It replaces the timeline. Each section plays for its longest loop rounded up to 4 bars, or for `bars` if given.", json!({
            "sections": { "type": "array", "description": "Section letters in order, like [\"A\",\"A\",\"B\",\"C\",\"B\"], or objects to set a length.", "items": { "anyOf": [
                { "type": "string" },
                { "type": "object", "properties": { "section": { "type": "string" }, "bars": { "type": "integer", "minimum": 1, "maximum": 64 } }, "required": ["section"], "additionalProperties": false }
            ] } },
            "loop": { "type": "boolean", "description": "Whether the song starts again when it ends." }
        }), &["sections"]),
        tool("play", "Start playing, so the person at the instrument hears it: one section's loops, or the song. With no instrument listening (a set worked on as a file) nothing sounds: render it instead.", json!({
            "section": { "type": "string", "description": "A section letter: play that row of loops." },
            "song": { "type": "boolean", "description": "true plays the timeline from its start." }
        }), &[]),
        tool("stop", "Stop playing: the transport stops and every loop lets go.", json!({}), &[]),
        tool("undo", "Take back the last change, or several. Each tool call that changed something is one step, the same steps the person at the instrument undoes.", json!({ "steps": { "type": "integer", "minimum": 1, "maximum": 50 } }), &[]),
    ]
}

// ------------------------------------------------------------ carrying out

fn act(app: &mut App, action: Value) -> Result<Value, String> {
    let action: Action = serde_json::from_value(action).map_err(|e| format!("internal: {e}"))?;
    app.handle(action)
}

fn project(app: &mut App) -> Result<Value, String> {
    act(app, json!({ "type": "get_project" }))
}

fn catalog(app: &mut App) -> Result<Value, String> {
    Ok(act(app, json!({ "type": "info" }))?["catalog"].clone())
}

fn arg_track(app: &mut App, args: &Value) -> Result<usize, String> {
    let n = args.get("track").and_then(Value::as_u64).ok_or("give the track's number (from 0)")? as usize;
    let count = project(app)?["tracks"].as_array().map_or(0, Vec::len);
    if n >= count {
        return Err(format!("there is no track {n}: the set has {count} (numbered from 0). get_set lists them."));
    }
    Ok(n)
}

fn section_index(v: Option<&Value>, what: &str) -> Result<usize, String> {
    let bad = || format!("{what} must be a letter A to H");
    match v {
        Some(Value::Number(n)) => n.as_u64().filter(|n| *n < 8).map(|n| n as usize).ok_or_else(bad),
        Some(Value::String(s)) => {
            let s = s.trim();
            if s.len() == 1 {
                let c = s.as_bytes()[0].to_ascii_uppercase();
                if (b'A'..=b'H').contains(&c) {
                    return Ok((c - b'A') as usize);
                }
            }
            s.parse::<usize>().ok().filter(|n| *n < 8).ok_or_else(bad)
        }
        _ => Err(bad()),
    }
}

fn section_name(i: usize) -> String {
    ((b'A' + i as u8) as char).to_string()
}

fn pitch_name(p: u64) -> String {
    format!("{}{}", NOTE_NAMES[(p % 12) as usize], p as i64 / 12 - 1)
}

/// "A2", "f#3", "Bb4" → a MIDI note; C4 is 60.
fn parse_pitch(s: &str) -> Option<u8> {
    let s = s.trim();
    let mut chars = s.chars();
    let letter = chars.next()?.to_ascii_uppercase();
    let base: i32 = match letter {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let rest: String = chars.collect();
    let (accidental, octave) = match rest.chars().next() {
        Some('#') | Some('♯') => (1, &rest[rest.chars().next()?.len_utf8()..]),
        Some('b') | Some('♭') => (-1, &rest[rest.chars().next()?.len_utf8()..]),
        _ => (0, rest.as_str()),
    };
    let octave: i32 = octave.parse().ok()?;
    let n = (octave + 1) * 12 + base + accidental;
    (0..=127).contains(&n).then_some(n as u8)
}

/// A root note without an octave: "A", "F#", "Bb" → 0..11.
fn parse_root(s: &str) -> Option<u8> {
    parse_pitch(&format!("{}4", s.trim())).map(|p| p % 12)
}

fn sound_id(instrument: &Value, cat: &Value) -> String {
    let kind = instrument["type"].as_str().unwrap_or("");
    match kind {
        "drums" => format!("kit:{}", instrument["kit"].as_str().unwrap_or("?")),
        "synth" => format!("preset:{}", instrument["preset"].as_str().unwrap_or("?")),
        "granular" => format!("texture:{}", instrument["texture"].as_str().unwrap_or("?")),
        "macro" => {
            // one of the ready sounds, until it is shaped
            let found = cat["macro_sounds"].as_array().and_then(|all| all.iter().find(|m| m["patch"] == instrument["patch"]));
            match found {
                Some(m) => format!("macro:{}", m["id"].as_str().unwrap_or("?")),
                None => format!("shaped synth \"{}\"", instrument["name"].as_str().unwrap_or("")),
            }
        }
        "custom_synth" => format!("designed synth \"{}\"", instrument["name"].as_str().unwrap_or("")),
        "custom_kit" => format!("designed kit \"{}\"", instrument["name"].as_str().unwrap_or("")),
        other => other.replace('_', " "),
    }
}

/// A sound id → the instrument to swap in, and whether it is a drum kit.
fn sound_spec(id: &str, cat: &Value) -> Result<(Value, bool, String), String> {
    let (family, key) = id.split_once(':').ok_or_else(|| format!("\"{id}\" is not a sound id: they look like \"kit:shadow\". list_sounds has them all."))?;
    let find = |list: &str| cat[list].as_array().and_then(|all| all.iter().find(|s| s["id"] == key)).cloned();
    let missing = || format!("there is no sound \"{id}\". list_sounds has them all.");
    match family {
        "kit" => {
            let s = find("kits").ok_or_else(missing)?;
            Ok((json!({ "type": "drums", "kit": key }), true, s["label"].as_str().unwrap_or(key).to_string()))
        }
        "preset" => {
            let s = find("presets").ok_or_else(missing)?;
            Ok((json!({ "type": "synth", "preset": key }), false, s["label"].as_str().unwrap_or(key).to_string()))
        }
        "texture" => {
            let s = find("textures").ok_or_else(missing)?;
            Ok((json!({ "type": "granular", "texture": key, "sample": null }), false, s["label"].as_str().unwrap_or(key).to_string()))
        }
        "macro" => {
            let s = find("macro_sounds").ok_or_else(missing)?;
            let label = s["label"].as_str().unwrap_or(key).to_string();
            Ok((json!({ "type": "macro", "name": label, "patch": s["patch"] }), false, label))
        }
        _ => Err(missing()),
    }
}

fn round(v: f64) -> Value {
    json!((v * 1000.0).round() / 1000.0)
}

fn knobs_of(track: &Value) -> Value {
    let mut out = Map::new();
    for (i, name) in KNOBS.iter().enumerate() {
        out.insert((*name).into(), round(track["macros"][i].as_f64().unwrap_or(0.0)));
    }
    Value::Object(out)
}

fn overview(app: &mut App) -> Result<Value, String> {
    let p = project(app)?;
    let cat = catalog(app)?;
    let ids: Vec<u64> = p["tracks"].as_array().map(|t| t.iter().map(|t| t["id"].as_u64().unwrap_or(0)).collect()).unwrap_or_default();
    let tracks: Vec<Value> = p["tracks"]
        .as_array()
        .map(|all| {
            all.iter()
                .enumerate()
                .map(|(i, t)| {
                    let mut loops = Map::new();
                    for (s, slot) in t["slots"].as_array().into_iter().flatten().enumerate().take(8) {
                        if let Some(notes) = slot["notes"].as_array() {
                            loops.insert(section_name(s), json!({ "bars": slot["bars"], "notes": notes.len() }));
                        }
                    }
                    let mut o = json!({
                        "track": i,
                        "name": t["name"],
                        "kind": t["kind"],
                        "sound": sound_id(&t["instrument"], &cat),
                        "knobs": knobs_of(t),
                        "loops": loops,
                    });
                    if t["mute"] == true {
                        o["mute"] = json!(true);
                    }
                    if t["solo"] == true {
                        o["solo"] = json!(true);
                    }
                    if t["pan"].as_f64().unwrap_or(0.0) != 0.0 {
                        o["pan"] = round(t["pan"].as_f64().unwrap_or(0.0));
                    }
                    o
                })
                .collect()
        })
        .unwrap_or_default();
    let clips: Vec<Value> = p["song"]["clips"]
        .as_array()
        .map(|all| {
            all.iter()
                .map(|c| {
                    let track = ids.iter().position(|id| Some(*id) == c["track"].as_u64());
                    json!({ "track": track, "section": section_name(c["slot"].as_u64().unwrap_or(0) as usize), "start_bar": c["start"], "bars": c["bars"] })
                })
                .collect()
        })
        .unwrap_or_default();
    let song_bars = p["song"]["clips"].as_array().map(|all| all.iter().map(|c| c["start"].as_u64().unwrap_or(0) + c["bars"].as_u64().unwrap_or(0)).max().unwrap_or(0)).unwrap_or(0);
    let master: Map<String, Value> = p["master"].as_object().map(|m| m.iter().map(|(k, v)| (k.clone(), round(v.as_f64().unwrap_or(0.0)))).collect()).unwrap_or_default();
    Ok(json!({
        "name": p["name"],
        "bpm": p["bpm"],
        "swing": round(p["swing"].as_f64().unwrap_or(0.0)),
        "key": format!("{} {}", NOTE_NAMES[(p["scale"]["root"].as_u64().unwrap_or(0) % 12) as usize], p["scale"]["kind"].as_str().unwrap_or("")),
        "meter": format!("{}/{}", p["meter"]["beats"], p["meter"]["unit"]),
        "tracks": tracks,
        "song": { "bars": song_bars, "loops_back": p["song"]["looping"], "clips": clips },
        "master": master,
    }))
}

/// The pads of a drum track: (name, pitch).
fn pads(app: &mut App, track: usize) -> Result<Vec<(String, u8)>, String> {
    let v = act(app, json!({ "type": "pads", "track": track }))?;
    Ok(v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| Some((p["label"].as_str()?.to_string(), p["pitches"][0].as_u64()? as u8)))
        .collect())
}

fn is_drums(app: &mut App, track: usize) -> Result<bool, String> {
    Ok(project(app)?["tracks"][track]["kind"] == "drums")
}

fn resolve_pitch(v: Option<&Value>, drum_pads: Option<&[(String, u8)]>) -> Result<u8, String> {
    match v {
        Some(Value::Number(n)) => n.as_u64().filter(|n| *n <= 127).map(|n| n as u8).ok_or_else(|| "a MIDI pitch is 0 to 127".to_string()),
        Some(Value::String(s)) => {
            if let Some(pads) = drum_pads {
                let want = s.trim().to_lowercase();
                if let Some((_, p)) = pads.iter().find(|(name, _)| name.to_lowercase() == want) {
                    return Ok(*p);
                }
                if let Some(p) = parse_pitch(s) {
                    return Ok(p);
                }
                let names: Vec<String> = pads.iter().map(|(n, _)| n.to_lowercase()).collect();
                return Err(format!("this kit has no pad \"{s}\". Its pads: {}.", names.join(", ")));
            }
            parse_pitch(s).ok_or_else(|| format!("\"{s}\" is not a note: write it like \"A2\", \"F#3\" or \"Bb4\", or as a MIDI number"))
        }
        _ => Err("every note needs a pitch".into()),
    }
}

fn beats_per_bar(p: &Value) -> f64 {
    let beats = p["meter"]["beats"].as_f64().unwrap_or(4.0);
    let unit = p["meter"]["unit"].as_f64().unwrap_or(4.0);
    beats * 4.0 / unit
}

fn loop_notes(app: &mut App, track: usize, slot: usize) -> Result<Value, String> {
    let p = project(app)?;
    let drums = p["tracks"][track]["kind"] == "drums";
    let names = if drums { pads(app, track)? } else { Vec::new() };
    let slot_v = &p["tracks"][track]["slots"][slot];
    let Some(notes) = slot_v["notes"].as_array() else {
        return Ok(json!({ "track": track, "section": section_name(slot), "empty": true }));
    };
    let out: Vec<Value> = notes
        .iter()
        .map(|n| {
            let pitch = n["pitch"].as_u64().unwrap_or(0);
            let name = names.iter().find(|(_, p)| *p as u64 == pitch).map(|(n, _)| n.to_lowercase()).unwrap_or_else(|| pitch_name(pitch));
            let mut o = json!({
                "pitch": name,
                "start": round(n["start"].as_f64().unwrap_or(0.0) / PPQ),
                "length": round(n["length"].as_f64().unwrap_or(0.0) / PPQ),
                "velocity": round(n["velocity"].as_f64().unwrap_or(0.0)),
            });
            if let Some(c) = n["chance"].as_u64().filter(|c| *c < 100) {
                o["chance"] = json!(c);
            }
            if let Some(r) = n["repeat"].as_u64().filter(|r| *r > 1) {
                o["repeat"] = json!(r);
            }
            o
        })
        .collect();
    let bars = slot_v["bars"].as_u64().unwrap_or(1);
    Ok(json!({ "track": track, "section": section_name(slot), "bars": bars, "beats": bars as f64 * beats_per_bar(&p), "notes": out }))
}

fn call(app: &mut App, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "get_set" => overview(app),

        "list_sounds" => {
            let cat = catalog(app)?;
            let want = args["kind"].as_str().unwrap_or("all");
            let list = |key: &str, family: &str| -> Vec<Value> {
                cat[key]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|s| json!({ "id": format!("{family}:{}", s["id"].as_str().unwrap_or("")), "name": s["label"], "shelf": s["category"] }))
                    .collect()
            };
            let mut out = Map::new();
            if want != "melodic" {
                out.insert("drum_kits".into(), json!(list("kits", "kit")));
                out.insert("drum_grooves".into(), cat["grooves"].as_array().map(|g| g.iter().map(|g| json!({ "id": g["id"], "name": g["label"], "is": if g["kind"] == "hats" { "a hat line, laid over the loop" } else { "a whole beat, replaces the loop" } })).collect::<Vec<_>>()).into());
            }
            if want != "drums" {
                let mut melodic = list("presets", "preset");
                melodic.extend(list("textures", "texture"));
                out.insert("recorded".into(), json!(melodic));
                out.insert("synth".into(), json!(list("macro_sounds", "macro")));
                out.insert("synth_engines".into(), cat["macro_params"][0]["choices"].clone());
            }
            if want == "all" {
                out.insert("scales".into(), cat["scales"].as_array().map(|s| s.iter().map(|s| s["id"].clone()).collect::<Vec<_>>()).into());
            }
            Ok(Value::Object(out))
        }

        "new_set" => {
            act(app, json!({ "type": "new_project" }))?;
            // the blank set comes with a starting band: an agent starts from silence
            let count = project(app)?["tracks"].as_array().map_or(0, Vec::len);
            for _ in 0..count {
                act(app, json!({ "type": "remove_track", "track": 0 }))?;
            }
            song_settings(app, args)?;
            overview(app)
        }

        "set_song" => {
            song_settings(app, args)?;
            overview(app)
        }

        "add_track" => {
            let cat = catalog(app)?;
            let id = args["sound"].as_str().ok_or("give a sound id, like \"kit:shadow\"")?;
            let (spec, drums, label) = sound_spec(id, &cat)?;
            act(app, json!({ "type": "add_track", "kind": if drums { "drums" } else { "melodic" } }))?;
            let track = project(app)?["tracks"].as_array().map_or(0, Vec::len) - 1;
            act(app, json!({ "type": "swap", "track": track, "instrument": spec }))?;
            let name = args["name"].as_str().map(str::to_string).unwrap_or(label);
            act(app, json!({ "type": "rename_track", "track": track, "name": name }))?;
            Ok(json!({ "track": track, "name": name, "kind": if drums { "drums" } else { "melodic" }, "sound": id }))
        }

        "remove_track" => {
            let track = arg_track(app, args)?;
            act(app, json!({ "type": "remove_track", "track": track }))?;
            overview(app)
        }

        "set_track" => {
            let track = arg_track(app, args)?;
            if let Some(id) = args["sound"].as_str() {
                let cat = catalog(app)?;
                let (spec, drums, _) = sound_spec(id, &cat)?;
                if drums != is_drums(app, track)? {
                    return Err(if drums {
                        format!("track {track} is melodic and \"{id}\" is a drum kit: add a new track for it")
                    } else {
                        format!("track {track} is a drum track and \"{id}\" is not a kit: add a new track for it")
                    });
                }
                act(app, json!({ "type": "swap", "track": track, "instrument": spec }))?;
            }
            if let Some(name) = args["name"].as_str() {
                act(app, json!({ "type": "rename_track", "track": track, "name": name }))?;
            }
            if let Some(knobs) = args["knobs"].as_object() {
                for (k, v) in knobs {
                    let i = KNOBS.iter().position(|n| n == k).ok_or_else(|| format!("there is no knob \"{k}\": the knobs are {}", KNOBS.join(", ")))?;
                    let v = v.as_f64().ok_or_else(|| format!("{k} takes a number from 0 to 1"))?;
                    act(app, json!({ "type": "set_macro", "track": track, "param": KNOB_IDS[i], "value": v.clamp(0.0, 1.0) }))?;
                }
            }
            if let Some(pan) = args["pan"].as_f64() {
                act(app, json!({ "type": "set_pan", "track": track, "pan": pan.clamp(-1.0, 1.0) }))?;
            }
            if let Some(mute) = args["mute"].as_bool() {
                act(app, json!({ "type": "set_mute", "track": track, "mute": mute }))?;
            }
            if let Some(solo) = args["solo"].as_bool() {
                act(app, json!({ "type": "set_solo", "track": track, "solo": solo }))?;
            }
            let p = project(app)?;
            let cat = catalog(app)?;
            let t = &p["tracks"][track];
            Ok(json!({ "track": track, "name": t["name"], "sound": sound_id(&t["instrument"], &cat), "knobs": knobs_of(t), "pan": t["pan"], "mute": t["mute"], "solo": t["solo"] }))
        }

        "write_loop" => {
            let track = arg_track(app, args)?;
            let slot = section_index(args.get("section"), "section")?;
            let p = project(app)?;
            let per_bar = beats_per_bar(&p);
            let drums = p["tracks"][track]["kind"] == "drums";
            let names = if drums { Some(pads(app, track)?) } else { None };
            let existing = &p["tracks"][track]["slots"][slot];
            let had = existing["notes"].is_array();
            let bars = match args["bars"].as_u64() {
                Some(b) if [1, 2, 4, 8].contains(&b) => b,
                Some(b) => return Err(format!("a loop is 1, 2, 4 or 8 bars long, not {b}")),
                None => existing["bars"].as_u64().unwrap_or(1),
            };
            let beats = bars as f64 * per_bar;
            let list = args["notes"].as_array().ok_or("notes must be a list")?;
            let mut notes: Vec<Value> = if args["add"] == true && had { existing["notes"].as_array().cloned().unwrap_or_default() } else { Vec::new() };
            let mut outside = 0;
            for (i, n) in list.iter().enumerate() {
                let at = |e: String| format!("note {}: {e}", i + 1);
                let pitch = resolve_pitch(n.get("pitch"), names.as_deref()).map_err(at)?;
                let start = n["start"].as_f64().ok_or_else(|| at("needs a start, in beats".into()))?;
                if start < 0.0 || start >= beats {
                    outside += 1;
                    continue;
                }
                let length = n["length"].as_f64().unwrap_or(if drums { 0.25 } else { 1.0 }).max(1.0 / PPQ).min(beats - start);
                let mut note = json!({
                    "start": (start * PPQ).round() as u64,
                    "length": ((length * PPQ).round() as u64).max(1),
                    "pitch": pitch,
                    "velocity": n["velocity"].as_f64().unwrap_or(0.8).clamp(0.05, 1.0),
                });
                if let Some(c) = n["chance"].as_u64() {
                    note["chance"] = json!(c.clamp(1, 100));
                }
                if let Some(r) = n["repeat"].as_u64() {
                    note["repeat"] = json!(r.clamp(1, 4));
                }
                notes.push(note);
            }
            if !had {
                act(app, json!({ "type": "create_pattern", "track": track, "slot": slot, "bars": bars }))?;
            } else if existing["bars"].as_u64() != Some(bars) {
                act(app, json!({ "type": "set_bars", "track": track, "slot": slot, "bars": bars }))?;
            }
            act(app, json!({ "type": "set_notes", "track": track, "slot": slot, "notes": notes }))?;
            let after = project(app)?;
            let written = after["tracks"][track]["slots"][slot]["notes"].as_array().map_or(0, Vec::len);
            let mut out = json!({ "track": track, "section": section_name(slot), "bars": bars, "beats": beats, "notes": written });
            if outside > 0 {
                out["left_out"] = json!(format!("{outside} note(s) started outside the loop's {beats} beats and were left out: make the loop longer with `bars`, or move them"));
            }
            Ok(out)
        }

        "read_loop" => {
            let track = arg_track(app, args)?;
            let slot = section_index(args.get("section"), "section")?;
            loop_notes(app, track, slot)
        }

        "clear_loop" => {
            let track = arg_track(app, args)?;
            let slot = section_index(args.get("section"), "section")?;
            if project(app)?["tracks"][track]["slots"][slot]["notes"].is_array() {
                act(app, json!({ "type": "delete_pattern", "track": track, "slot": slot }))?;
            }
            Ok(json!({ "track": track, "section": section_name(slot), "empty": true }))
        }

        "copy_loop" => {
            let track = arg_track(app, args)?;
            let from = section_index(args.get("from"), "from")?;
            let to = section_index(args.get("to"), "to")?;
            if !project(app)?["tracks"][track]["slots"][from]["notes"].is_array() {
                return Err(format!("track {track} has no loop in section {}", section_name(from)));
            }
            act(app, json!({ "type": "duplicate_pattern", "track": track, "from": from, "to": to }))?;
            loop_notes(app, track, to)
        }

        "drum_groove" => {
            let track = arg_track(app, args)?;
            let slot = section_index(args.get("section"), "section")?;
            if !is_drums(app, track)? {
                return Err(format!("track {track} is not a drum track: grooves go on drums"));
            }
            let groove = args["groove"].as_str().ok_or("give a groove id from list_sounds")?;
            let cat = catalog(app)?;
            if !cat["grooves"].as_array().is_some_and(|g| g.iter().any(|g| g["id"] == groove)) {
                let all: Vec<&str> = cat["grooves"].as_array().into_iter().flatten().filter_map(|g| g["id"].as_str()).collect();
                return Err(format!("there is no groove \"{groove}\". The grooves: {}.", all.join(", ")));
            }
            act(app, json!({ "type": "load_groove", "track": track, "slot": slot, "groove": groove }))?;
            loop_notes(app, track, slot)
        }

        "get_sound" => {
            let track = arg_track(app, args)?;
            sound_detail(app, track)
        }

        "shape_sound" => {
            let track = arg_track(app, args)?;
            let cat = catalog(app)?;
            let kind = project(app)?["tracks"][track]["instrument"]["type"].as_str().unwrap_or("").to_string();
            // anything that is not open yet becomes an editable copy of itself
            if !matches!(kind.as_str(), "macro" | "custom_synth" | "custom_kit") {
                act(app, json!({ "type": "design_sound", "track": track }))?;
            }
            let p = project(app)?;
            let inst = p["tracks"][track]["instrument"].clone();
            let kind = inst["type"].as_str().unwrap_or("").to_string();
            let pad = match (&args["pad"], kind.as_str()) {
                (Value::Null, _) => None,
                (v, "custom_kit") => {
                    let names = pads(app, track)?;
                    Some(match v {
                        Value::Number(n) => n.as_u64().map(|n| n as usize).filter(|n| *n < names.len()).ok_or("there is no pad with that number")?,
                        Value::String(s) => names.iter().position(|(n, _)| n.to_lowercase() == s.trim().to_lowercase()).ok_or_else(|| format!("this kit has no pad \"{s}\". Its pads: {}.", names.iter().map(|(n, _)| n.to_lowercase()).collect::<Vec<_>>().join(", ")))?,
                        _ => return Err("pad is a name or a number".into()),
                    })
                }
                _ => return Err("only a drum kit has pads".into()),
            };
            let specs = match kind.as_str() {
                "macro" => &cat["macro_params"],
                "custom_synth" => &cat["synth_params"],
                _ => &cat["drum_params"],
            };
            if let Some(params) = args["params"].as_object() {
                if kind == "custom_kit" && pad.is_none() && !params.is_empty() {
                    return Err("a kit is shaped one pad at a time: say which with `pad` (get_sound lists them)".into());
                }
                for (key, v) in params {
                    let spec = specs.as_array().and_then(|all| all.iter().find(|s| s["key"] == key.as_str())).ok_or_else(|| {
                        let keys: Vec<&str> = specs.as_array().into_iter().flatten().filter_map(|s| s["key"].as_str()).collect();
                        format!("this sound has no knob \"{key}\". Its knobs: {}.", keys.join(", "))
                    })?;
                    let value = match v {
                        Value::Number(n) => n.as_f64().unwrap_or(0.0),
                        // a choice by name: the engine "FM", a wave "saw"
                        Value::String(s) => spec["choices"].as_array().and_then(|c| c.iter().position(|c| c.as_str().is_some_and(|c| c.eq_ignore_ascii_case(s.trim())))).map(|i| i as f64).ok_or_else(|| {
                            let c: Vec<&str> = spec["choices"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
                            if c.is_empty() { format!("{key} takes a number") } else { format!("{key} is one of: {}", c.join(", ")) }
                        })?,
                        _ => return Err(format!("{key} takes a number")),
                    };
                    let mut a = json!({ "type": "set_sound_param", "track": track, "key": key, "value": value });
                    if let Some(pad) = pad {
                        a["pad"] = json!(pad);
                    }
                    act(app, a)?;
                }
            }
            if let Some(name) = args["name"].as_str() {
                let mut a = json!({ "type": "rename_sound", "track": track, "name": name });
                if let Some(pad) = pad {
                    a["pad"] = json!(pad);
                }
                act(app, a)?;
            }
            sound_detail(app, track)
        }

        "set_master" => {
            let mut master = project(app)?["master"].clone();
            for key in ["volume", "tape", "vinyl", "warmth", "space", "pump"] {
                if let Some(v) = args[key].as_f64() {
                    master[key] = json!(v.clamp(0.0, 1.0));
                }
            }
            act(app, json!({ "type": "set_master", "master": master }))?;
            Ok(overview(app)?["master"].clone())
        }

        "arrange" => {
            let list = args["sections"].as_array().ok_or("sections is a list of letters, like [\"A\",\"B\"]")?;
            let mut plan = Vec::new();
            for item in list {
                let (section, bars) = match item {
                    Value::Object(o) => (section_index(o.get("section"), "section")?, o.get("bars").and_then(Value::as_u64)),
                    other => (section_index(Some(other), "a section")?, None),
                };
                plan.push((section, bars));
            }
            let p = project(app)?;
            let ids: Vec<u64> = p["tracks"].as_array().map(|t| t.iter().map(|t| t["id"].as_u64().unwrap_or(0)).collect()).unwrap_or_default();
            for (section, _) in &plan {
                let any = p["tracks"].as_array().is_some_and(|t| t.iter().any(|t| t["slots"][*section]["notes"].is_array()));
                if !any {
                    return Err(format!("section {} has no loops yet: write some first", section_name(*section)));
                }
            }
            act(app, json!({ "type": "clear_song" }))?;
            let mut start = 0u64;
            for (section, bars) in plan {
                let longest = p["tracks"].as_array().into_iter().flatten().filter_map(|t| t["slots"][section]["bars"].as_u64()).max().unwrap_or(1);
                let bars = bars.unwrap_or_else(|| longest.max(4).div_ceil(longest) * longest);
                for (i, t) in p["tracks"].as_array().into_iter().flatten().enumerate() {
                    if t["slots"][section]["notes"].is_array() && i < ids.len() {
                        act(app, json!({ "type": "add_clip", "track": i, "slot": section, "start": start, "bars": bars }))?;
                    }
                }
                start += bars;
            }
            if let Some(looping) = args["loop"].as_bool() {
                act(app, json!({ "type": "set_song_looping", "looping": looping }))?;
            }
            Ok(overview(app)?["song"].clone())
        }

        "play" => {
            if args["song"] == true {
                if project(app)?["song"]["clips"].as_array().map_or(true, Vec::is_empty) {
                    return Err("the song is empty: arrange some sections first, or play one section".into());
                }
                act(app, json!({ "type": "stop" }))?;
                act(app, json!({ "type": "set_song_mode", "on": true }))?;
                act(app, json!({ "type": "seek", "bar": 0 }))?;
                act(app, json!({ "type": "play" }))?;
                return Ok(json!("playing the song"));
            }
            let section = match args.get("section") {
                Some(v) if !v.is_null() => section_index(Some(v), "section")?,
                _ => 0,
            };
            act(app, json!({ "type": "set_song_mode", "on": false }))?;
            act(app, json!({ "type": "launch_scene", "scene": section }))?;
            act(app, json!({ "type": "play" }))?;
            Ok(json!(format!("playing section {}", section_name(section))))
        }

        "stop" => {
            act(app, json!({ "type": "stop" }))?;
            Ok(json!("stopped"))
        }

        "undo" => {
            let steps = args["steps"].as_u64().unwrap_or(1).clamp(1, 50);
            let mut done = 0;
            for _ in 0..steps {
                if project(app)?["history"]["undo"] != true {
                    break;
                }
                act(app, json!({ "type": "undo" }))?;
                done += 1;
            }
            let mut out = overview(app)?;
            out["undone"] = json!(done);
            Ok(out)
        }

        _ => Err(format!("no such tool: {name}")),
    }
}

fn song_settings(app: &mut App, args: &Value) -> Result<(), String> {
    if let Some(name) = args["name"].as_str() {
        act(app, json!({ "type": "set_name", "name": name }))?;
    }
    if let Some(bpm) = args["bpm"].as_f64() {
        act(app, json!({ "type": "set_bpm", "bpm": bpm.clamp(20.0, 300.0) }))?;
    }
    if let Some(swing) = args["swing"].as_f64() {
        act(app, json!({ "type": "set_swing", "swing": swing.clamp(0.0, 1.0) }))?;
    }
    if args["key"].is_string() || args["scale"].is_string() {
        let p = project(app)?;
        let root = match args["key"].as_str() {
            Some(k) => parse_root(k).ok_or_else(|| format!("\"{k}\" is not a root note: write it like \"A\" or \"F#\""))? as u64,
            None => p["scale"]["root"].as_u64().unwrap_or(0),
        };
        let kind = match args["scale"].as_str() {
            Some(s) => {
                let cat = catalog(app)?;
                let id = s.trim().to_lowercase().replace(' ', "_");
                if !cat["scales"].as_array().is_some_and(|all| all.iter().any(|x| x["id"] == id.as_str())) {
                    let all: Vec<&str> = cat["scales"].as_array().into_iter().flatten().filter_map(|x| x["id"].as_str()).collect();
                    return Err(format!("there is no scale \"{s}\". The scales: {}.", all.join(", ")));
                }
                json!(id)
            }
            None => p["scale"]["kind"].clone(),
        };
        act(app, json!({ "type": "set_scale", "root": root, "kind": kind, "track": null }))?;
    }
    Ok(())
}

fn sound_detail(app: &mut App, track: usize) -> Result<Value, String> {
    let p = project(app)?;
    let cat = catalog(app)?;
    let t = &p["tracks"][track];
    let inst = &t["instrument"];
    let kind = inst["type"].as_str().unwrap_or("");
    let knob = |spec: &Value, value: &Value| {
        // values are stored as 32-bit floats: show them as they were set, not their binary tail
        let shown = value.as_f64().map_or(value.clone(), |v| json!((v * 10000.0).round() / 10000.0));
        let mut o = json!({ "key": spec["key"], "value": shown, "min": spec["min"], "max": spec["max"], "does": spec["help"] });
        if spec["choices"].as_array().is_some_and(|c| !c.is_empty()) {
            o["choices"] = spec["choices"].clone();
            if let Some(i) = value.as_f64() {
                o["value"] = spec["choices"][i.round() as usize].clone();
            }
        }
        if spec["unit"].as_str().is_some_and(|u| !u.is_empty()) {
            o["unit"] = spec["unit"].clone();
        }
        o
    };
    let knobs_for = |specs: &Value, values: &Value| -> Vec<Value> { specs.as_array().into_iter().flatten().filter(|s| !values[s["key"].as_str().unwrap_or("")].is_null()).map(|s| knob(s, &values[s["key"].as_str().unwrap_or("")])).collect() };
    let mut out = json!({ "track": track, "name": t["name"], "sound": sound_id(inst, &cat), "track_knobs": knobs_of(t) });
    match kind {
        "macro" => {
            out["is"] = json!("a synth sound: shape it with shape_sound");
            out["knobs"] = json!(knobs_for(&cat["macro_params"], &inst["patch"]));
        }
        "custom_synth" => {
            out["is"] = json!("a designed synth: shape it with shape_sound");
            out["knobs"] = json!(knobs_for(&cat["synth_params"], &inst["patch"]));
        }
        "custom_kit" => {
            out["is"] = json!("a designed kit: shape one pad at a time with shape_sound and `pad`");
            let names = pads(app, track)?;
            let list: Vec<Value> = inst["pads"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
                .map(|(i, pad)| {
                    let values = if pad["sound"].is_object() { &pad["sound"] } else { pad };
                    json!({ "pad": i, "name": names.get(i).map(|(n, _)| n.to_lowercase()), "pitch": names.get(i).map(|(_, p)| pitch_name(*p as u64)), "knobs": knobs_for(&cat["drum_params"], values).iter().map(|k| (k["key"].as_str().unwrap_or("").to_string(), k["value"].clone())).collect::<Map<String, Value>>() })
                })
                .collect();
            out["pads"] = json!(list);
            out["pad_knobs"] = cat["drum_params"].as_array().map(|all| all.iter().map(|s| json!({ "key": s["key"], "min": s["min"], "max": s["max"], "does": s["help"] })).collect::<Vec<_>>()).into();
        }
        _ => {
            out["is"] = json!(if kind == "drums" { "a kit as it comes: shape_sound turns it into an editable copy, pad by pad" } else { "a sound as it comes: shape_sound turns it into an editable synth copy; the track's seven knobs work on it as it is" });
            if kind == "drums" {
                let names = pads(app, track)?;
                out["pads"] = json!(names.iter().enumerate().map(|(i, (n, p))| json!({ "pad": i, "name": n.to_lowercase(), "pitch": pitch_name(*p as u64) })).collect::<Vec<_>>());
            }
        }
    }
    Ok(out)
}
