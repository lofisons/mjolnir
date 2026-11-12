//! Bouncing the beat: render the song timeline to a WAV (plus optional
//! per-track stems and a multitrack MIDI file), all offline and deterministic.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::controller::track_runtime;
use crate::engine::{ClipSpan, Command, Engine, SongRuntime};
use crate::instruments::SampleData;
use crate::model::{Project, SampleId, TrackKind, PPQ};

/// The song timeline in engine form (looping off: exports play once).
pub fn song_runtime(project: &Project) -> SongRuntime {
    let bar = project.bar_ticks() as f64;
    let lanes = project
        .tracks
        .iter()
        .map(|t| {
            let mut lane: Vec<ClipSpan> = project
                .song
                .clips
                .iter()
                .filter(|c| c.track == t.id && t.slots.get(c.slot).is_some_and(|p| p.is_some()))
                .map(|c| ClipSpan {
                    start: c.start as f64 * bar,
                    end: c.end() as f64 * bar,
                    slot: c.slot,
                })
                .collect();
            lane.sort_by(|a, b| a.start.total_cmp(&b.start));
            lane
        })
        .collect();
    SongRuntime {
        lanes,
        length: project.song.length_bars() as f64 * bar,
        looping: false,
        fade_in: project.song.fade_in as f64 * bar,
        fade_out: project.song.fade_out as f64 * bar,
    }
}

/// Absolute (tick, length, pitch, velocity) notes of one track along the song.
pub fn flatten_track(project: &Project, track: usize) -> Vec<(u32, u32, u8, f32)> {
    let mut out = Vec::new();
    let Some(t) = project.tracks.get(track) else {
        return out;
    };
    for clip in project.song.clips.iter().filter(|c| c.track == t.id) {
        let Some(p) = project.pattern(track, clip.slot) else {
            continue;
        };
        let bar = project.bar_ticks();
        let at = clip.start * bar;
        let section = clip.bars * bar;
        let len = p.length_ticks(bar);
        let mut loop_start = 0;
        while loop_start < section {
            for n in &p.notes {
                let s = loop_start + n.start;
                if s < section {
                    out.push((at + s, n.length.min(section - s), n.pitch, n.velocity));
                }
            }
            loop_start += len;
        }
    }
    out.sort_by_key(|n| (n.0, n.2));
    out
}

fn vlq(mut v: u32, out: &mut Vec<u8>) {
    let mut buf = [0u8; 5];
    let mut i = 4;
    buf[i] = (v & 0x7f) as u8;
    v >>= 7;
    while v > 0 {
        i -= 1;
        buf[i] = ((v & 0x7f) as u8) | 0x80;
        v >>= 7;
    }
    out.extend_from_slice(&buf[i..]);
}

fn chunk(id: &[u8; 4], data: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(id);
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(data);
}

fn meta_text(kind: u8, text: &str, out: &mut Vec<u8>) {
    out.extend_from_slice(&[0x00, 0xff, kind]);
    vlq(text.len() as u32, out);
    out.extend_from_slice(text.as_bytes());
}

/// Standard MIDI File (format 1): a tempo track plus one track per Yggdrasil
/// track. Drums go to channel 10 using General MIDI-ish note numbers.
pub fn to_midi(project: &Project) -> Vec<u8> {
    let mut file = Vec::new();
    let mut header = Vec::new();
    header.extend_from_slice(&1u16.to_be_bytes());
    header.extend_from_slice(&((project.tracks.len() + 1) as u16).to_be_bytes());
    header.extend_from_slice(&(PPQ as u16).to_be_bytes());
    chunk(b"MThd", &header, &mut file);

    let mut tempo = Vec::new();
    meta_text(0x03, &project.name, &mut tempo);
    let us_per_quarter = (60_000_000.0 / project.bpm as f64).round() as u32;
    tempo.extend_from_slice(&[0x00, 0xff, 0x51, 0x03]);
    tempo.extend_from_slice(&us_per_quarter.to_be_bytes()[1..]);
    // time signature: numerator, denominator as a power of two, clocks per click
    let m = project.meter;
    let clocks = (24 * 4 / m.unit) as u8;
    tempo.extend_from_slice(&[
        0x00,
        0xff,
        0x58,
        0x04,
        m.beats as u8,
        m.unit.trailing_zeros() as u8,
        clocks,
        8,
    ]);
    tempo.extend_from_slice(&[0x00, 0xff, 0x2f, 0x00]);
    chunk(b"MTrk", &tempo, &mut file);

    let mut melodic_channel = 0u8;
    for (ti, track) in project.tracks.iter().enumerate() {
        let channel = if track.kind == TrackKind::Drums {
            9
        } else {
            let c = melodic_channel;
            melodic_channel = (melodic_channel + 1) % 16;
            if melodic_channel == 9 {
                melodic_channel = 10;
            }
            c
        };
        let mut events: Vec<(u32, u8, u8, u8)> = Vec::new(); // tick, 0=off/1=on, pitch, vel
        for (start, len, pitch, vel) in flatten_track(project, ti) {
            let v = ((vel * 127.0).round() as u8).clamp(1, 127);
            events.push((start, 1, pitch, v));
            events.push((start + len.max(1), 0, pitch, 0));
        }
        events.sort_by_key(|e| (e.0, e.1, e.2));
        let mut data = Vec::new();
        meta_text(0x03, &track.name, &mut data);
        let mut last = 0;
        for (tick, on, pitch, vel) in events {
            vlq(tick - last, &mut data);
            last = tick;
            let status = if on == 1 { 0x90 } else { 0x80 } | channel;
            data.extend_from_slice(&[status, pitch, if on == 1 { vel } else { 64 }]);
        }
        data.extend_from_slice(&[0x00, 0xff, 0x2f, 0x00]);
        chunk(b"MTrk", &data, &mut file);
    }
    file
}

/// Render the song offline, through the same engine and master bus as live
/// playback. `solo` renders a single track (a stem) without the master's
/// lo-fi colour. Returns interleaved stereo samples.
pub fn render(
    project: &Project,
    samples: &HashMap<SampleId, Arc<SampleData>>,
    sample_rate: u32,
    solo: Option<usize>,
    tail_seconds: f32,
) -> Vec<f32> {
    let sr = sample_rate as f32;
    let (mut engine, mut ep) = Engine::new(sr);
    let mut send = |engine: &mut Engine, cmd: Command| {
        let mut cmd = cmd;
        loop {
            match ep.commands.push(cmd) {
                Ok(()) => break,
                Err(rtrb::PushError::Full(c)) => {
                    cmd = c;
                    let mut scratch = [0.0f32; 2];
                    engine.process(&mut scratch, 2);
                }
            }
        }
    };
    for (i, t) in project.tracks.iter().enumerate() {
        send(
            &mut engine,
            Command::InsertTrack {
                index: i,
                track: Box::new(track_runtime(t, samples, sr)),
            },
        );
        let (mute, solo_flag) = match solo {
            Some(s) => (s != i, false),
            None => (t.mute, t.solo),
        };
        send(
            &mut engine,
            Command::SetMix {
                track: i,
                mute,
                solo: solo_flag,
                pan: t.pan,
            },
        );
    }
    let mut master = project.master;
    if solo.is_some() {
        // stems: keep levels comparable but skip vinyl noise so stems stay clean
        master.vinyl = 0.0;
    }
    send(&mut engine, Command::SetMaster(master));
    send(
        &mut engine,
        Command::SetTempo {
            bpm: project.bpm,
            swing: project.swing,
        },
    );
    send(
        &mut engine,
        Command::SetMeter {
            bar: project.bar_ticks(),
            beat: project.meter.beat_ticks(),
        },
    );
    send(
        &mut engine,
        Command::SetSong(Box::new(song_runtime(project))),
    );
    send(&mut engine, Command::SetSongMode(true));
    send(&mut engine, Command::Seek { tick: 0.0 });
    send(&mut engine, Command::Play);

    let samples_per_tick = sr as f64 * 60.0 / (project.bpm as f64 * PPQ as f64);
    let song_ticks = project.song.length_bars() as f64 * project.bar_ticks() as f64;
    let total = (song_ticks * samples_per_tick).round() as usize + (tail_seconds * sr) as usize;
    let mut out = Vec::with_capacity(total * 2);
    let mut buf = vec![0.0f32; 2 * 512];
    let mut done = 0;
    while done < total {
        let n = (total - done).min(512);
        engine.process(&mut buf[..n * 2], 2);
        out.extend_from_slice(&buf[..n * 2]);
        done += n;
        while ep.garbage.pop().is_ok() {}
    }
    out
}

pub fn write_wav(path: &Path, interleaved: &[f32], sample_rate: u32) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 24,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    for &x in interleaved {
        w.write_sample((x.clamp(-1.0, 1.0) * 8_388_607.0) as i32)
            .map_err(|e| e.to_string())?;
    }
    w.finalize().map_err(|e| e.to_string())
}

/// This build has no MP3 encoder (the `mp3` feature is off).
#[cfg(not(feature = "mp3"))]
pub fn encode_mp3(_: &[f32], _: u32, _: &str) -> Result<Vec<u8>, String> {
    Err("this build cannot write MP3: export WAV".into())
}

/// A 24-bit stereo WAV file in memory, for hosts without a file system.
pub fn wav_bytes(interleaved: &[f32], sample_rate: u32) -> Result<Vec<u8>, String> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 24,
        sample_format: hound::SampleFormat::Int,
    };
    let mut bytes = std::io::Cursor::new(Vec::new());
    {
        let mut w = hound::WavWriter::new(&mut bytes, spec).map_err(|e| e.to_string())?;
        for &x in interleaved {
            w.write_sample((x.clamp(-1.0, 1.0) * 8_388_607.0) as i32)
                .map_err(|e| e.to_string())?;
        }
        w.finalize().map_err(|e| e.to_string())?;
    }
    Ok(bytes.into_inner())
}

/// Encode interleaved stereo to MP3 (LAME, 320 kbps CBR, best quality).
#[cfg(feature = "mp3")]
pub fn encode_mp3(interleaved: &[f32], sample_rate: u32, title: &str) -> Result<Vec<u8>, String> {
    use mp3lame_encoder::{Bitrate, Builder, FlushNoGap, Id3Tag, InterleavedPcm, Quality};
    let err = |e: &dyn std::fmt::Debug| format!("MP3 encoder error: {e:?}");
    let mut b = Builder::new().ok_or("could not start the MP3 encoder")?;
    b.set_num_channels(2).map_err(|e| err(&e))?;
    b.set_sample_rate(sample_rate).map_err(|e| err(&e))?;
    b.set_brate(Bitrate::Kbps320).map_err(|e| err(&e))?;
    b.set_quality(Quality::Best).map_err(|e| err(&e))?;
    let title = title.as_bytes();
    b.set_id3_tag(Id3Tag {
        title: &title[..title.len().min(30)],
        artist: b"Mjolnir",
        album: &[],
        year: &[],
        comment: b"Made with Mjolnir",
        album_art: &[],
    })
    .map_err(|e| err(&e))?;
    let mut enc = b.build().map_err(|e| err(&e))?;
    let clipped: Vec<f32> = interleaved.iter().map(|x| x.clamp(-1.0, 1.0)).collect();
    let mut out = Vec::new();
    for chunk in clipped.chunks(2 * 8192) {
        out.reserve(mp3lame_encoder::max_required_buffer_size(chunk.len() / 2));
        enc.encode_to_vec(InterleavedPcm(chunk), &mut out)
            .map_err(|e| err(&e))?;
    }
    out.reserve(7200);
    enc.flush_to_vec::<FlushNoGap>(&mut out)
        .map_err(|e| err(&e))?;
    Ok(out)
}

fn safe_name(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let t = cleaned.trim();
    if t.is_empty() {
        "track".into()
    } else {
        t.to_string()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioFormat {
    #[default]
    Wav,
    Mp3,
}

impl AudioFormat {
    fn ext(self) -> &'static str {
        match self {
            AudioFormat::Wav => "wav",
            AudioFormat::Mp3 => "mp3",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, serde::Deserialize)]
pub struct ExportOptions {
    #[serde(default)]
    pub format: AudioFormat,
    /// Also write one WAV per track into a "<name> stems" folder.
    #[serde(default)]
    pub stems: bool,
    /// Also write a multitrack MIDI file.
    #[serde(default)]
    pub midi: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExportReport {
    pub file: PathBuf,
    pub files: Vec<PathBuf>,
    pub bars: u32,
    pub seconds: f32,
}

/// Bounce the song to `wav_path` (and optional stems/MIDI next to it).
pub fn export_beat(
    project: &Project,
    samples: &HashMap<SampleId, Arc<SampleData>>,
    wav_path: &Path,
    sample_rate: u32,
    options: ExportOptions,
) -> Result<ExportReport, String> {
    let bars = project.song.length_bars();
    if bars == 0 {
        return Err("the song is empty: build your beat on the timeline first".into());
    }
    let ext = options.format.ext();
    let wav_path = if wav_path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    {
        wav_path.to_path_buf()
    } else {
        wav_path.with_extension(ext)
    };
    if let Some(dir) = wav_path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tail = 2.0;
    let mix = render(project, samples, sample_rate, None, tail);
    match options.format {
        AudioFormat::Wav => write_wav(&wav_path, &mix, sample_rate)?,
        AudioFormat::Mp3 => {
            let bytes = encode_mp3(&mix, sample_rate, &project.name)?;
            std::fs::write(&wav_path, bytes).map_err(|e| e.to_string())?;
        }
    }
    let mut files = vec![wav_path.clone()];
    let stem_name = wav_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "beat".into());

    if options.midi {
        let midi_path = wav_path.with_extension("mid");
        std::fs::write(&midi_path, to_midi(project)).map_err(|e| e.to_string())?;
        files.push(midi_path);
    }
    if options.stems {
        let folder = wav_path.with_file_name(format!("{stem_name} stems"));
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        for (i, t) in project.tracks.iter().enumerate() {
            if flatten_track(project, i).is_empty() {
                continue;
            }
            let stem = render(project, samples, sample_rate, Some(i), tail);
            let path = folder.join(format!("{:02} {}.wav", i + 1, safe_name(&t.name)));
            write_wav(&path, &stem, sample_rate)?;
            files.push(path);
        }
    }
    Ok(ExportReport {
        file: wav_path,
        files,
        bars,
        seconds: bars as f32 * 4.0 * 60.0 / project.bpm,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{SongClip, TICKS_PER_4_4_BAR as TICKS_PER_BAR};

    fn peak(a: &[f32]) -> f32 {
        a.iter().fold(0.0f32, |m, b| m.max(b.abs()))
    }

    #[test]
    fn flatten_loops_patterns_over_clips() {
        let p = Project::starter();
        // drums: slot 1 for 8 bars (verse) then slot 2 for 4 bars (outro)
        let notes = flatten_track(&p, 0);
        let a = p.tracks[0].slots[1].as_ref().unwrap().notes.len();
        let b = p.tracks[0].slots[2].as_ref().unwrap().notes.len();
        assert_eq!(notes.len(), a * 8 + b * 4);
        assert!(notes.iter().all(|n| n.0 >= 4 * TICKS_PER_BAR));
        // pluck: a 2-bar loop over the 4-bar intro plays twice
        let pluck = flatten_track(&p, 3);
        let k = p.tracks[3].slots[0].as_ref().unwrap().notes.len();
        assert_eq!(
            pluck.iter().filter(|n| n.0 < 4 * TICKS_PER_BAR).count(),
            k * 2
        );
    }

    #[test]
    fn odd_meters_render_and_export_their_signature() {
        let mut p = Project::starter();
        p.set_meter(crate::model::Meter { beats: 7, unit: 8 });
        let bars = p.song.length_bars() as f64;
        let audio = render(&p, &HashMap::new(), 8000, None, 0.0);
        // 7/8 = 3.5 quarters per bar
        let expected = bars * 3.5 * 60.0 / p.bpm as f64 * 8000.0;
        assert!(((audio.len() / 2) as f64 - expected).abs() < 600.0);
        assert!(peak(&audio) > 0.01);
        let m = to_midi(&p);
        let sig = m.windows(5).position(|w| w == [0xff, 0x58, 0x04, 7, 3]);
        assert!(sig.is_some(), "7/8 time signature meta event");
        // every flattened note stays inside its clip on the 7/8 grid
        let bar = p.bar_ticks();
        assert!(flatten_track(&p, 0)
            .iter()
            .all(|n| n.0 < p.song.length_bars() * bar));
    }

    #[test]
    fn midi_is_well_formed() {
        let p = Project::starter();
        let m = to_midi(&p);
        assert_eq!(&m[0..4], b"MThd");
        let mut pos = 14;
        let mut tracks = 0;
        while pos < m.len() {
            assert_eq!(&m[pos..pos + 4], b"MTrk");
            let len = u32::from_be_bytes(m[pos + 4..pos + 8].try_into().unwrap()) as usize;
            pos += 8 + len;
            tracks += 1;
        }
        assert_eq!(pos, m.len());
        assert_eq!(tracks, p.tracks.len() + 1);
    }

    #[test]
    fn render_length_and_level() {
        let mut p = Project::starter();
        p.bpm = 120.0;
        let audio = render(&p, &HashMap::new(), 48000, None, 0.5);
        // 16 bars at 120 bpm = 32 s, + 0.5 s tail
        assert_eq!(audio.len(), 2 * (16 * 96000 + 24000));
        let pk = peak(&audio);
        assert!(pk > 0.1 && pk <= 1.0, "peak {pk}");
    }

    #[test]
    fn clips_start_on_their_bar_and_gaps_are_silent() {
        let mut p = Project::starter();
        p.bpm = 120.0;
        p.master.vinyl = 0.0;
        let drums = p.tracks[0].id;
        p.song.clips.clear();
        p.song.insert(SongClip {
            track: drums,
            slot: 1,
            start: 0,
            bars: 1,
        });
        p.song.insert(SongClip {
            track: drums,
            slot: 1,
            start: 2,
            bars: 1,
        });
        let audio = render(&p, &HashMap::new(), 48000, None, 0.0);
        let bar = 96000usize;
        let pk = |from: usize, to: usize| peak(&audio[from * 2..to * 2]);
        assert!(
            pk(2 * bar - 4800, 2 * bar - 10) < 0.02,
            "bar 2 should be silent at its end"
        );
        assert!(
            pk(2 * bar, 2 * bar + 960) > 0.1,
            "bar 3 should start with a hit"
        );
    }

    #[test]
    fn song_fades_shape_the_level() {
        let mut p = Project::starter();
        p.bpm = 120.0;
        p.master.vinyl = 0.0;
        p.song.fade_in = 2;
        p.song.fade_out = 2;
        let audio = render(&p, &HashMap::new(), 48000, None, 0.0);
        let bar = 96000usize;
        let rms = |from: usize, to: usize| {
            let s = &audio[from * 2..to * 2];
            (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt()
        };
        let start = rms(0, bar / 4);
        let middle = rms(6 * bar, 7 * bar);
        let end = rms(16 * bar - bar / 8, 16 * bar);
        assert!(start < middle * 0.5, "fade in: {start} vs {middle}");
        assert!(end < middle * 0.3, "fade out: {end} vs {middle}");
    }

    #[test]
    fn export_writes_files() {
        let dir = std::env::temp_dir().join(format!("yggdrasil-export-{}", std::process::id()));
        let mut p = Project::starter();
        p.bpm = 160.0;
        let r = export_beat(
            &p,
            &HashMap::new(),
            &dir.join("beat"),
            44100,
            ExportOptions {
                format: AudioFormat::Wav,
                stems: true,
                midi: true,
            },
        )
        .unwrap();
        assert_eq!(r.file, dir.join("beat.wav"));
        assert_eq!(r.files.len(), 6); // wav, midi, 4 stems
        for f in &r.files {
            assert!(std::fs::metadata(f).unwrap().len() > 100);
        }
        #[cfg(feature = "mp3")]
        {
            // MP3: a valid MPEG stream (ID3 header) of plausible size for 320 kbps
            let r = export_beat(
                &p,
                &HashMap::new(),
                &dir.join("beat"),
                44100,
                ExportOptions {
                    format: AudioFormat::Mp3,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(r.file, dir.join("beat.mp3"));
            let mp3 = std::fs::read(&r.file).unwrap();
            assert_eq!(&mp3[..3], b"ID3");
            let expected = (r.seconds + 2.0) * 320_000.0 / 8.0;
            assert!(
                (mp3.len() as f32) > expected * 0.8 && (mp3.len() as f32) < expected * 1.3,
                "{} vs {expected}",
                mp3.len()
            );
        }
        std::fs::remove_dir_all(dir).ok();
        let mut empty = Project::starter();
        empty.song.clips.clear();
        assert!(export_beat(
            &empty,
            &HashMap::new(),
            Path::new("/tmp/x.wav"),
            44100,
            ExportOptions::default()
        )
        .is_err());
    }
}
