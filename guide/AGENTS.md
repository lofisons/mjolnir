# An agent at the instrument

Mjolnir speaks the [Model Context Protocol](https://modelcontextprotocol.io), so an agent (Claude, or any client that speaks it) can make music in it: write loops, build a drum part, shape a synth, arrange a song. It uses the same door a person's hands use, so what an agent does can be undone, saved and played like anything else.

There are two ways in.

## In the app, while you listen

The desktop app can let an agent on the same computer play the set that is open. What it writes appears on screen and sounds at once, and you can keep playing alongside it.

1. In the menu: **Agent → Let an agent play (MCP)**. It is off until you turn it on, and it stays as you left it.
2. **Agent → Copy its address**. It is `http://127.0.0.1:47800/mcp` unless that port was taken.
3. Give the address to your agent. In Claude Code:

```bash
claude mcp add --transport http mjolnir http://127.0.0.1:47800/mcp
```

To open the door for one run only, start the app with `--mcp`.

The app listens on this computer alone (`127.0.0.1`), and refuses anything that comes from a web page. There is no password: anything running as you on this computer can reach it while it is on. Turn it off when you are done.

## On its own, as a file

`mjolnir-mcp` is the instrument with no window: a small program an agent starts itself. It holds one set, saves it as a `.mjolnir` file the app opens, and bounces audio to WAV so the result can be heard.

```bash
cargo build --release -p mjolnir-mcp
```

```bash
claude mcp add mjolnir -- /path/to/mjolnir/target/release/mjolnir-mcp /path/to/song.mjolnir
```

For any other client, the same thing as configuration:

```json
{ "mcpServers": { "mjolnir": { "command": "/path/to/mjolnir-mcp", "args": ["/path/to/song.mjolnir"] } } }
```

The file is opened if it exists and is where `save_set` writes. Nothing is saved until the agent calls `save_set`.

## What an agent can do

| Tool | What it does |
|---|---|
| `get_set` | The whole set at a glance: tempo, key, tracks, which sections have loops, the song, the master |
| `list_sounds` | Every sound, the ready-made drum grooves, the scales |
| `new_set`, `set_song` | Start from silence; name, tempo, swing, key |
| `add_track`, `set_track`, `remove_track` | Tracks: their sound, name, seven knobs, pan, mute, solo |
| `write_loop`, `read_loop`, `clear_loop`, `copy_loop` | A track's notes in a section, in beats and note names (or pad names on drums) |
| `drum_groove` | A ready-made beat or hat line into a drum loop |
| `get_sound`, `shape_sound` | A sound from the inside: the synth's engine and knobs, a designed copy of any other sound, a kit pad by pad |
| `set_master` | Volume, tape, vinyl, warmth, room size, pump |
| `arrange` | The sections of the song, in order |
| `play`, `stop` | The transport of the app that is listening |
| `undo` | Take back the last tool calls; each one is one step |
| `open_set`, `save_set`, `render`, `export_midi` | `mjolnir-mcp` only: files, a WAV bounce with its loudness, a MIDI file |

Time is in beats (quarter notes) from the start of a loop. Pitches are note names (`A2`, `F#3`; `C4` is middle C) or MIDI numbers; on a drum track, a pad's name (`kick`, `snare`, `hat`). Sections are letters `A` to `H`, tracks are numbered from 0. When something is wrong the answer says what and what to try.

An agent has no ears: `render` answers how long the bounce is and how loud, which is enough to tell silence or clipping from music, but not whether it is good. That part is yours.

## How it is made

The tools live in `crates/app/src/mcp.rs`, on top of the same actions the interface sends, so both hosts share one implementation: the app carries each message to its engine (the `mcp` action), and `mjolnir-mcp` (`crates/mcp`) runs the engine natively and adds the tools that need a disk. `cargo test -p mjolnir-app -p mjolnir-mcp` drives both through the protocol.

## Where it stands

Tried here: the tools through the protocol in both hosts; a piece composed, bounced, saved and opened again by a client over stdio; the desktop app on Linux taking messages over HTTP and playing them. Not tried yet: a session with a commercial agent client on Windows or macOS. The 0.2.0 installers do not have the Agent menu yet; it comes with the next release.
