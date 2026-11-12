//! Built-in sample bank (the sampled drum kits), embedded in the binary.
//!
//! Dust comes from VS Chamber Orchestra Community Edition (CC0 1.0); Shadow
//! and Session are cut from lofisons' own tracks. See SAMPLES.md.

use std::sync::{Arc, OnceLock};

use crate::instruments::SampleData;
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
    match kit {
        DrumKitId::Shadow => SHADOW_CELL.get_or_init(|| build(SHADOW)),
        DrumKitId::Session => SESSION_CELL.get_or_init(|| build(SESSION)),
        _ => DUST_CELL.get_or_init(|| build(DUST)),
    }
    .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kit_decodes() {
        for id in [DrumKitId::Dust, DrumKitId::Shadow, DrumKitId::Session] {
            let (kit, slices) = sampled_kit(id);
            assert_eq!(slices.len(), 16);
            assert!(slices
                .iter()
                .all(|&(a, b)| b > a && (b as usize) <= kit.frames.len()));
        }
    }
}
