//! MIDI input: parse raw messages and route them to the project.
//!
//! Routing is deliberately simple (no MIDI learn, no settings):
//! - keys play the selected track (chromatic, with velocity);
//! - notes on channel 10 (the usual pad/drum channel) or on a drum track play
//!   drum pads: note 36 is pad 1, 37 pad 2… (any note wraps onto the 16 pads);
//! - a sustain pedal (CC 64) holds melodic notes;
//! - knobs: the CCs a small controller sends out of the box (e.g. the
//!   MiniLab mk2's factory map) drive the macros of the selected track and
//!   the lo-fi master. See [`cc_target`].
//!
//! `MidiHub` (feature `midi-io`) connects to every hardware input, also the
//! ones plugged in while the app runs, and hands raw bytes to a callback.

use std::time::{Duration, Instant};

use crate::controller::Controller;
use crate::model::{MacroParam, Master, TrackKind};

/// A channel voice message we care about. Channels are 0-based (10 → 9).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MidiMsg {
    NoteOn { ch: u8, note: u8, velocity: u8 },
    NoteOff { ch: u8, note: u8 },
    Cc { ch: u8, cc: u8, value: u8 },
}

pub const DRUM_CHANNEL: u8 = 9;
const DRUM_BASE: i32 = 36;
/// After this long without knob movement the burst is over: release the
/// automation "touch" and let the UI refresh.
const KNOB_IDLE: Duration = Duration::from_millis(350);

pub fn parse(bytes: &[u8]) -> Option<MidiMsg> {
    let (&status, data) = bytes.split_first()?;
    let ch = status & 0x0f;
    let d = |i: usize| data.get(i).map(|b| b & 0x7f);
    match status & 0xf0 {
        0x90 => {
            let (note, velocity) = (d(0)?, d(1)?);
            Some(if velocity == 0 {
                MidiMsg::NoteOff { ch, note }
            } else {
                MidiMsg::NoteOn { ch, note, velocity }
            })
        }
        0x80 => Some(MidiMsg::NoteOff { ch, note: d(0)? }),
        0xb0 => Some(MidiMsg::Cc {
            ch,
            cc: d(0)?,
            value: d(1)?,
        }),
        _ => None,
    }
}

/// What a knob (control change) moves.
#[derive(Debug, Clone, Copy)]
pub enum CcTarget {
    Macro(MacroParam),
    Master(fn(&mut Master) -> &mut f32),
    Humanize,
    Swing,
    Sustain,
    AllOff,
}

/// Factory CCs of common small controllers. Top row of the MiniLab mk2
/// (knobs 2-8: 74 71 76 77 93 73 75) follows the macro order; knob 10-15
/// (18 19 16 17 91 79) control the master and the feel.
pub fn cc_target(cc: u8) -> Option<CcTarget> {
    use MacroParam::*;
    Some(match cc {
        74 => CcTarget::Macro(Cutoff),
        71 => CcTarget::Macro(Resonance),
        76 => CcTarget::Macro(Drive),
        77 => CcTarget::Macro(Saturation),
        93 => CcTarget::Macro(Reverb),
        73 => CcTarget::Macro(Delay),
        75 | 7 => CcTarget::Macro(Volume),
        18 => CcTarget::Master(|m| &mut m.vinyl),
        19 => CcTarget::Master(|m| &mut m.tape),
        16 => CcTarget::Master(|m| &mut m.warmth),
        17 => CcTarget::Master(|m| &mut m.volume),
        91 => CcTarget::Humanize,
        79 => CcTarget::Swing,
        64 => CcTarget::Sustain,
        120 | 123 => CcTarget::AllOff,
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Held {
    Pad { track: usize, pad: usize },
    Note { track: usize, pitch: u8 },
}

/// Control-thread MIDI state kept inside the `Controller`.
#[derive(Default)]
pub struct MidiState {
    /// Track the keys play (the one selected in the UI).
    pub focus: usize,
    /// Sounding notes by (channel, note), so a note-off reaches the track
    /// that started it even if the selection changed in between.
    held: Vec<((u8, u8), Held)>,
    sustain: bool,
    sustained: Vec<Held>,
    touching: Vec<(usize, MacroParam)>,
    last_knob: Option<Instant>,
}

impl Controller {
    /// The track the keyboard plays (the UI's selected track).
    pub fn set_focus(&mut self, track: usize) {
        self.midi.focus = track;
    }

    /// Handle one raw MIDI message. Unknown messages are ignored.
    pub fn midi_message(&mut self, bytes: &[u8]) {
        let Some(msg) = parse(bytes) else { return };
        let tracks = self.project.tracks.len();
        if tracks == 0 {
            return;
        }
        let focus = self.midi.focus.min(tracks - 1);
        match msg {
            MidiMsg::NoteOn { ch, note, velocity } => {
                // a repeated note-on without its note-off: end the old one first
                self.release_key(ch, note, true);
                let vel = (velocity as f32 / 127.0).clamp(0.05, 1.0);
                let held = match self.drum_target(ch, focus) {
                    Some(track) => {
                        let pad = (note as i32 - DRUM_BASE).rem_euclid(16) as usize;
                        let _ = self.pad_on(track, pad, vel);
                        Held::Pad { track, pad }
                    }
                    None => {
                        let _ = self.note_on(focus, note, vel);
                        Held::Note {
                            track: focus,
                            pitch: note,
                        }
                    }
                };
                self.midi.held.push(((ch, note), held));
            }
            MidiMsg::NoteOff { ch, note } => self.release_key(ch, note, false),
            MidiMsg::Cc { cc, value, .. } => self.midi_cc(cc, value, focus),
        }
    }

    /// Drums take notes on channel 10, or any note while a drum track is selected.
    fn drum_target(&self, ch: u8, focus: usize) -> Option<usize> {
        let is_drums = |i: usize| self.project.tracks[i].kind == TrackKind::Drums;
        if is_drums(focus) {
            Some(focus)
        } else if ch == DRUM_CHANNEL {
            (0..self.project.tracks.len()).find(|&i| is_drums(i))
        } else {
            None
        }
    }

    fn release_key(&mut self, ch: u8, note: u8, force: bool) {
        let Some(i) = self.midi.held.iter().position(|(k, _)| *k == (ch, note)) else {
            return;
        };
        let (_, held) = self.midi.held.swap_remove(i);
        if self.midi.sustain && !force && matches!(held, Held::Note { .. }) {
            self.midi.sustained.push(held);
        } else {
            self.end(held);
        }
    }

    fn end(&mut self, held: Held) {
        let _ = match held {
            Held::Pad { track, pad } => self.pad_off(track, pad),
            Held::Note { track, pitch } => self.note_off(track, pitch),
        };
    }

    fn all_midi_notes_off(&mut self) {
        let held: Vec<Held> = self.midi.held.drain(..).map(|(_, h)| h).collect();
        let sustained: Vec<Held> = self.midi.sustained.drain(..).collect();
        for h in held.into_iter().chain(sustained) {
            self.end(h);
        }
    }

    fn midi_cc(&mut self, cc: u8, value: u8, focus: usize) {
        let Some(target) = cc_target(cc) else { return };
        let v = value as f32 / 127.0;
        match target {
            CcTarget::Sustain => {
                self.midi.sustain = value >= 64;
                if !self.midi.sustain {
                    for h in std::mem::take(&mut self.midi.sustained) {
                        self.end(h);
                    }
                }
                return;
            }
            CcTarget::AllOff => {
                self.all_midi_notes_off();
                return;
            }
            _ => {}
        }
        // first movement of a knob burst: one undo step for the whole gesture
        if self.midi.last_knob.is_none() {
            self.checkpoint();
        }
        self.midi.last_knob = Some(Instant::now());
        match target {
            CcTarget::Macro(param) => {
                if !self.midi.touching.contains(&(focus, param)) {
                    let _ = self.touch(focus, param, true);
                    self.midi.touching.push((focus, param));
                }
                let _ = self.set_macro(focus, param, v);
            }
            CcTarget::Master(field) => {
                let mut m = self.project.master;
                *field(&mut m) = v;
                self.set_master(m);
            }
            CcTarget::Humanize => {
                let _ = self.set_humanize(focus, v);
            }
            CcTarget::Swing => self.set_swing(v),
            CcTarget::Sustain | CcTarget::AllOff => {}
        }
    }

    /// Called from `poll`: once the knobs rest, hand them back to automation
    /// and tell the UI the project changed.
    pub(crate) fn midi_tick(&mut self) -> bool {
        match self.midi.last_knob {
            Some(t) if t.elapsed() >= KNOB_IDLE => {
                self.midi.last_knob = None;
                for (track, param) in std::mem::take(&mut self.midi.touching) {
                    let _ = self.touch(track, param, false);
                }
                true
            }
            _ => false,
        }
    }

    /// Track indices shifted (track removed, project loaded): forget held notes.
    pub(crate) fn midi_reset(&mut self) {
        self.midi.held.clear();
        self.midi.sustained.clear();
        self.midi.touching.clear();
    }
}

#[cfg(feature = "midi-io")]
pub use hub::MidiHub;

#[cfg(feature = "midi-io")]
mod hub {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use midir::{Ignore, MidiInput, MidiInputConnection};

    const SCAN_EVERY: Duration = Duration::from_millis(1500);
    const NO_MIDI_RETRY: Duration = Duration::from_secs(10);

    type Connection = MidiInputConnection<()>;
    type Sink = Arc<dyn Fn(&[u8]) + Send + Sync>;

    /// Keeps a connection to every MIDI input (hot-plug included).
    pub struct MidiHub {
        devices: Arc<Mutex<Vec<String>>>,
    }

    impl MidiHub {
        /// Start watching inputs; `sink` receives every raw message, on a
        /// MIDI thread. Never fails: without a MIDI system there are simply
        /// no devices.
        pub fn start(sink: impl Fn(&[u8]) + Send + Sync + 'static) -> MidiHub {
            let devices = Arc::new(Mutex::new(Vec::new()));
            let shown = devices.clone();
            let sink: Sink = Arc::new(sink);
            let _ = std::thread::Builder::new()
                .name("yggdrasil-midi".into())
                .spawn(move || {
                    let mut open: Vec<(String, Connection)> = Vec::new();
                    loop {
                        let Some(available) = scan() else {
                            // no MIDI system (e.g. no sound card): check rarely
                            std::thread::sleep(NO_MIDI_RETRY);
                            continue;
                        };
                        // unplugged: drop the connection
                        open.retain(|(name, _)| available.contains(name));
                        for name in available {
                            if open.iter().any(|(n, _)| *n == name) {
                                continue;
                            }
                            if let Some(conn) = connect(&name, sink.clone()) {
                                open.push((name, conn));
                            }
                        }
                        if let Ok(mut d) = shown.lock() {
                            *d = open.iter().map(|(n, _)| pretty(n)).collect();
                        }
                        std::thread::sleep(SCAN_EVERY);
                    }
                });
            MidiHub { devices }
        }

        /// Names of the connected controllers.
        pub fn devices(&self) -> Vec<String> {
            self.devices.lock().map(|d| d.clone()).unwrap_or_default()
        }
    }

    /// Hardware input ports (the system "Midi Through" loopback is skipped).
    fn scan() -> Option<Vec<String>> {
        let input = MidiInput::new("Yggdrasil scan").ok()?;
        Some(
            input
                .ports()
                .iter()
                .filter_map(|p| input.port_name(p).ok())
                .filter(|n| !n.contains("Midi Through") && !n.starts_with("Yggdrasil"))
                .collect(),
        )
    }

    fn connect(name: &str, sink: Sink) -> Option<Connection> {
        let mut input = MidiInput::new("Yggdrasil").ok()?;
        input.ignore(Ignore::All);
        let port = input
            .ports()
            .into_iter()
            .find(|p| input.port_name(p).ok().as_deref() == Some(name))?;
        input
            .connect(&port, "Yggdrasil in", move |_, bytes, _| sink(bytes), ())
            .ok()
    }

    /// "Arturia MiniLab mkII:Arturia MiniLab mkII MIDI 1 20:0" → "Arturia MiniLab mkII".
    fn pretty(port: &str) -> String {
        port.split(':').next().unwrap_or(port).trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Command, Engine};

    fn controller() -> (Engine, Controller) {
        let (engine, ep) = Engine::new(44100.0);
        (engine, Controller::new(ep, 44100.0))
    }

    fn drums_and_melodic(c: &Controller) -> (usize, usize) {
        let ts = &c.project.tracks;
        let d = ts.iter().position(|t| t.kind == TrackKind::Drums).unwrap();
        let m = ts
            .iter()
            .position(|t| t.kind == TrackKind::Melodic)
            .unwrap();
        (d, m)
    }

    #[test]
    fn parses_voice_messages() {
        assert_eq!(
            parse(&[0x90, 60, 100]),
            Some(MidiMsg::NoteOn {
                ch: 0,
                note: 60,
                velocity: 100
            })
        );
        // note-on with velocity 0 is a note-off (running-status keyboards)
        assert_eq!(
            parse(&[0x99, 36, 0]),
            Some(MidiMsg::NoteOff { ch: 9, note: 36 })
        );
        assert_eq!(
            parse(&[0x80, 60, 0]),
            Some(MidiMsg::NoteOff { ch: 0, note: 60 })
        );
        assert_eq!(
            parse(&[0xb0, 74, 127]),
            Some(MidiMsg::Cc {
                ch: 0,
                cc: 74,
                value: 127
            })
        );
        assert_eq!(parse(&[0xe0, 0, 64]), None); // pitch bend: ignored
        assert_eq!(parse(&[0x90, 60]), None); // truncated
        assert_eq!(parse(&[]), None);
    }

    /// The commands the controller sent, read back from the engine queue.
    fn played(engine: &mut Engine) -> Vec<(usize, u8, bool)> {
        engine
            .drain_commands_for_test()
            .into_iter()
            .filter_map(|c| match c {
                Command::NoteOn { track, pitch, .. } => Some((track, pitch, true)),
                Command::NoteOff { track, pitch } => Some((track, pitch, false)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn keys_play_the_selected_track_and_pads_play_drums() {
        let (mut engine, mut c) = controller();
        let (drums, melodic) = drums_and_melodic(&c);
        c.set_focus(melodic);
        played(&mut engine);

        c.midi_message(&[0x90, 64, 90]);
        c.midi_message(&[0x99, 37, 90]); // MiniLab pad 2 on channel 10
        assert_eq!(
            played(&mut engine),
            vec![(melodic, 64, true), (drums, 36 + 1, true)]
        );
        // the selection changes while the key is held: the note-off still
        // reaches the track that started it
        c.set_focus(drums);
        c.midi_message(&[0x80, 64, 0]);
        c.midi_message(&[0x99, 37, 0]);
        assert_eq!(
            played(&mut engine),
            vec![(melodic, 64, false), (drums, 37, false)]
        );
        // with a drum track selected the keys play its pads, wrapping at 16
        c.midi_message(&[0x90, 36 + 17, 100]);
        assert_eq!(played(&mut engine), vec![(drums, 37, true)]);
    }

    #[test]
    fn sustain_pedal_holds_melodic_notes() {
        let (mut engine, mut c) = controller();
        let (_, melodic) = drums_and_melodic(&c);
        c.set_focus(melodic);
        c.midi_message(&[0xb0, 64, 127]);
        c.midi_message(&[0x90, 60, 100]);
        c.midi_message(&[0x80, 60, 0]);
        played(&mut engine);
        c.midi_message(&[0xb0, 64, 0]);
        assert_eq!(played(&mut engine), vec![(melodic, 60, false)]);
    }

    #[test]
    fn knobs_move_macros_master_and_feel_with_one_undo_step() {
        let (_engine, mut c) = controller();
        let (_, melodic) = drums_and_melodic(&c);
        c.set_focus(melodic);
        let before = c.project.clone();
        c.midi_message(&[0xb0, 74, 0]); // knob 2 → filter
        c.midi_message(&[0xb0, 74, 127]);
        c.midi_message(&[0xb0, 18, 127]); // knob 10 → vinyl
        c.midi_message(&[0xb0, 91, 0]); // knob 14 → humanize
        let t = &c.project.tracks[melodic];
        assert_eq!(t.macros.get(MacroParam::Cutoff), 1.0);
        assert_eq!(t.humanize, 0.0);
        assert_eq!(c.project.master.vinyl, 1.0);
        c.undo().unwrap();
        assert_eq!(c.project, before);
    }

    #[test]
    fn unknown_messages_and_empty_projects_are_harmless() {
        let (_engine, mut c) = controller();
        c.set_focus(99);
        c.midi_message(&[0xf8]);
        c.midi_message(&[0xb0, 1, 64]);
        c.midi_message(&[0x90, 60, 100]);
        while c.remove_track(0).is_ok() {}
        c.midi_message(&[0x80, 60, 0]);
        c.midi_message(&[0x99, 36, 100]);
    }
}
