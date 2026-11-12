//! Retroactive capture: everything played live is remembered, and pressing
//! "Capture" turns the latest phrase into a quantized, perfectly looping pattern.

use std::collections::VecDeque;

use crate::model::{quantize_tick, Note, Pattern, TICKS_PER_4_4_BAR, TICKS_PER_STEP};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveEvent {
    pub pitch: u8,
    pub velocity: f32,
    pub on: bool,
    pub tick: f64,
    /// Whether the transport was running (ticks are then bar-aligned song time).
    pub playing: bool,
}

/// Rolling memory of what was played on one track.
#[derive(Debug, Default, Clone)]
pub struct CaptureBuffer {
    events: VecDeque<LiveEvent>,
}

/// Silence longer than this separates two phrases.
const PHRASE_GAP_TICKS: f64 = (TICKS_PER_4_4_BAR * 2) as f64;
const MAX_EVENTS: usize = 4096;

impl CaptureBuffer {
    pub fn push(&mut self, ev: LiveEvent) {
        // A change of transport state makes the timeline incomparable: start over.
        if let Some(last) = self.events.back() {
            if last.playing != ev.playing || ev.tick < last.tick - 1.0 {
                self.events.clear();
            }
        }
        if self.events.len() >= MAX_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(ev);
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }

    pub fn is_empty(&self) -> bool {
        !self.events.iter().any(|e| e.on)
    }

    /// Build a loop from the most recent phrase, in bars of `bar_ticks`.
    /// `max_bars` caps the length.
    pub fn capture(&self, max_bars: u32, quantize: bool, bar_ticks: u32) -> Option<Pattern> {
        // pair note-ons with their note-offs
        let mut notes: Vec<(f64, f64, u8, f32)> = Vec::new(); // start, end, pitch, vel
        let evs: Vec<&LiveEvent> = self.events.iter().collect();
        let playing = evs.last()?.playing;
        for (i, e) in evs.iter().enumerate() {
            if !e.on {
                continue;
            }
            let end = evs[i + 1..]
                .iter()
                .find(|x| x.pitch == e.pitch)
                .filter(|x| !x.on)
                .map(|x| x.tick)
                .unwrap_or(e.tick + TICKS_PER_STEP as f64);
            notes.push((e.tick, end.max(e.tick + 1.0), e.pitch, e.velocity));
        }
        if notes.is_empty() {
            return None;
        }
        // keep only the latest phrase
        let mut first = notes.len() - 1;
        while first > 0 && notes[first].0 - notes[first - 1].0 < PHRASE_GAP_TICKS {
            first -= 1;
        }
        let phrase = &notes[first..];
        let bar = bar_ticks as f64;
        let origin = if playing {
            (phrase[0].0 / bar).floor() * bar
        } else {
            // free time: the first note defines the downbeat (snapped to the grid)
            if quantize {
                quantize_tick(phrase[0].0.round() as u32, TICKS_PER_STEP, 1.0) as f64
            } else {
                phrase[0].0
            }
        };
        let last_on = phrase.iter().map(|n| n.0).fold(f64::MIN, f64::max);
        let span = last_on - origin + 1.0;
        let mut bars = 1u32;
        while (bars as f64) * bar < span && bars < max_bars {
            bars *= 2;
        }
        let len = bars * bar_ticks;
        // If the phrase is longer than the max length, keep its most recent part.
        let origin = if span > len as f64 {
            let shift = ((span - len as f64) / bar).ceil() * bar;
            origin + shift
        } else {
            origin
        };

        let mut pattern = Pattern::new("Captura", bars);
        for &(start, end, pitch, vel) in phrase {
            if start < origin {
                continue;
            }
            let rel = start - origin;
            let mut s = rel.round().max(0.0) as u32;
            let mut l = (end - start).round().max(1.0) as u32;
            if quantize {
                s = quantize_tick(s, TICKS_PER_STEP, 1.0);
                l = quantize_tick(l, TICKS_PER_STEP, 1.0).max(TICKS_PER_STEP / 2);
            }
            pattern.add_note(
                Note {
                    start: s % len,
                    length: l.min(len),
                    pitch,
                    velocity: vel,
                    ..Default::default()
                },
                bar_ticks,
            );
        }
        Some(pattern)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(buf: &mut CaptureBuffer, tick: f64, pitch: u8, playing: bool) {
        buf.push(LiveEvent {
            pitch,
            velocity: 1.0,
            on: true,
            tick,
            playing,
        });
        buf.push(LiveEvent {
            pitch,
            velocity: 0.0,
            on: false,
            tick: tick + 20.0,
            playing,
        });
    }

    #[test]
    fn captures_sloppy_one_bar_groove_while_playing() {
        let mut b = CaptureBuffer::default();
        // bar 3 (ticks 768..1152), slightly early/late hits
        for (t, p) in [(770.0, 36), (865.0, 38), (958.0, 36), (1055.0, 38)] {
            hit(&mut b, t, p, true);
        }
        let pat = b.capture(8, true, TICKS_PER_4_4_BAR).unwrap();
        assert_eq!(pat.bars, 1);
        let starts: Vec<u32> = pat.notes.iter().map(|n| n.start).collect();
        assert_eq!(starts, vec![0, 96, 192, 288]);
    }

    #[test]
    fn only_latest_phrase_and_length_power_of_two() {
        let mut b = CaptureBuffer::default();
        hit(&mut b, 0.0, 60, false);
        // long silence, then a 3-bar phrase -> 4 bar loop
        let base = 5000.0;
        for i in 0..12 {
            hit(&mut b, base + i as f64 * 96.0, 60 + i as u8, false);
        }
        let pat = b.capture(8, true, TICKS_PER_4_4_BAR).unwrap();
        assert_eq!(pat.bars, 4);
        assert_eq!(pat.notes.len(), 12);
        assert_eq!(pat.notes[0].start, 0);
        assert_eq!(pat.notes[0].pitch, 60);
    }

    #[test]
    fn transport_change_resets() {
        let mut b = CaptureBuffer::default();
        hit(&mut b, 100.0, 60, false);
        hit(&mut b, 10.0, 62, true);
        let pat = b.capture(8, true, TICKS_PER_4_4_BAR).unwrap();
        assert_eq!(pat.notes.len(), 1);
        assert_eq!(pat.notes[0].pitch, 62);
    }
}
