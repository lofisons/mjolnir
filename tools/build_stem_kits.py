#!/usr/bin/env python3
"""Build the Shadow and Session drum kits from the author's own drum stems.

The stems are lofisons tracks (drums-only mixes). Every hit is found, sorted
into kick / snare / thud / hat / perc by where its energy sits, and for each
pad the most typical hit that stands alone (nothing ringing before it, nothing
landing on its tail) is cut out: mono, 16-bit, 32 kHz, faded, levelled.

Usage:  python3 tools/build_stem_kits.py <folder of drum stems as 48 kHz mono wav>
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


if __name__ == "__main__":
    main()
