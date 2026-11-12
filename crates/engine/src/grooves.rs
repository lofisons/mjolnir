//! Ready-made drum loops: hi-hat lines (trap rolls, lazy lo-fi eighths…) and
//! whole beats. They use the standard drum map (kick, snare, clap, closed
//! and open hat…), so they play on any kit.

use serde::{Deserialize, Serialize};

use crate::model::{Note, Pattern, DRUM_BASE_NOTE, TICKS_PER_STEP};

const KICK: u8 = DRUM_BASE_NOTE;
const SNARE: u8 = DRUM_BASE_NOTE + 1;
const CLAP: u8 = DRUM_BASE_NOTE + 2;
const HAT: u8 = DRUM_BASE_NOTE + 3;
const OPEN_HAT: u8 = DRUM_BASE_NOTE + 4;
const RIM: u8 = DRUM_BASE_NOTE + 8;
/// Pads a hat loop owns: loading one replaces only these, the rest stays.
const HAT_PADS: [u8; 2] = [HAT, OPEN_HAT];

const STEP: u32 = TICKS_PER_STEP;
/// 1/32 note: half a step.
const THIRTY_SECOND: u32 = STEP / 2;
/// 1/16 triplet: six hits in one beat.
const SIXTEENTH_TRIPLET: u32 = STEP * 4 / 6;
/// 1/8 triplet: three hits in one beat.
const EIGHTH_TRIPLET: u32 = STEP * 4 / 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrooveKind {
    /// Only hi-hats: layered over whatever the loop already has.
    Hats,
    /// A whole beat: replaces the loop's notes.
    Drums,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrooveId {
    HatsEighths,
    HatsSixteenths,
    HatsLazy,
    HatsTrapRolls,
    HatsTriplets,
    BoomBap,
    LofiLazy,
    TrapHalfTime,
    TrapBounce,
}

impl GrooveId {
    pub const ALL: [GrooveId; 9] = [
        GrooveId::HatsEighths,
        GrooveId::HatsSixteenths,
        GrooveId::HatsLazy,
        GrooveId::HatsTrapRolls,
        GrooveId::HatsTriplets,
        GrooveId::BoomBap,
        GrooveId::LofiLazy,
        GrooveId::TrapHalfTime,
        GrooveId::TrapBounce,
    ];

    pub fn label(self) -> &'static str {
        match self {
            GrooveId::HatsEighths => "Hats: 8ths",
            GrooveId::HatsSixteenths => "Hats: 16ths",
            GrooveId::HatsLazy => "Hats: lazy lo-fi",
            GrooveId::HatsTrapRolls => "Hats: trap rolls",
            GrooveId::HatsTriplets => "Hats: triplets",
            GrooveId::BoomBap => "Boom bap",
            GrooveId::LofiLazy => "Lo-fi lazy",
            GrooveId::TrapHalfTime => "Trap half-time",
            GrooveId::TrapBounce => "Trap bounce",
        }
    }

    pub fn kind(self) -> GrooveKind {
        match self {
            GrooveId::HatsEighths
            | GrooveId::HatsSixteenths
            | GrooveId::HatsLazy
            | GrooveId::HatsTrapRolls
            | GrooveId::HatsTriplets => GrooveKind::Hats,
            _ => GrooveKind::Drums,
        }
    }

    /// The loop on its own, at its natural length.
    pub fn pattern(self) -> Pattern {
        let mut b = Beat::default();
        match self {
            GrooveId::HatsEighths => eighths(&mut b, 0, 0.6, 0.4),
            GrooveId::HatsSixteenths => {
                for s in 0..16 {
                    b.hit(s * STEP, HAT, [0.6, 0.3, 0.45, 0.3][s as usize % 4]);
                }
            }
            GrooveId::HatsLazy => lazy_hats(&mut b, 0),
            GrooveId::HatsTrapRolls => {
                // bar 1: sixteenths with a 1/32 roll into beat 4
                for s in 0..16 {
                    if s == 11 {
                        b.roll(s * STEP, THIRTY_SECOND, 4, HAT, 0.3, 0.6);
                    } else if !(12..14).contains(&s) {
                        b.hit(s * STEP, HAT, if s % 2 == 0 { 0.55 } else { 0.3 });
                    }
                }
                // bar 2: eighths, an open hat, then a triplet roll to the top
                for s in 16..28 {
                    if s % 2 == 0 && s != 22 {
                        b.hit(s * STEP, HAT, if s % 4 == 0 { 0.6 } else { 0.4 });
                    }
                }
                b.hit(22 * STEP, OPEN_HAT, 0.45);
                b.roll(28 * STEP, SIXTEENTH_TRIPLET, 6, HAT, 0.25, 0.65);
            }
            GrooveId::HatsTriplets => {
                for k in 0..12 {
                    b.hit(k * EIGHTH_TRIPLET, HAT, if k % 3 == 0 { 0.6 } else { 0.32 });
                }
            }
            GrooveId::BoomBap => {
                for (s, v) in [(0, 0.95), (7, 0.6), (10, 0.85)] {
                    b.hit(s * STEP, KICK, v);
                }
                for s in [4, 12] {
                    b.hit(s * STEP, SNARE, 0.9);
                }
                eighths(&mut b, 0, 0.55, 0.35);
            }
            GrooveId::LofiLazy => {
                for (s, v) in [
                    (0, 0.9),
                    (3, 0.5),
                    (10, 0.8),
                    (16, 0.9),
                    (26, 0.8),
                    (29, 0.45),
                ] {
                    b.hit(s * STEP, KICK, v);
                }
                for s in [4, 12, 20, 28] {
                    b.hit(s * STEP, SNARE, 0.8);
                }
                for (s, v) in [(15, 0.2), (31, 0.25)] {
                    b.hit(s * STEP, SNARE, v);
                }
                b.hit(6 * STEP, RIM, 0.35);
                lazy_hats(&mut b, 0);
                lazy_hats(&mut b, 16);
            }
            GrooveId::TrapHalfTime => {
                for (s, v) in [(0, 1.0), (10, 0.85), (11, 0.6)] {
                    b.hit(s * STEP, KICK, v);
                }
                b.hit(8 * STEP, SNARE, 0.85);
                b.hit(8 * STEP, CLAP, 0.8);
                for s in 0..14 {
                    b.hit(s * STEP, HAT, if s % 2 == 0 { 0.55 } else { 0.3 });
                }
                b.roll(14 * STEP, THIRTY_SECOND, 4, HAT, 0.3, 0.6);
            }
            GrooveId::TrapBounce => {
                for (s, v) in [
                    (0, 1.0),
                    (7, 0.7),
                    (10, 0.85),
                    (16, 1.0),
                    (19, 0.7),
                    (26, 0.85),
                    (27, 0.6),
                ] {
                    b.hit(s * STEP, KICK, v);
                }
                for s in [8, 24] {
                    b.hit(s * STEP, CLAP, 0.85);
                    b.hit(s * STEP, SNARE, 0.6);
                }
                // a snare pickup in 1/32s into the next bar
                b.roll(30 * STEP, THIRTY_SECOND, 4, SNARE, 0.3, 0.7);
                for s in (0..16).step_by(2) {
                    b.hit(s * STEP, HAT, if s % 4 == 0 { 0.6 } else { 0.4 });
                }
                b.roll(6 * STEP, THIRTY_SECOND, 4, HAT, 0.3, 0.5);
                b.roll(12 * STEP, SIXTEENTH_TRIPLET, 6, HAT, 0.3, 0.55);
                for s in (16..32).step_by(2) {
                    b.hit(s * STEP, HAT, if s % 4 == 0 { 0.6 } else { 0.4 });
                }
                b.hit(30 * STEP, OPEN_HAT, 0.4);
            }
        }
        b.into_pattern(self.label())
    }

    /// Load this groove into a loop. A beat replaces `existing` (keeping its
    /// automation); a hat line only replaces the hats and keeps the rest.
    /// The result is as long as the longer of the two; the shorter repeats.
    /// Grooves are written in 4/4; in another meter each bar keeps the steps
    /// that fit (a 3/4 bar drops the fourth beat, a 7/8 bar its last eighth).
    pub fn apply(self, existing: Option<&Pattern>, bar: u32) -> Pattern {
        let groove = refit(self.pattern(), bar);
        let Some(old) = existing else {
            return groove;
        };
        let bars = old.bars.max(groove.bars);
        let mut fill = groove;
        fill.set_bars(bars, bar);
        let mut out = old.clone();
        out.set_bars(bars, bar);
        match self.kind() {
            GrooveKind::Drums => {
                out.name = fill.name;
                out.notes = fill.notes;
            }
            GrooveKind::Hats => {
                out.notes.retain(|n| !HAT_PADS.contains(&n.pitch));
                out.notes.extend(fill.notes);
                out.sort();
            }
        }
        out
    }
}

/// Move a 4/4 pattern onto bars of `bar` ticks, bar by bar.
fn refit(p: Pattern, bar: u32) -> Pattern {
    const FOUR: u32 = crate::model::TICKS_PER_4_4_BAR;
    if bar == FOUR {
        return p;
    }
    let mut out = Pattern::new(p.name.clone(), p.bars);
    for n in p.notes {
        let (b, at) = (n.start / FOUR, n.start % FOUR);
        if at < bar {
            out.add_note(
                Note {
                    start: b * bar + at,
                    length: n.length.min(bar - at),
                    ..n
                },
                bar,
            );
        }
    }
    out
}

/// Plain eighth notes from `from` (a step) for one bar.
fn eighths(b: &mut Beat, from: u32, on_beat: f32, off_beat: f32) {
    for s in (0..16).step_by(2) {
        b.hit(
            (from + s) * STEP,
            HAT,
            if s % 4 == 0 { on_beat } else { off_beat },
        );
    }
}

/// One bar of soft eighths with ghost sixteenths and an open hat at the end.
fn lazy_hats(b: &mut Beat, from: u32) {
    for s in (0..14).step_by(2) {
        b.hit((from + s) * STEP, HAT, if s % 4 == 0 { 0.45 } else { 0.3 });
    }
    for s in [7, 11] {
        b.hit((from + s) * STEP, HAT, 0.18);
    }
    b.hit((from + 14) * STEP, OPEN_HAT, 0.35);
}

#[derive(Default)]
struct Beat {
    notes: Vec<Note>,
}

impl Beat {
    fn hit(&mut self, start: u32, pitch: u8, velocity: f32) {
        self.notes.push(Note {
            start,
            length: STEP,
            pitch,
            velocity,
        });
    }

    /// `count` quick hits `every` ticks apart, swelling from `from` to `to`.
    fn roll(&mut self, start: u32, every: u32, count: u32, pitch: u8, from: f32, to: f32) {
        for k in 0..count {
            let t = k as f32 / (count - 1).max(1) as f32;
            self.notes.push(Note {
                start: start + k * every,
                length: every,
                pitch,
                velocity: from + (to - from) * t,
            });
        }
    }

    fn into_pattern(self, name: &str) -> Pattern {
        let last = self.notes.iter().map(|n| n.start).max().unwrap_or(0);
        const BAR: u32 = crate::model::TICKS_PER_4_4_BAR;
        let mut p = Pattern::new(name, last / BAR + 1);
        for n in self.notes {
            p.add_note(n, BAR);
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TICKS_PER_4_4_BAR as TICKS_PER_BAR;

    #[test]
    fn every_groove_has_notes_inside_its_loop() {
        for g in GrooveId::ALL {
            let p = g.pattern();
            assert!(!p.notes.is_empty(), "{g:?} is empty");
            assert!(
                p.notes
                    .iter()
                    .all(|n| n.start < p.length_ticks(TICKS_PER_BAR)),
                "{g:?}"
            );
            if g.kind() == GrooveKind::Hats {
                assert!(p.notes.iter().all(|n| HAT_PADS.contains(&n.pitch)), "{g:?}");
            }
        }
    }

    #[test]
    fn hat_rolls_are_finer_than_the_grid() {
        let p = GrooveId::HatsTrapRolls.pattern();
        assert_eq!(p.bars, 2);
        assert!(p.notes.iter().any(|n| n.start % STEP != 0));
    }

    #[test]
    fn hats_layer_over_a_beat_and_beats_replace_it() {
        let mut beat = Pattern::new("Mine", 2);
        beat.add_note(
            Note {
                start: 0,
                length: STEP,
                pitch: KICK,
                velocity: 1.0,
            },
            TICKS_PER_BAR,
        );
        beat.add_note(
            Note {
                start: STEP,
                length: STEP,
                pitch: HAT,
                velocity: 1.0,
            },
            TICKS_PER_BAR,
        );
        let hats = GrooveId::HatsEighths.apply(Some(&beat), TICKS_PER_BAR);
        assert_eq!(hats.name, "Mine");
        assert_eq!(hats.bars, 2);
        // the kick stays, the old off-grid hat is gone, the eighths fill both bars
        assert!(hats.notes.iter().any(|n| n.pitch == KICK));
        assert!(!hats.notes.iter().any(|n| n.pitch == HAT && n.start == STEP));
        assert_eq!(hats.notes.iter().filter(|n| n.pitch == HAT).count(), 16);
        assert!(hats.notes.iter().any(|n| n.start >= TICKS_PER_BAR));

        let drums = GrooveId::BoomBap.apply(Some(&hats), TICKS_PER_BAR);
        assert_eq!(drums.name, "Boom bap");
        assert_eq!(drums.bars, 2);
        assert_eq!(
            drums.notes.len(),
            2 * GrooveId::BoomBap.pattern().notes.len()
        );
    }
}

#[cfg(test)]
mod meter_tests {
    use super::*;
    use crate::model::Meter;

    #[test]
    fn grooves_fit_other_meters() {
        let waltz = Meter { beats: 3, unit: 4 }.bar_ticks();
        let p = GrooveId::BoomBap.apply(None, waltz);
        assert!(!p.notes.is_empty());
        assert!(p.notes.iter().all(|n| n.start < p.length_ticks(waltz)));
        // nothing from the dropped fourth beat moved into the next bar's start
        assert!(p.notes.iter().all(|n| n.start % waltz + n.length <= waltz));
    }
}
