//! Turning raw microphone recordings into playable instruments:
//! trimming, normalizing, pitch detection (melodic) and onset slicing (kits).

use crate::instruments::SampleData;
use crate::model::DRUM_PADS;

/// Trim leading/trailing silence, remove DC, normalize to -1 dBFS and apply
/// short fades so the sample can be cut anywhere without clicks.
pub fn prepare(raw: &[f32], sample_rate: u32) -> Option<SampleData> {
    if raw.is_empty() {
        return None;
    }
    let mean = raw.iter().sum::<f32>() / raw.len() as f32;
    let peak = raw.iter().fold(0.0f32, |a, &b| a.max((b - mean).abs()));
    if peak < 1e-3 {
        return None;
    }
    let threshold = peak * 0.03; // ~ -30 dB below the peak
    let start = raw.iter().position(|&x| (x - mean).abs() > threshold)?;
    let end = raw.iter().rposition(|&x| (x - mean).abs() > threshold)?;
    // keep a little pre-roll so transients are intact
    let pre = (sample_rate as usize / 200).min(start);
    let post = (sample_rate as usize / 10).min(raw.len() - 1 - end);
    let gain = 0.89 / peak;
    let mut frames: Vec<f32> = raw[start - pre..=end + post]
        .iter()
        .map(|&x| (x - mean) * gain)
        .collect();
    let fade = (sample_rate as usize / 500).min(frames.len() / 4).max(1);
    let n = frames.len();
    for i in 0..fade {
        let g = i as f32 / fade as f32;
        frames[i] *= g;
        frames[n - 1 - i] *= g;
    }
    Some(SampleData {
        frames,
        sample_rate,
    })
}

/// Detect the fundamental of a (roughly) pitched sound using the YIN
/// difference function on the loudest region. Returns a MIDI pitch.
pub fn detect_pitch(sample: &SampleData) -> Option<u8> {
    let sr = sample.sample_rate as f32;
    let frames = &sample.frames;
    let win = (sr * 0.05) as usize; // 50 ms window, enough down to ~40 Hz
    let max_lag = (sr / 50.0) as usize;
    let min_lag = (sr / 1500.0) as usize;
    if frames.len() < win + max_lag {
        return None;
    }
    // analyze the region right after the loudest point (past the attack)
    let loudest = frames
        .chunks(win / 2)
        .enumerate()
        .max_by(|a, b| energy(a.1).total_cmp(&energy(b.1)))
        .map(|(i, _)| i * win / 2)
        .unwrap_or(0);
    let start = (loudest + win / 2).min(frames.len() - win - max_lag);
    let x = &frames[start..start + win + max_lag];

    let mut diff = vec![0.0f32; max_lag + 1];
    for tau in 1..=max_lag {
        let mut s = 0.0;
        for i in 0..win {
            let d = x[i] - x[i + tau];
            s += d * d;
        }
        diff[tau] = s;
    }
    // cumulative mean normalized difference
    let mut cmnd = vec![1.0f32; max_lag + 1];
    let mut running = 0.0;
    for tau in 1..=max_lag {
        running += diff[tau];
        cmnd[tau] = if running > 0.0 {
            diff[tau] * tau as f32 / running
        } else {
            1.0
        };
    }
    let mut best = None;
    for tau in min_lag.max(2)..max_lag {
        if cmnd[tau] < 0.15 {
            // descend to the local minimum
            let mut t = tau;
            while t + 1 < max_lag && cmnd[t + 1] < cmnd[t] {
                t += 1;
            }
            best = Some(t);
            break;
        }
    }
    // no clear dip under the threshold: accept the global minimum if decent
    let tau = best.or_else(|| {
        let (t, v) = cmnd
            .iter()
            .enumerate()
            .take(max_lag)
            .skip(min_lag.max(2))
            .min_by(|a, b| a.1.total_cmp(b.1))?;
        (*v < 0.4).then_some(t)
    })?;
    // parabolic interpolation for sub-sample accuracy
    let (a, b, c) = (cmnd[tau - 1], cmnd[tau], cmnd[tau + 1]);
    let denom = a - 2.0 * b + c;
    let shift = if denom.abs() > 1e-9 {
        0.5 * (a - c) / denom
    } else {
        0.0
    };
    let freq = sr / (tau as f32 + shift);
    let midi = 69.0 + 12.0 * (freq / 440.0).log2();
    if (0.0..=127.0).contains(&midi) {
        Some(midi.round() as u8)
    } else {
        None
    }
}

fn energy(x: &[f32]) -> f32 {
    x.iter().map(|v| v * v).sum()
}

/// Split a recording into percussive hits using an energy-flux onset
/// detector. Returns up to `DRUM_PADS` (start, end) frame ranges.
pub fn slice_onsets(sample: &SampleData) -> Vec<(u32, u32)> {
    let sr = sample.sample_rate as usize;
    let hop = (sr / 200).max(1); // 5 ms
    let frames = &sample.frames;
    let env: Vec<f32> = frames
        .chunks(hop)
        .map(|c| (energy(c) / c.len() as f32).sqrt())
        .collect();
    if env.is_empty() {
        return vec![(0, frames.len() as u32)];
    }
    let peak = env.iter().fold(0.0f32, |a, &b| a.max(b));
    let min_gap = (0.08 * sr as f32 / hop as f32) as usize; // 80 ms between hits
    let mut onsets = vec![0usize];
    let mut last = 0usize;
    for i in 1..env.len() {
        let rise = env[i] - env[i - 1];
        let local_floor = env[i.saturating_sub(4)..i]
            .iter()
            .fold(f32::MAX, |a, &b| a.min(b));
        if rise > peak * 0.12
            && env[i] > local_floor * 2.5
            && i - last >= min_gap
            && env[i] > peak * 0.15
        {
            onsets.push(i);
            last = i;
        }
    }
    onsets.truncate(DRUM_PADS);
    let mut slices = Vec::with_capacity(onsets.len());
    for (k, &o) in onsets.iter().enumerate() {
        let start = (o * hop).saturating_sub(hop); // include the attack
        let end = if k + 1 < onsets.len() {
            onsets[k + 1] * hop - hop
        } else {
            frames.len()
        };
        let start = if k == 0 { 0 } else { start };
        if end > start + hop {
            slices.push((start as u32, end.min(frames.len()) as u32));
        }
    }
    if slices.is_empty() {
        slices.push((0, frames.len() as u32));
    }
    slices
}

/// Encode a sample as 16-bit mono WAV bytes (used inside project files).
pub fn to_wav_bytes(sample: &SampleData) -> Vec<u8> {
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: sample.sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::new(&mut cursor, spec).expect("wav header");
        for &x in &sample.frames {
            w.write_sample((x.clamp(-1.0, 1.0) * 32767.0) as i16)
                .expect("wav write");
        }
        w.finalize().expect("wav finalize");
    }
    cursor.into_inner()
}

/// Decode any PCM WAV into a mono sample.
pub fn from_wav_bytes(bytes: &[u8]) -> Result<SampleData, String> {
    let mut r = hound::WavReader::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let spec = r.spec();
    let ch = spec.channels.max(1) as usize;
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r
            .samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>()
                .map(|s| s.map(|v| v as f32 * scale))
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?
        }
    };
    let frames = interleaved
        .chunks(ch)
        .map(|c| c.iter().sum::<f32>() / ch as f32)
        .collect();
    Ok(SampleData {
        frames,
        sample_rate: spec.sample_rate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freq: f32, secs: f32, sr: u32) -> Vec<f32> {
        (0..(secs * sr as f32) as usize)
            .map(|i| {
                let t = i as f32 / sr as f32;
                // a few harmonics, like a voice or a plucked string
                0.5 * (2.0 * std::f32::consts::PI * freq * t).sin()
                    + 0.25 * (4.0 * std::f32::consts::PI * freq * t).sin()
                    + 0.1 * (6.0 * std::f32::consts::PI * freq * t).sin()
            })
            .collect()
    }

    #[test]
    fn detects_pitch_of_harmonic_tones() {
        for (freq, midi) in [
            (110.0, 45u8),
            (220.0, 57),
            (261.63, 60),
            (440.0, 69),
            (659.26, 76),
        ] {
            let mut raw = vec![0.0; 4800];
            raw.extend(tone(freq, 0.6, 48000));
            raw.extend(vec![0.0; 4800]);
            let s = prepare(&raw, 48000).unwrap();
            assert_eq!(detect_pitch(&s), Some(midi), "freq {freq}");
        }
    }

    #[test]
    fn prepare_trims_and_normalizes() {
        let mut raw = vec![0.0; 10000];
        raw.extend(tone(300.0, 0.2, 48000).iter().map(|x| x * 0.1));
        raw.extend(vec![0.0; 10000]);
        let s = prepare(&raw, 48000).unwrap();
        assert!(s.frames.len() < 9600 + 4800 + 400);
        let peak = s.frames.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!((peak - 0.89).abs() < 0.05);
        assert!(prepare(&vec![0.0; 1000], 48000).is_none());
    }

    #[test]
    fn slices_claps() {
        let sr = 48000;
        let mut raw = Vec::new();
        let mut noise = crate::dsp::Noise::new(7);
        for _ in 0..4 {
            raw.extend(vec![0.0; sr as usize / 4]);
            for i in 0..(sr as usize / 20) {
                raw.push(noise.next() * (1.0 - i as f32 / (sr as f32 / 20.0)));
            }
        }
        raw.extend(vec![0.0; sr as usize / 4]);
        let s = prepare(&raw, sr).unwrap();
        let slices = slice_onsets(&s);
        assert_eq!(slices.len(), 4, "{slices:?}");
    }

    #[test]
    fn wav_roundtrip() {
        let s = SampleData {
            frames: vec![0.0, 0.5, -0.5, 0.25],
            sample_rate: 44100,
        };
        let back = from_wav_bytes(&to_wav_bytes(&s)).unwrap();
        assert_eq!(back.sample_rate, 44100);
        for (a, b) in s.frames.iter().zip(&back.frames) {
            assert!((a - b).abs() < 1e-3);
        }
    }
}
