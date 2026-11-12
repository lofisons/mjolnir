#!/usr/bin/env python3
"""Build Mjolnir's own sounds from the author's stems (lofisons tracks).

Drum kits (Shadow, Session): every hit in the drums-only mixes is found,
sorted into kick / snare / thud / hat / perc by where its energy sits, and
for each pad the most typical hit that stands alone (nothing ringing before
it, nothing landing on its tail) is cut out: mono, 16-bit, 32 kHz.

Basses: one clean, steady note from each bass stem, with its pitch.
Airs: four seconds of each pad stem where the harmony holds still; the
granular instrument plays them as drifting clouds.
Leads: the passages where one voice stands almost alone, played the same way.
Chops: sixteen slices of each pad stem, as a kit.

Usage:  python3 tools/build_stem_sounds.py <folder of stems as 48 kHz mono wav>
        (drums as <Track>.wav, the others as <Track>__bass.wav, <Track>__other.wav)
Needs:  numpy, librosa, soundfile
"""

import os
import sys
import warnings
import wave

warnings.filterwarnings("ignore")
import librosa
import numpy as np
import soundfile as sf

SR = 48000
RATE = 32000
OUT = os.path.join(os.path.dirname(__file__), "..", "crates", "engine", "assets", "samples")

# pad -> (stem, class, which, peak, label). `which` picks among the hits that
# stand alone: the typical one, or the typical long / short / bright / dark one.
KITS = {
    "shadow": [
        ("Shadow_Ambient", "kick", "typical", 0.95, "Kick"),
        ("Silent_Focus", "snare", "typical", 0.85, "Snare"),
        ("Shadow_Ambient", "snare", "long", 0.8, "Hall snare"),
        ("Shadow_Ambient", "hat", "short", 0.4, "Hat"),
        ("Raw_Frequency", "hat", "long", 0.4, "Open hat"),
        ("Void_Pulse", "kick", "long", 0.95, "Deep kick"),
        ("Raw_Frequency", "kick", "long", 0.95, "Boom"),
        ("Shadow_Ambient", "thud", "typical", 0.7, "Thud"),
        ("Raw_Frequency", "snare", "typical", 0.8, "Rim"),
        ("Silent_Focus", "thud", "typical", 0.7, "Knock"),
        ("Void_Pulse", "snare", "long", 0.75, "Wash"),
        ("Silent_Focus", "hat", "typical", 0.5, "Tick"),
        ("Shadow_Ambient", "hat", "bright", 0.35, "Shaker"),
        ("Silent_Focus", "kick", "typical", 0.95, "Kick 2"),
        ("Void_Pulse", "snare", "short", 0.85, "Snare 2"),
        ("Raw_Frequency", "perc", "typical", 0.7, "Clack"),
    ],
    "session": [
        ("Modular_Code_Sessions_Vol__200", "kick", "typical", 0.95, "Kick"),
        ("Modular_Code_Sessions_Vol__200", "snare", "typical", 0.85, "Snare"),
        ("Modular_Code_Sessions_Vol__198", "snare", "typical", 0.8, "Snap"),
        ("Modular_Code_Sessions_Vol__198", "hat", "short", 0.4, "Hat"),
        ("Modular_Code_Sessions_Vol__198", "hat", "long", 0.4, "Open hat"),
        ("Modular_Code_Sessions_Vol__195", "thud", "typical", 0.75, "Low tom"),
        ("Modular_Code_Sessions_Vol__201", "thud", "typical", 0.75, "Mid tom"),
        ("Modular_Code_Sessions_Vol__200", "thud", "typical", 0.7, "Knock"),
        ("Modular_Code_Sessions_Vol__195", "snare", "short", 0.8, "Rim"),
        ("Modular_Code_Sessions_Vol__200", "perc", "typical", 0.7, "Blip"),
        ("Modular_Code_Sessions_Vol__195", "hat", "long", 0.4, "Wash"),
        ("Modular_Code_Sessions_Vol__198", "hat", "bright", 0.35, "Tick"),
        ("Modular_Code_Sessions_Vol__198", "hat", "dark", 0.4, "Shaker"),
        ("Modular_Code_Sessions_Vol__195", "kick", "typical", 0.95, "Kick 2"),
        ("Modular_Code_Sessions_Vol__201", "snare", "typical", 0.85, "Snare 2"),
        ("Modular_Code_Sessions_Vol__201", "kick", "long", 0.95, "Boom"),
    ],
}


def find_hits(y):
    onsets = librosa.onset.onset_detect(
        y=y, sr=SR, hop_length=256, units="samples", backtrack=True, delta=0.12
    )
    hits = []
    for k, o in enumerate(onsets):
        nxt = onsets[k + 1] if k + 1 < len(onsets) else len(y)
        seg = y[o : o + int(0.9 * SR)]
        if len(seg) < int(0.1 * SR):
            continue
        peak = np.abs(seg[: int(0.05 * SR)]).max()
        if peak < 0.15:
            continue
        pre = y[max(0, o - int(0.03 * SR)) : o]
        pre = float(np.sqrt((pre**2).mean() + 1e-12) / peak)
        a = seg[: int(0.08 * SR)]
        S = np.abs(np.fft.rfft(a * np.hanning(len(a)))) ** 2
        f = np.fft.rfftfreq(len(a), 1 / SR)
        t = S.sum() + 1e-12
        sh = [S[(f >= lo) & (f < hi)].sum() / t for lo, hi in ((0, 150), (150, 800), (800, 3000), (3000, 24000))]
        cen = float((S * f).sum() / t)
        sm = np.convolve(np.abs(seg), np.ones(480) / 480, "same")
        i = int(np.argmax(sm))
        below = np.where(sm[i:] < sm[i] * 0.0316)[0]
        dec = (i + below[0]) / SR if len(below) else 0.9
        if sh[0] > 0.55:
            cls = "kick"
        elif sh[3] > 0.45:
            cls = "hat"
        elif sh[1] + sh[2] > 0.5 and sh[3] > 0.04:
            cls = "snare"
        elif sh[1] > 0.5:
            cls = "thud"
        else:
            cls = "perc"
        hits.append(dict(o=int(o), gap=(nxt - o) / SR, peak=float(peak), pre=pre, cen=cen, dec=float(dec), cls=cls))
    return hits


def choose(hits, cls, which, used):
    pool = [h for h in hits if h["cls"] == cls and h["o"] not in used]
    if not pool:
        raise SystemExit(f"no {cls} hits")
    alone = [h for h in pool if h["pre"] < 0.05 and h["gap"] >= min(h["dec"] * 0.9, 0.6)]
    if len(alone) < 3:
        alone = sorted(pool, key=lambda h: h["pre"] - h["gap"])[: max(3, len(pool) // 4)]
    cen = np.log([h["cen"] for h in alone])
    dec = np.array([h["dec"] for h in alone])
    pk = np.array([h["peak"] for h in alone])
    q = {"typical": (50, 50), "long": (50, 88), "short": (50, 12), "bright": (88, 50), "dark": (12, 50)}[which]
    tc, td = np.percentile(cen, q[0]), np.percentile(dec, q[1])
    z = lambda v, t: (v - t) / (v.std() + 1e-9)
    score = z(cen, tc) ** 2 + z(dec, td) ** 2 + 0.25 * z(pk, np.percentile(pk, 75)) ** 2
    return alone[int(np.argmin(score))]


# closed sounds are kept tight whatever room rang after them
LIMIT = {"Hat": 0.16, "Tick": 0.12, "Shaker": 0.22, "Rim": 0.15}


def cut(y, h, peak, label):
    start = max(0, h["o"] - int(0.002 * SR))
    length = min(h["gap"] - 0.004, h["dec"] + 0.06, LIMIT.get(label, 0.85))
    seg = y[start : start + int(max(0.06, length) * SR)].astype(np.float64)
    seg = librosa.resample(seg, orig_sr=SR, target_sr=RATE)
    fade_in = int(0.001 * RATE)
    seg[:fade_in] *= np.linspace(0, 1, fade_in)
    fade = min(len(seg) // 3, int(0.03 * RATE))
    seg[-fade:] *= np.linspace(1, 0, fade) ** 2
    return seg / (np.abs(seg).max() + 1e-12) * peak


# name -> (stem, start seconds, length seconds, peak). The times come from
# listening with numbers: the longest steady single pitch with quiet before it
# (basses), the four seconds whose harmony moves least (airs). The pitch of
# each is in crates/engine/src/library.rs.
TONES = {
    "bass_shadow": ("Shadow_Ambient__bass", 85.54, 1.50, 0.42),
    "bass_silent": ("Silent_Focus__bass", 53.86, 1.47, 0.42),
    "air_shadow": ("Shadow_Ambient__other", 107.83, 4.0, 0.7),
    "air_silent": ("Silent_Focus__other", 59.91, 4.0, 0.7),
    # where one voice stands almost alone: the end of Shadow Ambient, the
    # opening of Silent Focus. Played as grains they are the tracks' own leads.
    "lead_shadow": ("Shadow_Ambient__other", 144.64, 2.6, 0.7),
    "lead_silent": ("Silent_Focus__other", 5.80, 4.0, 0.7),
}
TONE_RATE = 24000


def tones(folder):
    for name, (stem, start, length, peak) in TONES.items():
        y, sr = sf.read(os.path.join(folder, stem + ".wav"))
        assert sr == SR and y.ndim == 1, "stems must be 48 kHz mono"
        bass = name.startswith("bass")
        a = int((start - (0.03 if bass else 0.0)) * SR)
        seg = y[a : a + int((length + (0.08 if bass else 0.0)) * SR)].astype(np.float64)
        seg = librosa.resample(seg, orig_sr=SR, target_sr=TONE_RATE)
        fade_in = int((0.006 if bass else 0.05) * TONE_RATE)
        fade_out = int((0.07 if bass else 0.05) * TONE_RATE)
        seg[:fade_in] *= np.linspace(0, 1, fade_in)
        seg[-fade_out:] *= np.linspace(1, 0, fade_out)
        seg = seg / (np.abs(seg).max() + 1e-12) * peak
        with wave.open(os.path.join(OUT, name + ".wav"), "wb") as w:
            w.setnchannels(1)
            w.setsampwidth(2)
            w.setframerate(TONE_RATE)
            w.writeframes((np.clip(seg, -1, 1) * 32767).astype("<i2").tobytes())
        print(f"{name:13s} {stem:38s} at {start:7.2f}s  {len(seg) / TONE_RATE:.2f} s")


# chop kits: sixteen slices of each pad stem, cut where something new begins
CHOPS = {"shadowchops": "Shadow_Ambient__other", "silentchops": "Silent_Focus__other"}
CHOP_RATE = 22050


def chops(folder):
    for kit, stem in CHOPS.items():
        y, sr = sf.read(os.path.join(folder, stem + ".wav"))
        y = y.astype(np.float32)
        env = librosa.onset.onset_strength(y=y, sr=sr, hop_length=512)
        onsets = librosa.onset.onset_detect(onset_envelope=env, sr=sr, hop_length=512, backtrack=True, delta=0.15)
        level = librosa.feature.rms(y=y, frame_length=2048, hop_length=512)[0]
        # the strongest beginning in each sixteenth of the track, so the kit
        # walks through the whole piece
        picks = []
        edges = np.linspace(0, len(env), 17).astype(int)
        for a, b in zip(edges[:-1], edges[1:]):
            inside = [o for o in onsets if a <= o < b and level[min(o + 4, len(level) - 1)] > level.max() * 0.2]
            if inside:
                picks.append(max(inside, key=lambda o: env[min(o + 2, len(env) - 1)]))
        # a quiet stretch gives nothing: fill up with the strongest beginnings
        # left, none closer than two seconds to one already taken
        for o in sorted(onsets, key=lambda o: -env[min(o + 2, len(env) - 1)]):
            if len(picks) >= 16:
                break
            if all(abs(o - q) > int(2.0 * sr / 512) for q in picks):
                picks.append(o)
        picks.sort()
        for i in range(16):
            o = picks[i % len(picks)]
            later = [n for n in onsets if n > o + int(0.3 * sr / 512)]
            end = min(later[0] if later else o + 200, o + int(0.9 * sr / 512))
            seg = y[o * 512 : end * 512].astype(np.float64)
            seg = librosa.resample(seg, orig_sr=sr, target_sr=CHOP_RATE)
            seg[: int(0.004 * CHOP_RATE)] *= np.linspace(0, 1, int(0.004 * CHOP_RATE))
            fade = int(0.06 * CHOP_RATE)
            seg[-fade:] *= np.linspace(1, 0, fade)
            seg = seg / (np.abs(seg).max() + 1e-12) * 0.7
            with wave.open(os.path.join(OUT, f"{kit}_{i:02d}.wav"), "wb") as w:
                w.setnchannels(1)
                w.setsampwidth(2)
                w.setframerate(CHOP_RATE)
                w.writeframes((np.clip(seg, -1, 1) * 32767).astype("<i2").tobytes())
            print(f"{kit}_{i:02d} at {o * 512 / sr:7.2f}s  {len(seg) / CHOP_RATE * 1000:4.0f} ms")


def main():
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    cache = {}
    used = set()
    for kit, pads in KITS.items():
        for i, (stem, cls, which, peak, label) in enumerate(pads):
            if stem not in cache:
                y, sr = sf.read(os.path.join(sys.argv[1], stem + ".wav"))
                assert sr == SR and y.ndim == 1, "stems must be 48 kHz mono"
                cache[stem] = (y.astype(np.float32), find_hits(y.astype(np.float32)))
            y, hits = cache[stem]
            h = choose(hits, cls, which, used)
            used.add(h["o"])
            seg = cut(y, h, peak, label)
            path = os.path.join(OUT, f"{kit}_{i:02d}.wav")
            with wave.open(path, "wb") as w:
                w.setnchannels(1)
                w.setsampwidth(2)
                w.setframerate(RATE)
                w.writeframes((np.clip(seg, -1, 1) * 32767).astype("<i2").tobytes())
            print(
                f"{kit}_{i:02d} {label:11s} {stem[-14:]:14s} {cls:5s} {which:7s} at {h['o'] / SR:7.2f}s  "
                f"{len(seg) / RATE * 1000:4.0f} ms  centre {h['cen']:5.0f} Hz  before {h['pre']:.3f}"
            )
    tones(sys.argv[1])
    chops(sys.argv[1])


if __name__ == "__main__":
    main()
