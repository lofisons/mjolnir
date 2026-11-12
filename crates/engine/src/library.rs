//! Built-in sample bank (the sampled drum kits), embedded in the binary.
//!
//! Dust comes from VS Chamber Orchestra Community Edition (CC0 1.0); Shadow
//! and Session are cut from lofisons' own tracks. See SAMPLES.md.

use std::sync::{Arc, OnceLock};

use crate::instruments::{SampleData, Zone};
use crate::model::DrumKitId;
use crate::sampling::from_wav_bytes;

macro_rules! asset {
    ($($part:literal),+) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/samples/",
            $($part,)+
            ".wav"
        ))
        .as_slice()
    };
}

macro_rules! kit {
    ($prefix:literal) => {
        [
            asset!($prefix, "_00"),
            asset!($prefix, "_01"),
            asset!($prefix, "_02"),
            asset!($prefix, "_03"),
            asset!($prefix, "_04"),
            asset!($prefix, "_05"),
            asset!($prefix, "_06"),
            asset!($prefix, "_07"),
            asset!($prefix, "_08"),
            asset!($prefix, "_09"),
            asset!($prefix, "_10"),
            asset!($prefix, "_11"),
            asset!($prefix, "_12"),
            asset!($prefix, "_13"),
            asset!($prefix, "_14"),
            asset!($prefix, "_15"),
        ]
    };
}

/// Dust kit, one sample per pad in drum-map order.
const DUST: [&[u8]; 16] = [
    asset!("kit_00_kick"),
    asset!("kit_01_snare"),
    asset!("kit_02_hand"),
    asset!("kit_03_tamb"),
    asset!("kit_04_shake"),
    asset!("kit_05_tumba"),
    asset!("kit_06_conga"),
    asset!("kit_07_quinto"),
    asset!("kit_08_rim"),
    asset!("kit_09_cowbell"),
    asset!("kit_10_crash"),
    asset!("kit_11_ride"),
    asset!("kit_12_guiro"),
    asset!("kit_13_kick2"),
    asset!("kit_14_taps"),
    asset!("kit_15_claves"),
];
/// Cut from lofisons' own drum stems (see SAMPLES.md).
const SHADOW: [&[u8]; 16] = kit!("shadow");
const SESSION: [&[u8]; 16] = kit!("session");
const SHADOW_CHOPS: [&[u8]; 16] = kit!("shadowchops");
const SILENT_CHOPS: [&[u8]; 16] = kit!("silentchops");

type Kit = (Arc<SampleData>, Vec<(u32, u32)>);

fn build(files: [&[u8]; 16]) -> Kit {
    let mut frames = Vec::new();
    let mut slices = Vec::with_capacity(files.len());
    let mut rate = 24000;
    for bytes in files {
        let s = from_wav_bytes(bytes).expect("built-in kit sample");
        rate = s.sample_rate;
        let start = frames.len() as u32;
        frames.extend_from_slice(&s.frames);
        slices.push((start, frames.len() as u32));
        // a little silence between hits so interpolation never bleeds
        frames.extend_from_slice(&[0.0; 8]);
    }
    (
        Arc::new(SampleData {
            frames,
            sample_rate: rate,
        }),
        slices,
    )
}

/// A sampled kit as one buffer plus (start, end) slices per pad, ready for
/// the slice sampler. A kit that is not sampled answers with Dust.
pub fn sampled_kit(kit: DrumKitId) -> Kit {
    static DUST_CELL: OnceLock<Kit> = OnceLock::new();
    static SHADOW_CELL: OnceLock<Kit> = OnceLock::new();
    static SESSION_CELL: OnceLock<Kit> = OnceLock::new();
    static SHADOW_CHOPS_CELL: OnceLock<Kit> = OnceLock::new();
    static SILENT_CHOPS_CELL: OnceLock<Kit> = OnceLock::new();
    match kit {
        DrumKitId::ShadowChops => SHADOW_CHOPS_CELL.get_or_init(|| build(SHADOW_CHOPS)),
        DrumKitId::SilentChops => SILENT_CHOPS_CELL.get_or_init(|| build(SILENT_CHOPS)),
        DrumKitId::Shadow => SHADOW_CELL.get_or_init(|| build(SHADOW)),
        DrumKitId::Session => SESSION_CELL.get_or_init(|| build(SESSION)),
        _ => DUST_CELL.get_or_init(|| build(DUST)),
    }
    .clone()
}

/// One recorded note or passage by name (the sampled basses and airs).
pub fn tone(name: &str) -> Arc<SampleData> {
    static CELL: OnceLock<Vec<(&'static str, Arc<SampleData>)>> = OnceLock::new();
    let all = CELL.get_or_init(|| {
        let files: [(&'static str, &[u8]); 6] = [
            ("bass_shadow", asset!("bass_shadow")),
            ("bass_silent", asset!("bass_silent")),
            ("air_shadow", asset!("air_shadow")),
            ("air_silent", asset!("air_silent")),
            ("lead_shadow", asset!("lead_shadow")),
            ("lead_silent", asset!("lead_silent")),
        ];
        files
            .into_iter()
            .map(|(name, bytes)| {
                let s = from_wav_bytes(bytes).expect("built-in tone");
                (
                    name,
                    Arc::new(SampleData {
                        frames: s.frames,
                        sample_rate: s.sample_rate,
                    }),
                )
            })
            .collect()
    });
    all.iter()
        .find(|(n, _)| *n == name)
        .map(|(_, s)| s.clone())
        .unwrap_or_else(|| all[0].1.clone())
}

/// A multi-sampled instrument: its recording, where each note sits in it,
/// and the envelope it is played with.
#[derive(Clone)]
pub struct Recorded {
    pub sample: Arc<SampleData>,
    pub zones: Vec<Zone>,
    pub attack: f32,
    pub release: f32,
    pub gain: f32,
}

#[derive(serde::Deserialize)]
struct RecordedMeta {
    attack: f32,
    release: f32,
    gain: f32,
    zones: Vec<Zone>,
}

macro_rules! recorded {
    ($name:literal) => {
        (
            $name,
            asset!($name),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/samples/",
                $name,
                ".json"
            )),
        )
    };
}

/// One of the real instruments by name (see `SynthPreset::zoned`).
pub fn instrument(name: &str) -> Recorded {
    static CELL: OnceLock<Vec<(&'static str, Recorded)>> = OnceLock::new();
    let all = CELL.get_or_init(|| {
        let files: [(&'static str, &[u8], &'static str); 7] = [
            recorded!("inst_piano"),
            recorded!("inst_musicbox"),
            recorded!("inst_harp"),
            recorded!("inst_strings"),
            recorded!("inst_organ"),
            recorded!("inst_flute"),
            recorded!("inst_contrabass"),
        ];
        files
            .into_iter()
            .map(|(name, wav, meta)| {
                let s = from_wav_bytes(wav).expect("built-in instrument");
                let m: RecordedMeta = serde_json::from_str(meta).expect("built-in instrument map");
                (
                    name,
                    Recorded {
                        sample: Arc::new(SampleData {
                            frames: s.frames,
                            sample_rate: s.sample_rate,
                        }),
                        zones: m.zones,
                        attack: m.attack,
                        release: m.release,
                        gain: m.gain,
                    },
                )
            })
            .collect()
    });
    all.iter()
        .find(|(n, _)| *n == name)
        .map(|(_, r)| r.clone())
        .unwrap_or_else(|| all[0].1.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instruments_decode_and_their_zones_fit() {
        for name in [
            "inst_piano",
            "inst_musicbox",
            "inst_harp",
            "inst_strings",
            "inst_organ",
            "inst_flute",
            "inst_contrabass",
        ] {
            let i = instrument(name);
            assert!(!i.zones.is_empty(), "{name}");
            for z in &i.zones {
                assert!(
                    z.start < z.end && (z.end as usize) <= i.sample.frames.len(),
                    "{name}"
                );
                if let Some(l) = z.loop_start {
                    assert!(l > z.start && l < z.end, "{name}");
                }
            }
        }
    }

    #[test]
    fn kit_decodes() {
        for id in [
            DrumKitId::Dust,
            DrumKitId::Shadow,
            DrumKitId::Session,
            DrumKitId::ShadowChops,
            DrumKitId::SilentChops,
        ] {
            let (kit, slices) = sampled_kit(id);
            assert_eq!(slices.len(), 16);
            assert!(slices
                .iter()
                .all(|&(a, b)| b > a && (b as usize) <= kit.frames.len()));
        }
    }
}
