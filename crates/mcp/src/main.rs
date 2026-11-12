//! Mjolnir for an agent, with no window: a Model Context Protocol server on
//! standard input and output. It holds one set in memory, the same set the
//! app would, and adds what only a program with a disk can do: open and save
//! `.mjolnir` files, bounce the music to a WAV file, write a MIDI file.
//!
//!   mjolnir-mcp                 start with an empty set
//!   mjolnir-mcp song.mjolnir    open that set (and save back to it)
//!
//! One JSON-RPC message per line in, one per line out. Nothing else is ever
//! written to standard output; what a person may want to read goes to
//! standard error.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use mjolnir_app::mcp::{self, Host};
use mjolnir_app::{Action, App};
use mjolnir_engine::controller::Controller;
use mjolnir_engine::engine::Engine;
use mjolnir_engine::export;
use serde_json::{json, Value};

const SAMPLE_RATE: u32 = 48000;

/// The disk: where the set lives, and where bounces go.
struct Disk {
    /// The file the set was opened from or last saved to.
    file: Option<PathBuf>,
}

fn act(app: &mut App, action: Value) -> Result<Value, String> {
    let action: Action = serde_json::from_value(action).map_err(|e| format!("internal: {e}"))?;
    app.handle(action)
}

fn path_arg(args: &Value, ext: &[&str]) -> Result<PathBuf, String> {
    let p = args["path"].as_str().ok_or("give a path")?;
    let path = PathBuf::from(p);
    let ok = path.extension().and_then(|e| e.to_str()).is_some_and(|e| ext.iter().any(|x| x.eq_ignore_ascii_case(e)));
    if !ok {
        return Err(format!("the file's name must end in .{}", ext.join(" or .")));
    }
    Ok(path)
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("could not make the folder {}: {e}", dir.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("could not write {}: {e}", path.display()))
}

fn db(v: f32) -> Value {
    if v <= 1e-6 {
        json!("silence")
    } else {
        json!(((20.0 * (v as f64).log10()) * 10.0).round() / 10.0)
    }
}

impl Disk {
    fn open(&mut self, app: &mut App, path: &Path) -> Result<(), String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
        act(app, json!({ "type": "open_project", "json": text })).map_err(|e| format!("{} is not a Mjolnir set: {e}", path.display()))?;
        self.file = Some(path.to_path_buf());
        Ok(())
    }

    /// The set as it is, in a controller of its own, laid out to be bounced:
    /// its song, or one section repeated.
    fn staged(&self, app: &mut App, args: &Value) -> Result<Controller, String> {
        let text = act(app, json!({ "type": "save_project" }))?["json"].as_str().unwrap_or_default().to_string();
        let (_engine, endpoints) = Engine::new(SAMPLE_RATE as f32);
        let mut ctrl = Controller::new(endpoints, SAMPLE_RATE as f32);
        ctrl.open_json(&text)?;
        // mute and solo are how the set is being listened to, not part of the file: carry them over
        for (i, t) in app.ctrl.project.tracks.iter().enumerate() {
            ctrl.set_mute(i, t.mute)?;
            ctrl.set_solo(i, t.solo)?;
        }
        if let Some(section) = args["section"].as_str() {
            let c = section.trim().bytes().next().map(|b| b.to_ascii_uppercase()).filter(|b| (b'A'..=b'H').contains(b)).ok_or("section must be a letter A to H")?;
            ctrl.clear_song();
            ctrl.set_song_looping(false);
            ctrl.append_scene((c - b'A') as usize, args["bars"].as_u64().map(|b| b.clamp(1, 64) as u32)).map_err(|e| format!("section {}: {e}", c as char))?;
        } else if ctrl.project.song.length_bars() == 0 {
            return Err("the song is empty: arrange some sections first, or bounce one section with `section`".into());
        }
        Ok(ctrl)
    }
}

impl Host for Disk {
    fn tools(&self) -> Vec<Value> {
        let tool = |name: &str, description: &str, properties: Value, required: &[&str]| {
            json!({ "name": name, "description": description, "inputSchema": { "type": "object", "properties": properties, "required": required, "additionalProperties": false } })
        };
        vec![
            tool("open_set", "Open a set from a .mjolnir file. It replaces the set in memory: save first if that one matters.", json!({ "path": { "type": "string", "description": "The file to open." } }), &["path"]),
            tool("save_set", "Save the set to a .mjolnir file, which the Mjolnir app opens. With no path it goes back to the file it came from.", json!({ "path": { "type": "string", "description": "Where to save. Must end in .mjolnir." } }), &[]),
            tool("render", "Bounce the music to a WAV file (48 kHz, 24-bit, stereo) so it can be listened to: the whole song, or one section. Answers how long it is and how loud (peak and average, in dB below full scale), which is the only way to check the result without ears: silence means nothing was written or everything is muted; a peak at 0 means it is clipping.", json!({
                "path": { "type": "string", "description": "Where to write it. Must end in .wav." },
                "section": { "type": "string", "description": "A letter A to H: bounce that section's loops instead of the song." },
                "bars": { "type": "integer", "minimum": 1, "maximum": 64, "description": "With `section`: for how many bars. Default: its longest loop, rounded up to 4." },
                "track": { "type": "integer", "minimum": 0, "description": "Only this track (a stem)." }
            }), &["path"]),
            tool("export_midi", "Write the song's notes as a standard MIDI file, one track per track, for a DAW.", json!({ "path": { "type": "string", "description": "Where to write it. Must end in .mid." } }), &["path"]),
        ]
    }

    fn call(&mut self, app: &mut App, name: &str, args: &Value) -> Option<Result<Value, String>> {
        Some(match name {
            "open_set" => (|| {
                let path = path_arg(args, &["mjolnir"])?;
                self.open(app, &path)?;
                mcp::handle(app, &mut mcp::NoHost, r#"{"jsonrpc":"2.0","id":0,"method":"tools/call","params":{"name":"get_set","arguments":{}}}"#)
                    .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                    .and_then(|v| v["result"]["content"][0]["text"].as_str().and_then(|t| serde_json::from_str::<Value>(t).ok()))
                    .ok_or_else(|| "opened, but the set could not be read back".to_string())
            })(),
            "save_set" => (|| {
                let path = match args["path"].as_str() {
                    Some(_) => path_arg(args, &["mjolnir"])?,
                    None => self.file.clone().ok_or("this set has no file yet: give a path ending in .mjolnir")?,
                };
                let text = act(app, json!({ "type": "save_project" }))?["json"].as_str().unwrap_or_default().to_string();
                write(&path, text.as_bytes())?;
                self.file = Some(path.clone());
                Ok(json!({ "saved": path.display().to_string(), "bytes": text.len() }))
            })(),
            "render" => (|| {
                let path = path_arg(args, &["wav"])?;
                let ctrl = self.staged(app, args)?;
                let solo = match args["track"].as_u64() {
                    Some(t) if (t as usize) < ctrl.project.tracks.len() => Some(t as usize),
                    Some(t) => return Err(format!("there is no track {t}")),
                    None => None,
                };
                let audio = export::render(&ctrl.project, ctrl.samples(), SAMPLE_RATE, solo, 2.0);
                let peak = audio.iter().fold(0.0f32, |a, s| a.max(s.abs()));
                let rms = if audio.is_empty() { 0.0 } else { (audio.iter().map(|s| (*s as f64) * (*s as f64)).sum::<f64>() / audio.len() as f64).sqrt() as f32 };
                write(&path, &export::wav_bytes(&audio, SAMPLE_RATE)?)?;
                let bars = ctrl.project.song.length_bars();
                Ok(json!({
                    "wrote": path.display().to_string(),
                    "bars": bars,
                    "seconds": (audio.len() as f64 / 2.0 / SAMPLE_RATE as f64 * 10.0).round() / 10.0,
                    "peak_db": db(peak),
                    "average_db": db(rms),
                }))
            })(),
            "export_midi" => (|| {
                let path = path_arg(args, &["mid", "midi"])?;
                let ctrl = self.staged(app, &json!({}))?;
                let bytes = export::to_midi(&ctrl.project);
                write(&path, &bytes)?;
                Ok(json!({ "wrote": path.display().to_string(), "bytes": bytes.len() }))
            })(),
            _ => return None,
        })
    }
}

fn main() {
    let (_engine, endpoints) = Engine::new(SAMPLE_RATE as f32);
    let mut app = App::new(endpoints, SAMPLE_RATE as f32);
    let mut disk = Disk { file: None };
    match std::env::args().nth(1) {
        Some(arg) if arg == "--help" || arg == "-h" => {
            eprintln!("mjolnir-mcp {}: Mjolnir as a Model Context Protocol server, on standard input and output.\n\n  mjolnir-mcp                 start with an empty set\n  mjolnir-mcp song.mjolnir    open that set (and save back to it)", env!("CARGO_PKG_VERSION"));
            return;
        }
        Some(arg) if arg == "--version" => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            return;
        }
        Some(file) => {
            let path = PathBuf::from(&file);
            if path.exists() {
                if let Err(e) = disk.open(&mut app, &path) {
                    eprintln!("mjolnir-mcp: {e}");
                    std::process::exit(1);
                }
            } else {
                // a file that is not there yet is where the set will be saved
                disk.file = Some(path);
            }
        }
        None => {}
    }
    eprintln!("mjolnir-mcp {}: listening on standard input", env!("CARGO_PKG_VERSION"));
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(answer) = mcp::handle(&mut app, &mut disk, &line) {
            if writeln!(out, "{answer}").and_then(|_| out.flush()).is_err() {
                break;
            }
        }
    }
}
