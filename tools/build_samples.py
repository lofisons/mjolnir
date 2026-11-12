#!/usr/bin/env python3
"""Build Yggdrasil's built-in sample banks from VS Chamber Orchestra: Community
Edition (VSCO-2 CE, CC0 1.0, https://github.com/sgossner/VSCO-2-CE).

Only the Dust drum kit is built from samples (the piano is procedural).
Only a handful of files are fetched (sparse, blob-less clone) and each one is
turned into a small lo-fi friendly asset: mono, 16-bit, 24 kHz, trimmed,
faded and peak-normalised. Output goes to crates/engine/assets/samples/.

Usage:  python3 tools/build_samples.py [path-to-existing-vsco-clone]
Needs:  git, ffmpeg, numpy
"""

import os
import subprocess
import sys
import tempfile
import wave

import numpy as np

REPO = "https://github.com/sgossner/VSCO-2-CE"
RATE = 24000
OUT = os.path.join(os.path.dirname(__file__), "..", "crates", "engine", "assets", "samples")


# Dust kit: pad order matches the drum pad map (36 = pad 0).
# name -> (source path, max seconds, gain trim dB, playback-rate change, decay seconds)
# Orchestral drums ring for a long time; `decay` imposes an exponential
# envelope (time to -60 dB) so each hit is tight enough for a beat, and every
# hit starts right on its transient.
KIT = {
    "kit_00_kick": ("VSCO 1 Percussion/drums/bass/bdrum_muted_pp_1.wav", 0.4, 0, 1.6, 0.35),
    "kit_01_snare": ("VSCO 1 Percussion/drums/snare/OldSnare/snare_mf.wav", 0.45, 0, 1.0, None),
    "kit_02_hand": ("VSCO 1 Percussion/drums/other/ethnic/giant/hand/EthnicLargeHand_hit_mp_1.wav", 0.3, 0, 1.0, 0.25),
    "kit_03_tamb": ("Percussion/Tamb1-Hit_v1_rr1_Sum.wav", 0.15, -12, 1.0, 0.12),
    "kit_04_shake": ("Percussion/Tamb1-Shake_v1_rr1_Sum.wav", 0.4, -11, 1.0, 0.4),
    "kit_05_tumba": ("Percussion/Tumba-HitN_v2_rr1_Sum.wav", 0.5, 0, 1.0, 0.45),
    "kit_06_conga": ("Percussion/Conga-HitN_v2_rr1_Sum.wav", 0.45, 0, 1.0, 0.4),
    "kit_07_quinto": ("Percussion/Quinto-HitN_v2_rr1_Sum.wav", 0.4, 0, 1.0, 0.35),
    "kit_08_rim": ("VSCO 1 Percussion/drums/snare/drum2/snare2_rimshot_f_1.wav", 0.25, -3, 1.0, 0.2),
    "kit_09_cowbell": ("Percussion/Cowbell1_Hit_v2_rr1_Sum.wav", 0.35, -6, 1.0, None),
    "kit_10_crash": ("Percussion/cymbal-crashshort_v1.wav", 1.5, -6, 1.0, None),
    "kit_11_ride": ("Percussion/susCymb1-hitstick_mp_rr1.wav", 1.2, -6, 1.0, None),
    "kit_12_guiro": ("Percussion/Guiro-Hit_v1_rr1_Sum.wav", 0.3, -3, 1.0, None),
    "kit_13_kick2": ("Percussion/BDrumNewhit_v3_rr1_Sum.wav", 0.5, 0, 1.6, 0.45),
    "kit_14_taps": ("Percussion/Snare2-taps_v3_rr1_Sum.wav", 0.3, -3, 1.0, 0.25),
    "kit_15_claves": ("Percussion/Claves1_Hit_v2_rr1_Sum.wav", 0.25, -6, 1.0, None),
}


def sh(*args, cwd=None):
    subprocess.run(args, cwd=cwd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def fetch(clone, paths):
    if not os.path.isdir(os.path.join(clone, ".git")):
        sh("git", "clone", "--filter=blob:none", "--no-checkout", "--depth", "1", REPO, clone)
    sh("git", "checkout", "HEAD", "--", *paths, cwd=clone)


# Bright metal/jingle hits get a gentle low-pass: dusty, not sizzly.
DARKEN = {"kit_03_tamb": 3800, "kit_04_shake": 3800, "kit_09_cowbell": 3500, "kit_10_crash": 3500, "kit_11_ride": 3500, "kit_15_claves": 4000, "kit_01_snare": 5500, "kit_12_guiro": 4500, "kit_08_rim": 5000, "kit_14_taps": 5000}
DEFAULT_LOWPASS = 6500  # everything else: dusty, like a sampler with a tired filter


def decode(path, rate, lowpass=None):
    """Any WAV -> mono float32 at `rate` (ffmpeg does the resampling)."""
    # two 2-pole stages = 24 dB/oct: a clearly dusty top end
    filters = ["-af", f"lowpass=f={lowpass}:p=2,lowpass=f={lowpass}:p=2"] if lowpass else []
    raw = subprocess.run(
        ["ffmpeg", "-v", "quiet", "-i", path, *filters, "-ac", "1", "-ar", str(rate), "-f", "f32le", "-"],
        check=True,
        capture_output=True,
    ).stdout
    return np.frombuffer(raw, dtype=np.float32).copy()


def process(x, max_seconds, gain_db=0.0, decay=None):
    x = x - np.mean(x)
    peak = np.max(np.abs(x)) or 1.0
    if decay:
        # start right before the loudest moment: drums hit immediately
        start = int(np.argmax(np.abs(x) > peak * 0.5))
        start = max(0, start - RATE // 300)
    else:
        # trim leading silence (keep 2 ms of pre-roll for the transient)
        start = int(np.argmax(np.abs(x) > peak * 0.02))
        start = max(0, start - RATE // 500)
    x = x[start : start + int(max_seconds * RATE)]
    if decay:
        t = np.arange(len(x)) / RATE
        x = x * np.exp(-6.9 * t / decay)
    # fade the end so long notes can be cut short without clicks
    fade = min(len(x) // 3, int(0.25 * RATE))
    x[-fade:] *= np.linspace(1.0, 0.0, fade) ** 2
    x[: RATE // 1000] *= np.linspace(0.0, 1.0, RATE // 1000)
    x = x / (np.max(np.abs(x)) or 1.0) * 0.89 * 10 ** (gain_db / 20)
    return x


def write(name, x):
    os.makedirs(OUT, exist_ok=True)
    pcm = np.clip(x * 32767, -32768, 32767).astype("<i2")
    with wave.open(os.path.join(OUT, name + ".wav"), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(pcm.tobytes())


def main():
    clone = sys.argv[1] if len(sys.argv) > 1 else os.path.join(tempfile.gettempdir(), "vsco-2-ce")
    sources = [k[0] for k in KIT.values()]
    fetch(clone, sources)
    for name, (path, secs, gain, speed, decay) in KIT.items():
        # speed > 1 shortens and raises the pitch (deep orchestral drums -> beat drums)
        x = decode(os.path.join(clone, path), int(RATE * speed), DARKEN.get(name, DEFAULT_LOWPASS))
        write(name, process(x, secs, gain, decay))
    total = sum(os.path.getsize(os.path.join(OUT, f)) for f in os.listdir(OUT))
    print(f"{len(KIT)} samples, {total / 1024:.0f} KiB in {os.path.normpath(OUT)}")


if __name__ == "__main__":
    main()
