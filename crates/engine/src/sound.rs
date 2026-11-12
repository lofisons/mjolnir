//! Designable sounds: the parameters of a synth patch and of a drum pad,
//! with their ranges, so the interface can draw an editor for each (like a
//! plugin's panel) and a designed sound can be saved to a `.yggsound` file.
//!
//! Every parameter is an `f32`; switches are 0/1 and choices are whole
//! numbers. One table per sound lists them all: the editor, the clamping and
//! the file format all follow it.

use serde::{Deserialize, Serialize};

/// How a parameter is shown and how its knob maps to the value.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ParamSpec {
    pub key: &'static str,
    pub label: &'static str,
    /// Section of the editor ("Oscillators", "Filter"…).
    pub group: &'static str,
    pub min: f32,
    pub max: f32,
    /// The knob moves on a logarithmic curve (frequencies, times).
    pub log: bool,
    /// 0 = continuous; 1 = whole numbers (a switch when the range is 0..1).
    pub step: f32,
    /// Shown after the value: "Hz", "s", "ct", "st", "×"…
    pub unit: &'static str,
    /// Names of the values of a choice (e.g. filter types).
    pub choices: &'static [&'static str],
    pub help: &'static str,
}

impl ParamSpec {
    pub fn clamp(&self, v: f32) -> f32 {
        let v = if v.is_finite() { v } else { self.min };
        let v = v.clamp(self.min, self.max);
        if self.step > 0.0 {
            (v / self.step).round() * self.step
        } else {
            v
        }
    }
}

const fn p(
    key: &'static str,
    label: &'static str,
    group: &'static str,
    min: f32,
    max: f32,
    log: bool,
    unit: &'static str,
    help: &'static str,
) -> ParamSpec {
    ParamSpec {
        key,
        label,
        group,
        min,
        max,
        log,
        step: 0.0,
        unit,
        choices: &[],
        help,
    }
}

const fn whole(mut s: ParamSpec) -> ParamSpec {
    s.step = 1.0;
    s
}

const fn choice(mut s: ParamSpec, choices: &'static [&'static str]) -> ParamSpec {
    s.step = 1.0;
    s.choices = choices;
    s
}

/// Defines a sound struct of `f32` fields, its parameter table and
/// by-name access, from one list.
macro_rules! sound {
    ($(#[$m:meta])* $name:ident, $table:ident { $($field:ident: $spec:expr,)* }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
        #[serde(default)]
        pub struct $name {
            $(pub $field: f32,)*
        }

        pub const $table: &[ParamSpec] = &[$($spec,)*];

        impl $name {
            /// The value of a parameter by its key.
            pub fn get(&self, key: &str) -> Option<f32> {
                match key {
                    $(stringify!($field) => Some(self.$field),)*
                    _ => None,
                }
            }

            /// Set a parameter by its key, clamped to its range.
            pub fn set(&mut self, key: &str, value: f32) -> Result<(), String> {
                let spec = $table
                    .iter()
                    .find(|s| s.key == key)
                    .ok_or_else(|| format!("unknown sound parameter {key}"))?;
                let v = spec.clamp(value);
                match key {
                    $(stringify!($field) => self.$field = v,)*
                    _ => unreachable!(),
                }
                Ok(())
            }

            /// Bring every parameter into its range (after loading a file).
            pub fn clamped(mut self) -> Self {
                $(
                    let spec = $table.iter().find(|s| s.key == stringify!($field)).unwrap();
                    self.$field = spec.clamp(self.$field);
                )*
                self
            }
        }
    };
}

sound!(
    /// A subtractive / FM synth voice: oscillators into a resonant low-pass
    /// (or a low-pass gate), with envelopes, drift, vibrato, glide and drive.
    SynthPatch, SYNTH_PARAMS {
        sine: p("sine", "Sine / FM", "Oscillators", 0.0, 1.5, false, "", "level of the sine carrier (phase-modulated when FM depth is up)"),
        tri: p("tri", "Triangle", "Oscillators", 0.0, 1.5, false, "", "level of the triangle: soft and hollow"),
        saw: p("saw", "Saw", "Oscillators", 0.0, 1.5, false, "", "level of two saws, detuned against each other: bright and buzzy"),
        square: p("square", "Square", "Oscillators", 0.0, 1.5, false, "", "level of the square: woody, reedy"),
        sub: p("sub", "Sub", "Oscillators", 0.0, 1.5, false, "", "level of a sine an octave below: weight"),
        noise: p("noise", "Noise", "Oscillators", 0.0, 1.0, false, "", "level of white noise: breath and air"),
        detune_cents: p("detune_cents", "Detune", "Oscillators", 0.0, 50.0, false, "ct", "how far apart the two saws (and the square) are: chorus and width"),
        fm_ratio: p("fm_ratio", "Ratio", "FM", 0.25, 12.0, true, "×", "frequency of the modulator against the note: whole numbers sound harmonic, others bell-like"),
        fm_index: p("fm_index", "Depth", "FM", 0.0, 10.0, false, "", "how hard the modulator bends the sine: brightness and bark at the attack"),
        fm_decay: p("fm_decay", "Decay", "FM", 0.005, 4.0, true, "s", "how fast the FM depth falls to its sustain"),
        fm_sustain: p("fm_sustain", "Sustain", "FM", 0.0, 10.0, false, "", "FM depth that stays while the note is held"),
        attack: p("attack", "Attack", "Amp envelope", 0.001, 5.0, true, "s", "time to reach full level"),
        decay: p("decay", "Decay", "Amp envelope", 0.01, 8.0, true, "s", "time to fall to the sustain level"),
        sustain: p("sustain", "Sustain", "Amp envelope", 0.0, 1.0, false, "", "level while the note is held"),
        release: p("release", "Release", "Amp envelope", 0.01, 8.0, true, "s", "fade after the note is let go"),
        cutoff: p("cutoff", "Cutoff", "Filter", 40.0, 16000.0, true, "Hz", "base frequency of the low-pass filter"),
        q: p("q", "Resonance", "Filter", 0.5, 12.0, true, "", "peak at the cutoff: from soft to whistling"),
        env_amount: p("env_amount", "Env amount", "Filter", 0.0, 10000.0, false, "Hz", "how far the filter envelope opens the cutoff (velocity scales it)"),
        filter_decay: p("filter_decay", "Env decay", "Filter", 0.005, 5.0, true, "s", "how fast the filter closes after the attack"),
        key_track: p("key_track", "Key track", "Filter", 0.0, 1.0, false, "", "how much the cutoff follows the note: high notes stay as bright as low ones"),
        lpg: whole(p("lpg", "Low-pass gate", "Filter", 0.0, 1.0, false, "", "the filter opens and closes with the loudness, like a vactrol: plucks and bongos")),
        drift_cents: p("drift_cents", "Drift", "Pitch", 0.0, 30.0, false, "ct", "slow random wandering of each voice: analog and tape feel"),
        vibrato_rate: p("vibrato_rate", "Vibrato rate", "Pitch", 0.0, 12.0, false, "Hz", "speed of the shared vibrato (low values: tape wow)"),
        vibrato_cents: p("vibrato_cents", "Vibrato depth", "Pitch", 0.0, 100.0, false, "ct", "depth of the vibrato"),
        glide: p("glide", "Glide", "Pitch", 0.0, 1.0, false, "s", "slide time between notes (mono, or a note that is already sounding)"),
        pitch_drop: p("pitch_drop", "Pitch drop", "Pitch", 0.0, 36.0, false, "st", "the note starts this many semitones higher and falls into place (808 punch)"),
        pitch_drop_decay: p("pitch_drop_decay", "Drop time", "Pitch", 0.005, 1.0, true, "s", "how fast the pitch drop lands"),
        drive: p("drive", "Drive", "Output", 0.0, 1.0, false, "", "saturation after the filter: a sine grows audible harmonics"),
        gain: p("gain", "Level", "Output", 0.0, 1.5, false, "", "loudness of the voice before the track's knobs"),
        mono: whole(p("mono", "Mono", "Output", 0.0, 1.0, false, "", "one note at a time; overlapping notes slide (with Glide)")),
    }
);

impl Default for SynthPatch {
    fn default() -> Self {
        SynthPatch {
            sine: 0.0,
            tri: 0.0,
            saw: 0.0,
            square: 0.0,
            sub: 0.0,
            noise: 0.0,
            detune_cents: 0.0,
            fm_ratio: 1.0,
            fm_index: 0.0,
            fm_decay: 0.3,
            fm_sustain: 0.0,
            attack: 0.005,
            decay: 0.3,
            sustain: 0.6,
            release: 0.3,
            cutoff: 1200.0,
            q: 0.7,
            env_amount: 1500.0,
            filter_decay: 0.3,
            key_track: 0.3,
            lpg: 0.0,
            drift_cents: 4.0,
            vibrato_rate: 0.0,
            vibrato_cents: 0.0,
            glide: 0.0,
            pitch_drop: 0.0,
            pitch_drop_decay: 0.03,
            drive: 0.0,
            gain: 0.25,
            mono: 0.0,
        }
    }
}

const NOISE_MODES: &[&str] = &["Low-pass", "Band-pass", "High-pass"];

sound!(
    /// One synthesized drum pad: a sine body that sweeps down, filtered
    /// noise, a metallic cluster, a click, and some lo-fi grit.
    DrumPad, DRUM_PARAMS {
        tone_level: p("tone_level", "Body", "Body", 0.0, 1.5, false, "", "level of the sine body (kicks, toms, 808s)"),
        tone_start: p("tone_start", "Start pitch", "Body", 20.0, 2000.0, true, "Hz", "pitch at the hit"),
        tone_end: p("tone_end", "End pitch", "Body", 20.0, 2000.0, true, "Hz", "pitch it sweeps down to"),
        pitch_decay: p("pitch_decay", "Sweep time", "Body", 0.0, 0.5, false, "s", "how fast the pitch falls (0 = no sweep)"),
        tone_decay: p("tone_decay", "Body decay", "Body", 0.005, 4.0, true, "s", "how long the body rings"),
        noise_level: p("noise_level", "Noise", "Noise", 0.0, 1.5, false, "", "level of the filtered noise (snares, hats, claps)"),
        noise_decay: p("noise_decay", "Noise decay", "Noise", 0.005, 2.0, true, "s", "how long the noise lasts"),
        noise_mode: choice(p("noise_mode", "Filter", "Noise", 0.0, 2.0, false, "", "colour of the noise filter"), NOISE_MODES),
        noise_freq: p("noise_freq", "Noise tone", "Noise", 40.0, 16000.0, true, "Hz", "frequency of the noise filter"),
        noise_q: p("noise_q", "Noise Q", "Noise", 0.3, 12.0, true, "", "how narrow the noise filter is"),
        bursts: whole(p("bursts", "Bursts", "Noise", 1.0, 8.0, false, "", "the noise retriggers this many times, like hands clapping")),
        metal_level: p("metal_level", "Metal", "Metal", 0.0, 1.5, false, "", "level of six detuned squares: cymbals, cowbells"),
        metal_decay: p("metal_decay", "Metal decay", "Metal", 0.005, 3.0, true, "s", "how long the metal rings"),
        metal_freq: p("metal_freq", "Metal tone", "Metal", 100.0, 2000.0, true, "Hz", "base frequency of the metal cluster"),
        click: p("click", "Click", "Character", 0.0, 1.5, false, "", "a short tick at the hit: attack and beater"),
        drive: p("drive", "Drive", "Character", 0.0, 1.0, false, "", "saturation: punch and grit"),
        bits: whole(p("bits", "Bits", "Character", 0.0, 16.0, false, "", "bit depth (0 = off): lower is crunchier")),
        crush: whole(p("crush", "Downsample", "Character", 0.0, 16.0, false, "×", "holds each sample this many times (0/1 = off): old-sampler aliasing")),
        choke: whole(p("choke", "Choke group", "Character", 0.0, 4.0, false, "", "a hit silences the other pads of the same group (0 = none): open and closed hats")),
        gain: p("gain", "Level", "Character", 0.0, 2.0, false, "", "loudness of this pad"),
    }
);

impl Default for DrumPad {
    fn default() -> Self {
        DrumPad {
            tone_level: 0.0,
            tone_start: 200.0,
            tone_end: 60.0,
            pitch_decay: 0.0,
            tone_decay: 0.2,
            noise_level: 0.0,
            noise_decay: 0.1,
            noise_mode: 0.0,
            noise_freq: 3000.0,
            noise_q: 0.7,
            bursts: 1.0,
            metal_level: 0.0,
            metal_decay: 0.2,
            metal_freq: 400.0,
            click: 0.0,
            drive: 0.0,
            bits: 0.0,
            crush: 0.0,
            choke: 0.0,
            gain: 1.0,
        }
    }
}

/// What a `.yggsound` file holds: one designed instrument.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundFile {
    /// Always "yggsound", so other JSON is not mistaken for a sound.
    pub format: String,
    pub version: u32,
    pub sound: crate::model::InstrumentSpec,
}

pub const SOUND_FILE_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_match_the_structs() {
        let s = SynthPatch::default();
        for spec in SYNTH_PARAMS {
            let v = s.get(spec.key).unwrap();
            assert!(spec.min <= spec.max, "{}", spec.key);
            assert!(v >= spec.min && v <= spec.max, "{} default {v}", spec.key);
        }
        let d = DrumPad::default();
        for spec in DRUM_PARAMS {
            let v = d.get(spec.key).unwrap();
            assert!(v >= spec.min && v <= spec.max, "{} default {v}", spec.key);
        }
        assert_eq!(
            serde_json::to_value(s).unwrap().as_object().unwrap().len(),
            SYNTH_PARAMS.len()
        );
        assert_eq!(
            serde_json::to_value(d).unwrap().as_object().unwrap().len(),
            DRUM_PARAMS.len()
        );
    }

    #[test]
    fn set_clamps_and_rounds() {
        let mut s = SynthPatch::default();
        s.set("cutoff", 99999.0).unwrap();
        assert_eq!(s.cutoff, 16000.0);
        s.set("mono", 0.7).unwrap();
        assert_eq!(s.mono, 1.0);
        s.set("attack", f32::NAN).unwrap();
        assert_eq!(s.attack, 0.001);
        assert!(s.set("nope", 1.0).is_err());
        let mut d = DrumPad::default();
        d.set("bursts", 3.4).unwrap();
        assert_eq!(d.bursts, 3.0);
        // a file with missing or wild values loads into range
        let loaded: DrumPad = serde_json::from_str(r#"{"tone_start": -5, "bits": 99}"#).unwrap();
        let loaded = loaded.clamped();
        assert_eq!(loaded.tone_start, 20.0);
        assert_eq!(loaded.bits, 16.0);
        assert_eq!(loaded.gain, 1.0);
    }
}
