//! Music theory helpers: scales, scale locking, chords and hot scale changes.

use serde::{Deserialize, Serialize};

pub const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScaleKind {
    Major,
    Minor,
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Locrian,
    HarmonicMinor,
    MelodicMinor,
    MajorPentatonic,
    MinorPentatonic,
    Blues,
    Chromatic,
}

impl ScaleKind {
    pub const ALL: [ScaleKind; 13] = [
        ScaleKind::Major,
        ScaleKind::Minor,
        ScaleKind::Dorian,
        ScaleKind::Phrygian,
        ScaleKind::Lydian,
        ScaleKind::Mixolydian,
        ScaleKind::Locrian,
        ScaleKind::HarmonicMinor,
        ScaleKind::MelodicMinor,
        ScaleKind::MajorPentatonic,
        ScaleKind::MinorPentatonic,
        ScaleKind::Blues,
        ScaleKind::Chromatic,
    ];

    /// Semitone offsets from the root.
    pub fn intervals(self) -> &'static [u8] {
        match self {
            ScaleKind::Major => &[0, 2, 4, 5, 7, 9, 11],
            ScaleKind::Minor => &[0, 2, 3, 5, 7, 8, 10],
            ScaleKind::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            ScaleKind::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            ScaleKind::Lydian => &[0, 2, 4, 6, 7, 9, 11],
            ScaleKind::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            ScaleKind::Locrian => &[0, 1, 3, 5, 6, 8, 10],
            ScaleKind::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
            ScaleKind::MelodicMinor => &[0, 2, 3, 5, 7, 9, 11],
            ScaleKind::MajorPentatonic => &[0, 2, 4, 7, 9],
            ScaleKind::MinorPentatonic => &[0, 3, 5, 7, 10],
            ScaleKind::Blues => &[0, 3, 5, 6, 7, 10],
            ScaleKind::Chromatic => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        }
    }

    /// Human readable name for the UI.
    pub fn label(self) -> &'static str {
        match self {
            ScaleKind::Major => "Major",
            ScaleKind::Minor => "Minor",
            ScaleKind::Dorian => "Dorian",
            ScaleKind::Phrygian => "Phrygian",
            ScaleKind::Lydian => "Lydian",
            ScaleKind::Mixolydian => "Mixolydian",
            ScaleKind::Locrian => "Locrian",
            ScaleKind::HarmonicMinor => "Harmonic minor",
            ScaleKind::MelodicMinor => "Melodic minor",
            ScaleKind::MajorPentatonic => "Major pentatonic",
            ScaleKind::MinorPentatonic => "Minor pentatonic",
            ScaleKind::Blues => "Blues",
            ScaleKind::Chromatic => "Chromatic",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scale {
    /// Pitch class of the root, 0 = C .. 11 = B.
    pub root: u8,
    pub kind: ScaleKind,
}

impl Default for Scale {
    fn default() -> Self {
        // New projects start in C major; saved projects retain their scale.
        Scale {
            root: 0,
            kind: ScaleKind::Major,
        }
    }
}

impl Scale {
    pub fn new(root: u8, kind: ScaleKind) -> Self {
        Scale {
            root: root % 12,
            kind,
        }
    }

    pub fn len(&self) -> usize {
        self.kind.intervals().len()
    }

    pub fn contains(&self, pitch: u8) -> bool {
        let pc = (pitch as i32 - self.root as i32).rem_euclid(12) as u8;
        self.kind.intervals().contains(&pc)
    }

    /// MIDI pitch of a (possibly negative or > len) scale degree, counted from
    /// the root in `base_octave` (MIDI octave, C4 = 60 -> octave 4).
    pub fn degree_to_pitch(&self, degree: i32, base_octave: i32) -> u8 {
        let n = self.len() as i32;
        let oct = degree.div_euclid(n);
        let idx = degree.rem_euclid(n) as usize;
        let pitch =
            (base_octave + 1 + oct) * 12 + self.root as i32 + self.kind.intervals()[idx] as i32;
        pitch.clamp(0, 127) as u8
    }

    /// Absolute degree index of `pitch` relative to C-1 root octave, snapping
    /// down to the nearest scale tone when the pitch is out of scale.
    /// Returns (degree, cents_off) where cents_off is the chromatic remainder.
    pub fn pitch_to_degree(&self, pitch: u8) -> (i32, i32) {
        let n = self.len() as i32;
        let rel = pitch as i32 - self.root as i32;
        let oct = rel.div_euclid(12);
        let pc = rel.rem_euclid(12) as u8;
        let ints = self.kind.intervals();
        let mut idx = 0usize;
        for (i, &iv) in ints.iter().enumerate() {
            if iv <= pc {
                idx = i;
            }
        }
        let remainder = pc as i32 - ints[idx] as i32;
        (oct * n + idx as i32, remainder)
    }

    /// Snap a pitch to the nearest note of the scale (ties resolve downward).
    pub fn snap(&self, pitch: u8) -> u8 {
        if self.contains(pitch) {
            return pitch;
        }
        for d in 1..12i32 {
            let down = pitch as i32 - d;
            if down >= 0 && self.contains(down as u8) {
                return down as u8;
            }
            let up = pitch as i32 + d;
            if up <= 127 && self.contains(up as u8) {
                return up as u8;
            }
        }
        pitch
    }

    /// Build a diatonic chord (stacked thirds within the scale) on `degree`.
    pub fn chord(&self, degree: i32, base_octave: i32, kind: ChordKind) -> Vec<u8> {
        let steps: &[i32] = match kind {
            ChordKind::Triad => &[0, 2, 4],
            ChordKind::Seventh => &[0, 2, 4, 6],
            ChordKind::Sus2 => &[0, 1, 4],
            ChordKind::Sus4 => &[0, 3, 4],
            ChordKind::Power => &[0, 4, 7],
        };
        if self.len() != 7 {
            // Pentatonic/blues/chromatic do not stack thirds well: use major/minor
            // triads guessed from the closest heptatonic parent.
            let parent = Scale::new(
                self.root,
                if self.kind.intervals().contains(&3) {
                    ScaleKind::Minor
                } else {
                    ScaleKind::Major
                },
            );
            let root_pitch = self.degree_to_pitch(degree, base_octave);
            let (pd, _) = parent.pitch_to_degree(root_pitch);
            return steps
                .iter()
                .map(|s| parent.degree_to_pitch(pd + s, -1))
                .collect();
        }
        steps
            .iter()
            .map(|s| self.degree_to_pitch(degree + s, base_octave))
            .collect()
    }

    /// Remap a pitch from `from` scale to this scale, keeping its scale degree
    /// (and its chromatic offset for out-of-scale notes, then snapping).
    pub fn remap_from(&self, from: &Scale, pitch: u8) -> u8 {
        let (deg, rem) = from.pitch_to_degree(pitch);
        // Work out degree per octave in the target scale so a 7-note line stays
        // a 7-note line even when moving to a pentatonic.
        let from_n = from.len() as i32;
        let to_n = self.len() as i32;
        let oct = deg.div_euclid(from_n);
        let idx = deg.rem_euclid(from_n);
        let mapped_idx = if from_n == to_n {
            idx
        } else {
            (idx * to_n) / from_n
        };
        let new_deg = oct * to_n + mapped_idx;
        // degree_to_pitch with base octave -1 means degree 0 is root at MIDI octave -1
        let p = self.degree_to_pitch(new_deg, -1) as i32 + rem;
        self.snap(p.clamp(0, 127) as u8)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChordKind {
    #[default]
    Triad,
    Seventh,
    Sus2,
    Sus4,
    Power,
}

pub fn note_name(pitch: u8) -> String {
    format!(
        "{}{}",
        NOTE_NAMES[(pitch % 12) as usize],
        pitch as i32 / 12 - 1
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_minor_contains() {
        let s = Scale::new(9, ScaleKind::Minor);
        for p in [57u8, 59, 60, 62, 64, 65, 67, 69] {
            assert!(s.contains(p), "{}", note_name(p));
        }
        assert!(!s.contains(61));
        assert!(!s.contains(66));
    }

    #[test]
    fn degree_roundtrip() {
        let s = Scale::new(2, ScaleKind::Dorian);
        for d in 0..60 {
            let p = s.degree_to_pitch(d, -1);
            assert_eq!(s.pitch_to_degree(p), (d, 0));
        }
    }

    #[test]
    fn c_major_triads() {
        let s = Scale::new(0, ScaleKind::Major);
        assert_eq!(s.chord(0, 4, ChordKind::Triad), vec![60, 64, 67]);
        assert_eq!(s.chord(1, 4, ChordKind::Triad), vec![62, 65, 69]); // Dm
        assert_eq!(s.chord(4, 4, ChordKind::Seventh), vec![67, 71, 74, 77]); // G7
    }

    #[test]
    fn pentatonic_chords_are_in_parent() {
        let s = Scale::new(9, ScaleKind::MinorPentatonic);
        let c = s.chord(0, 3, ChordKind::Triad);
        assert_eq!(c, vec![57, 60, 64]); // A minor
    }

    #[test]
    fn remap_major_to_minor_keeps_degrees() {
        let c_major = Scale::new(0, ScaleKind::Major);
        let c_minor = Scale::new(0, ScaleKind::Minor);
        assert_eq!(c_minor.remap_from(&c_major, 64), 63); // E -> Eb
        assert_eq!(c_minor.remap_from(&c_major, 60), 60);
        assert_eq!(c_minor.remap_from(&c_major, 71), 70); // B -> Bb
        let d_dorian = Scale::new(2, ScaleKind::Dorian);
        assert_eq!(d_dorian.remap_from(&c_major, 60), 62); // C (deg 0) -> D
    }

    #[test]
    fn snap_works() {
        let s = Scale::new(0, ScaleKind::Major);
        assert_eq!(s.snap(61), 60);
        assert_eq!(s.snap(66), 65);
        assert_eq!(s.snap(62), 62);
    }
}
