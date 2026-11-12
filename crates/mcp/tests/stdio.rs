//! The server as a client meets it: a process, a line in, a line out.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

struct Server {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    id: u64,
}

impl Server {
    fn start(args: &[&str]) -> Server {
        let mut child = Command::new(env!("CARGO_BIN_EXE_mjolnir-mcp")).args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Server { child, stdin, stdout, id: 0 }
    }
    fn tool(&mut self, name: &str, args: Value) -> (Value, bool) {
        self.id += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": self.id, "method": "tools/call", "params": { "name": name, "arguments": args } });
        writeln!(self.stdin, "{msg}").unwrap();
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        let v: Value = serde_json::from_str(&line).expect("one JSON message per line");
        assert_eq!(v["id"], self.id);
        let text = v["result"]["content"][0]["text"].as_str().unwrap_or_else(|| panic!("{v}"));
        (serde_json::from_str(text).unwrap_or_else(|_| json!(text)), v["result"]["isError"] == true)
    }
    fn ok(&mut self, name: &str, args: Value) -> Value {
        let (v, err) = self.tool(name, args);
        assert!(!err, "{name}: {v}");
        v
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn a_set_is_written_bounced_saved_and_opened_again() {
    let dir = std::env::temp_dir().join(format!("mjolnir-mcp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let set = dir.join("piece.mjolnir");
    let wav = dir.join("out/piece.wav");
    let mid = dir.join("piece.mid");
    let (set_s, wav_s, mid_s) = (set.to_str().unwrap(), wav.to_str().unwrap(), mid.to_str().unwrap());

    let mut s = Server::start(&[set_s]);
    s.ok("new_set", json!({ "name": "Piece", "bpm": 96, "key": "C", "scale": "minor" }));
    s.ok("add_track", json!({ "sound": "kit:shadow" }));
    s.ok("add_track", json!({ "sound": "preset:soft_piano" }));

    // nothing written yet: there is nothing to bounce, and it says how to get there
    let (why, failed) = s.tool("render", json!({ "path": wav_s }));
    assert!(failed && why.as_str().unwrap().contains("arrange"), "{why}");

    s.ok("drum_groove", json!({ "track": 0, "section": "A", "groove": "boom_bap" }));
    s.ok("write_loop", json!({ "track": 1, "section": "A", "bars": 1, "notes": [{ "pitch": "C4", "start": 0, "length": 2 }, { "pitch": "Eb4", "start": 2, "length": 2 }] }));

    // one section, without a timeline
    let r = s.ok("render", json!({ "path": wav_s, "section": "A", "bars": 2 }));
    assert_eq!(r["bars"], 2);
    assert!(r["peak_db"].as_f64().is_some_and(|d| d < 0.0 && d > -30.0), "{r}");
    assert!(r["average_db"].as_f64().is_some_and(|d| d < r["peak_db"].as_f64().unwrap()), "{r}");
    let bytes = std::fs::read(&wav).unwrap();
    assert_eq!(&bytes[..4], b"RIFF");
    assert!(bytes.len() > 400_000);
    // bouncing a section leaves the set's own timeline alone
    assert_eq!(s.ok("get_set", json!({}))["song"]["bars"], 0);

    // a muted mix is silence, and says so (once the master's vinyl crackle is off: that never stops)
    s.ok("set_master", json!({ "vinyl": 0 }));
    s.ok("set_track", json!({ "track": 0, "mute": true }));
    let one = s.ok("render", json!({ "path": wav_s, "section": "A" }));
    assert!(one["peak_db"].as_f64().is_some(), "the piano still sounds: {one}");
    s.ok("set_track", json!({ "track": 1, "mute": true }));
    assert_eq!(s.ok("render", json!({ "path": wav_s, "section": "A" }))["peak_db"], "silence");
    s.ok("undo", json!({ "steps": 3 }));

    s.ok("arrange", json!({ "sections": ["A", "A"] }));
    assert_eq!(s.ok("render", json!({ "path": wav_s }))["bars"], 8);
    assert!(s.ok("export_midi", json!({ "path": mid_s }))["bytes"].as_u64().unwrap() > 50);
    assert_eq!(&std::fs::read(&mid).unwrap()[..4], b"MThd");

    // a wrong kind of file is refused before anything is written
    let (why, failed) = s.tool("render", json!({ "path": dir.join("piece.txt").to_str().unwrap() }));
    assert!(failed && why.as_str().unwrap().contains(".wav"));

    // saved to the file it was started with, and there again in a new process
    let saved = s.ok("save_set", json!({}));
    assert_eq!(saved["saved"], set_s);
    drop(s);
    let mut again = Server::start(&[set_s]);
    let got = again.ok("get_set", json!({}));
    assert_eq!(got["name"], "Piece");
    assert_eq!(got["bpm"], 96.0);
    assert_eq!(got["tracks"][1]["sound"], "preset:soft_piano");
    assert_eq!(got["tracks"][1]["loops"]["A"]["notes"], 2);
    assert_eq!(got["song"]["bars"], 8);
    let opened = again.ok("open_set", json!({ "path": set_s }));
    assert_eq!(opened["name"], "Piece");
    let _ = std::fs::remove_dir_all(&dir);
}
