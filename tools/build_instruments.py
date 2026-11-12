#!/usr/bin/env python3
"""Build Mjolnir's real instruments from VS Chamber Orchestra: Community
Edition (VSCO-2 CE, CC0 1.0, https://github.com/sgossner/VSCO-2-CE).

Each instrument is a handful of recorded notes ("zones") a few semitones
apart, darkened with a low-pass so it sits in slow, dusty music, tuned,
levelled and joined into one mono 16-bit 22.05 kHz file. Held instruments
(strings, flute, organ, bass) carry a loop: the steady part of each note,
cross-faded so it can turn for as long as a key is down. A small JSON file
beside each one says where every zone starts, its pitch, and its loop.

Usage:  python3 tools/build_instruments.py [path-to-existing-vsco-clone]
Needs:  git, numpy, scipy, librosa, soundfile
"""

import json
import os
import subprocess
import sys
import tempfile
import warnings
import wave

warnings.filterwarnings("ignore")
import librosa
import numpy as np
import soundfile as sf
from scipy.signal import butter, sosfiltfilt

REPO = "https://github.com/sgossner/VSCO-2-CE"
RATE = 22050
OUT = os.path.join(os.path.dirname(__file__), "..", "crates", "engine", "assets", "samples")

NOTE = {"C": 0, "D": 2, "E": 4, "F": 5, "G": 7, "A": 9, "B": 11}


def midi(name, middle_c=4):
    """'F#3' -> 54 when middle C is C4. Most of this library's file names call
    middle C "C3" (pass middle_c=3); the harp's call it C4."""
    pc = NOTE[name[0]] + (1 if "#" in name else 0)
    return pc + 12 * (int(name[-1]) + 1 + (4 - middle_c))


def c3(name):
    return midi(name, 3)


def piano(i):
    return f"Keys/Upright Piano/Player_dyn2_rr1_{i:03d}.wav", 21 + 2 * i


# name -> settings and zones (file, MIDI note; None = measure it).
#   seconds: how much of each note is kept      cutoff: the darkening low-pass
#   loop:    seconds of steady tone to loop (0 = the note just rings out)
#   attack / release: the envelope the sampler plays it with
INSTRUMENTS = {
    "inst_piano": dict(
        seconds=3.0, cutoff=3400, loop=0, attack=0.002, release=0.45, gain=0.6,
        zones=[piano(i) for i in (10, 14, 18, 22, 26, 30)],
    ),
    "inst_musicbox": dict(
        seconds=2.2, cutoff=7000, loop=0, attack=0.001, release=0.6, gain=0.7,
        zones=[(f"Percussion/Glock/glock_medium_{n}.wav", c3(n)) for n in ("G4", "C5", "G5", "C6")],
    ),
    "inst_harp": dict(
        seconds=2.4, cutoff=3800, loop=0, attack=0.002, release=0.5, gain=0.9,
        zones=[(f"Strings/Harp/KSHarp_{n}_mf.wav", midi(n)) for n in ("A2", "E3", "B3", "F4", "C5", "G5")],
    ),
    "inst_strings": dict(
        seconds=0.5, cutoff=4200, loop=1.7, attack=0.14, release=0.7, gain=0.8,
        zones=[
            ("Strings/Cello Section/susvib/susvib_C1_v1_1.wav", c3("C1")),
            ("Strings/Cello Section/susvib/susvib_G1_v1_1.wav", c3("G1")),
            ("Strings/Cello Section/susvib/susvib_D2_v1_1.wav", c3("D2")),
            ("Strings/Cello Section/susvib/susvib_A2_v1_1.wav", c3("A2")),
            ("Strings/Cello Section/susvib/susvib_E3_v1_1.wav", c3("E3")),
            ("Strings/Viola Section/susvib/ViolaEns_susvib_C4_v1_1.wav", c3("C4")),
            ("Strings/Viola Section/susvib/ViolaEns_susvib_G4_v1_1.wav", c3("G4")),
        ],
    ),
    "inst_flute": dict(
        seconds=0.4, cutoff=5200, loop=1.4, attack=0.05, release=0.3, gain=0.6,
        zones=[(f"Woodwinds/Flute/susNV/LDFlute_susNV_{n}_v1_1.wav", c3(n)) for n in ("C3", "E3", "A3", "E4", "A4")],
    ),
    "inst_organ": dict(
        seconds=0.4, cutoff=3600, loop=1.6, attack=0.08, release=0.5, gain=0.7,
        # the organ's files are numbered 86 above their MIDI note
        zones=[(f"Keys/Organ/Quiet/NT5_Man3Quiet_{n}_rr1.wav", n - 86) for n in (125, 137, 149, 161, 173)],
    ),
    "inst_contrabass": dict(
        seconds=0.4, cutoff=2200, loop=1.4, attack=0.03, release=0.3, gain=0.9,
        zones=[
            ("Strings/Solo Contrabass/SusNV/BKCtbss_SusNV_E0_v1_rr1.wav", c3("E0")),
            ("Strings/Solo Contrabass/SusNV/BKCtbss_SusNV_A#0_v1_rr1.wav", c3("A#0")),
            ("Strings/Solo Contrabass/SusNV/BKCtbss_SusNV_E1_v1_rr1.wav", c3("E1")),
            ("Strings/Solo Contrabass/SusNV/BKCtbss_SusNV_A1_v1_rr1.wav", c3("A1")),
        ],
    ),
}


def fetch(clone, paths):
    if not os.path.isdir(os.path.join(clone, ".git")):
        subprocess.run(["git", "clone", "--filter=blob:none", "--no-checkout", "--depth", "1", REPO, clone], check=True)
    subprocess.run(["git", "-C", clone, "config", "gc.auto", "0"], check=True)
    missing = [p for p in paths if not os.path.exists(os.path.join(clone, p))]
    for i in range(0, len(missing), 12):
        subprocess.run(["git", "-C", clone, "checkout", "HEAD", "--"] + missing[i : i + 12], check=True)


def measure(y, sr):
    """The note a recording holds, from its steady middle."""
    mid = y[int(0.3 * sr) : int(1.5 * sr)]
    f0, _, prob = librosa.pyin(mid, fmin=30, fmax=2200, sr=sr, frame_length=4096)
    ok = ~np.isnan(f0) & (prob > 0.5)
    return float(np.median(librosa.hz_to_midi(f0[ok]))) if ok.sum() > 3 else None


def zone(path, root, cfg):
    y, sr = sf.read(path, always_2d=True)
    y = y.mean(axis=1)
    # start right where the note does
    level = np.abs(y)
    first = int(np.argmax(level > level.max() * 0.02))
    y = y[max(0, first - int(0.003 * sr)) :]
    held = cfg["loop"] > 0
    heard = measure(y, sr)
    if root is None:
        root = int(round(heard))
    # how far off its note the recording sits, whatever octave the ear landed on
    off = 0.0 if heard is None else ((heard - root + 0.5) % 1.0 - 0.5) * 100
    cents = off if abs(off) < 40 else 0.0
    need = cfg["seconds"] + cfg["loop"] + (0.4 if held else 0.0)
    y = y[: int(need * sr * 1.1)]
    # tune (a recording a few cents off is pulled onto its note), then the house rate
    y = librosa.resample(y, orig_sr=sr * 2 ** (cents / 1200), target_sr=RATE)
    y = sosfiltfilt(butter(2, cfg["cutoff"], btype="low", fs=RATE, output="sos"), y)
    loop_at = None
    if held:
        a = int(cfg["seconds"] * RATE)
        b = a + int(cfg["loop"] * RATE)
        x = int(0.3 * RATE)
        y = y[:b].copy()
        # the end of the loop melts into what came just before its start
        w = np.linspace(0, 1, x)
        y[b - x : b] = y[b - x : b] * np.sqrt(1 - w) + y[a - x : a] * np.sqrt(w)
        # keep the loop's level even from start to end
        loop_at = a
        y = y / (np.sqrt((y[a:b] ** 2).mean()) + 1e-9) * 0.2
        peak = np.abs(y).max()
        if peak > 0.9:
            y *= 0.9 / peak
    else:
        y = y[: int(cfg["seconds"] * RATE)].copy()
        fade = int(0.25 * RATE)
        y[-fade:] *= np.linspace(1, 0, fade) ** 2
        y = y / (np.abs(y).max() + 1e-9) * 0.8
    y[: int(0.002 * RATE)] *= np.linspace(0, 1, int(0.002 * RATE))
    return y, root, loop_at, cents


def main():
    clone = sys.argv[1] if len(sys.argv) > 1 else os.path.join(tempfile.gettempdir(), "vsco-2-ce")
    fetch(clone, [p for cfg in INSTRUMENTS.values() for p, _ in cfg["zones"]])
    for name, cfg in INSTRUMENTS.items():
        frames, zones = [], []
        for path, root in cfg["zones"]:
            y, root, loop_at, cents = zone(os.path.join(clone, path), root, cfg)
            start = sum(len(f) for f in frames)
            zones.append(
                dict(root=root, start=start, end=start + len(y), loop=None if loop_at is None else start + loop_at)
            )
            frames.append(y)
            frames.append(np.zeros(8))
            print(f"{name:16s} {os.path.basename(path):42s} note {root:3d}  tuned {cents:+5.0f} cents  {len(y) / RATE:.2f} s")
        all_frames = np.concatenate(frames)
        with wave.open(os.path.join(OUT, name + ".wav"), "wb") as w:
            w.setnchannels(1)
            w.setsampwidth(2)
            w.setframerate(RATE)
            w.writeframes((np.clip(all_frames, -1, 1) * 32767).astype("<i2").tobytes())
        meta = dict(attack=cfg["attack"], release=cfg["release"], gain=cfg["gain"], zones=zones)
        with open(os.path.join(OUT, name + ".json"), "w") as f:
            json.dump(meta, f, indent=1)
            f.write("\n")


if __name__ == "__main__":
    main()
