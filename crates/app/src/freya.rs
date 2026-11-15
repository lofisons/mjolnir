//! Finished by Freya. Mjolnir makes the music; Freya, lofisons' mixing
//! helper, finishes it: every track is bounced on its own, given a role
//! (drums, bass, keys…) and handed to Freya's engine, which sets a chain,
//! a level and a place for each by what the genre asks, and masters the sum
//! to the genre's loudness.
//!
//! Offline only: nothing here touches the audio thread.
//!
//! What stays of the set: each track's own sound, its knobs, its sends to
//! Mjolnir's room and echo, and its pan when anything was panned. What does
//! not: Mjolnir's master (its limiter, tape, warmth and vinyl) and its pump,
//! because Freya is the master on this path. Tape, warmth and vinyl that
//! were dialled in carry over as Freya's own tape and dust.

use std::collections::HashMap;
use std::sync::Arc;

use freya_engine::analysis::{analyze_audio, lufs_stereo, role_from_name, role_from_sound, Stats};
use freya_engine::auto::{auto_mix, fit_master};
use freya_engine::mixer::{peak_db, render, What};
use freya_engine::model::{Audio, Genre, Role, Session, Track as FreyaTrack};
use freya_engine::presets;
use mjolnir_engine::export;
use mjolnir_engine::instruments::SampleData;
use mjolnir_engine::model::{InstrumentSpec, Master, Project, SampleId, SynthPreset, TextureKind, Track};
use serde_json::{json, Value};

/// A stem leaves Mjolnir well under its limiter, so nothing is shaped twice.
const STEM_GAIN: f32 = 0.25;

pub struct Finished {
    /// Interleaved stereo at the sample rate asked for.
    pub audio: Vec<f32>,
    /// What Freya did and how the result measures.
    pub report: Value,
}

/// Freya's genres: id, label, what it does, how loud it aims.
pub fn genres() -> Value {
    json!(presets::genres())
}

pub fn genre_ids() -> Vec<&'static str> {
    Genre::ALL.iter().map(|g| g.id()).collect()
}

fn genre_of(id: &str) -> Result<Genre, String> {
    Genre::from_id(id).ok_or_else(|| format!("freya has no genre called \"{id}\" → there is: {}", genre_ids().join(", ")))
}

/// What a track is in a mix, from its sound; `None` when only listening can tell.
fn role_of_sound(t: &Track) -> Option<Role> {
    Some(match &t.instrument {
        InstrumentSpec::Drums { .. } | InstrumentSpec::SampleKit { .. } | InstrumentSpec::CustomKit { .. } => Role::Drums,
        InstrumentSpec::Synth { preset } => match preset {
            SynthPreset::SubBass | SynthPreset::Bass808 | SynthPreset::ShadowSub | SynthPreset::SilentSub => Role::Sub,
            SynthPreset::Bass | SynthPreset::SessionBass | SynthPreset::Contrabass => Role::Bass,
            SynthPreset::Lead | SynthPreset::Flute => Role::Lead,
            p if p.category() == "pads" => Role::Synth,
            SynthPreset::Pluck => Role::Synth,
            _ => Role::Keys,
        },
        InstrumentSpec::Granular { texture, .. } => match texture {
            TextureKind::ShadowLead | TextureKind::SilentLead => Role::Lead,
            _ => Role::Synth,
        },
        InstrumentSpec::SampleMelodic { .. } | InstrumentSpec::CustomSynth { .. } | InstrumentSpec::Macro { .. } => return None,
    })
}

/// The role of a track: its name when it says one, then its sound's kind,
/// then how the stem itself measures.
fn role_of(t: &Track, stats: &Stats) -> Role {
    let sound_name = match &t.instrument {
        InstrumentSpec::CustomSynth { name, .. } | InstrumentSpec::Macro { name, .. } => name.as_str(),
        _ => "",
    };
    let drums = matches!(role_of_sound(t), Some(Role::Drums));
    let named = |name: &str| {
        role_from_name(name).filter(|r| match r {
            // a track is never the whole song, and a kit named "kick" is still a kit
            Role::Mix => false,
            Role::Kick | Role::Snare | Role::Hats | Role::Toms | Role::Perc | Role::Drums => drums,
            _ => !drums,
        })
    };
    named(&t.name).or_else(|| role_of_sound(t)).or_else(|| named(sound_name)).unwrap_or_else(|| match role_from_sound(stats) {
        Role::Bass => Role::Bass,
        // anything else a synth plays sits where synths sit
        _ => Role::Synth,
    })
}

/// Bounce the song finished by Freya in a genre.
pub fn finish(project: &Project, samples: &HashMap<SampleId, Arc<SampleData>>, sample_rate: u32, genre: &str) -> Result<Finished, String> {
    let genre = genre_of(genre)?;
    if project.song.length_bars() == 0 {
        return Err("the song is empty → place loops on the timeline first".into());
    }
    let sr = sample_rate as f32;
    // each stem leaves through a master that does nothing but pass it on, quietly
    let mut bare = project.clone();
    bare.master = Master { volume: (STEM_GAIN / 1.5).sqrt(), vinyl: 0.0, tape: 0.0, warmth: 0.0, space: project.master.space, pump: 0.0 };
    let any_solo = project.tracks.iter().any(|t| t.solo);
    let mut session = Session { name: project.name.clone(), genre, ..Session::default() };
    let mut audio = Vec::new();
    let mut stats = Vec::new();
    let mut from = Vec::new();
    for (i, t) in project.tracks.iter().enumerate() {
        if t.mute || (any_solo && !t.solo) || export::flatten_track(project, i).is_empty() {
            continue;
        }
        let stem = Audio::new(export::render(&bare, samples, sample_rate, Some(i), 2.0), 2);
        let s = analyze_audio(&stem, sr);
        if s.lufs < -70.0 {
            continue;
        }
        let name = if t.name.trim().is_empty() { format!("track {}", i + 1) } else { t.name.clone() };
        session.tracks.push(FreyaTrack::named(&name, role_of(t, &s)));
        audio.push(Arc::new(stem));
        stats.push(s);
        from.push(i);
    }
    if session.tracks.is_empty() {
        return Err("there is nothing to hear in the song → check that its tracks are not all muted".into());
    }
    auto_mix(&mut session, &audio, &stats, sr);
    // what the person already decided stays decided
    let panned = from.iter().any(|i| project.tracks[*i].pan.abs() > 0.01);
    for (ft, i) in session.tracks.iter_mut().zip(&from) {
        let t = &project.tracks[*i];
        if panned {
            // the stem already sits where it was put
            ft.pan = 0.0;
            if t.pan.abs() > 0.01 {
                ft.chain.width = ft.chain.width.max(1.0);
            }
        }
        // a track that already sends to Mjolnir's room or echo gets less of Freya's
        ft.chain.reverb *= 1.0 - t.macros.0[4].clamp(0.0, 1.0);
        ft.chain.echo *= 1.0 - t.macros.0[5].clamp(0.0, 1.0);
    }
    // tape, warmth and vinyl that were dialled in, in Freya's voice
    let m = project.master;
    session.master.lofi = session.master.lofi.max(0.6 * m.vinyl.max(m.tape)).clamp(0.0, 1.0);
    session.master.tape = session.master.tape.max(0.6 * m.warmth).clamp(0.0, 1.0);
    let report = fit_master(&mut session, &audio, &stats, sr);
    let out = render(&session, &audio, sr, What::Mix);
    let lufs = lufs_stereo(&out, sr);
    // through f64, so the numbers read as they were rounded
    let round1 = |v: f32| (v as f64 * 10.0).round() / 10.0;
    let round2 = |v: f32| (v as f64 * 100.0).round() / 100.0;
    let report = json!({
        "genre": genre.id(),
        "seconds": round1(out.len() as f32 / 2.0 / sr),
        "lufs": round1(lufs),
        "target_lufs": round1(session.master.target_lufs),
        "peak_db": round1(peak_db(&out)),
        "limiting_db": round1(report.limiting_db),
        "tracks": session.tracks.iter().zip(&from).map(|(ft, i)| json!({ "track": i, "name": ft.name, "role": ft.role.id(), "gain_db": round1(ft.gain_db), "pan": round2(ft.pan) })).collect::<Vec<_>>(),
        "notes": report.notes.iter().map(|n| n.text.clone()).collect::<Vec<_>>(),
    });
    Ok(Finished { audio: out, report })
}
