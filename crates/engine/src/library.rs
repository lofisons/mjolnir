//! Built-in sample bank (the Dust drum kit), embedded in the binary.
//!
//! Source: VS Chamber Orchestra Community Edition (VSCO-2 CE), CC0 1.0.
//! Regenerate with `python3 tools/build_samples.py` (see SAMPLES.md).

use std::sync::{Arc, OnceLock};

use crate::instruments::SampleData;
use crate::sampling::from_wav_bytes;

macro_rules! asset {
    ($name:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/samples/",
            $name,
            ".wav"
        ))
        .as_slice()
    };
}

/// Dust kit, one sample per pad in drum-map order.
const KIT: [&[u8]; 16] = [
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

/// The Dust kit as one buffer plus (start, end) slices per pad, ready for
/// the slice sampler.
pub fn dusty_kit() -> (Arc<SampleData>, Vec<(u32, u32)>) {
    static CELL: OnceLock<(Arc<SampleData>, Vec<(u32, u32)>)> = OnceLock::new();
    CELL.get_or_init(|| {
        let mut frames = Vec::new();
        let mut slices = Vec::with_capacity(KIT.len());
        let mut rate = 24000;
        for bytes in KIT {
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
    })
    .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kit_decodes() {
        let (kit, slices) = dusty_kit();
        assert_eq!(slices.len(), 16);
        assert!(slices
            .iter()
            .all(|&(a, b)| b > a && (b as usize) <= kit.frames.len()));
    }
}
