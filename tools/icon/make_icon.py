# SPDX-License-Identifier: MIT
"""The lofisons app icon: two letters in Geist Serif on a VSGARD plate.

Every app in the family draws its icon with this same script; only the letters
change. The plate is ink (#0b0b0a) with square corners and a hairline frame at
35 % of the paper, so it holds its edge on a dark taskbar. The letters sit at
the top left in paper (#ededea). Under them, a ruler: one thick stroke and four
fine ticks. Below 64 px the ticks would turn to mush, so only the stroke and
the frame remain; each size is drawn on its own, never shrunk from the big one.

    python3 tools/icon/make_icon.py                 # this app: Yg, into tools/icon/out/ (the app's own set: tools/build_icon.py)
    python3 tools/icon/make_icon.py Ab out/         # other letters, other folder
    python3 tools/icon/make_icon.py Ab out/ --raw   # also coverage-NN.bin, for code without image crates

It writes icon.png (1024), NxN.png, icon.ico (16 to 256, each size drawn on its own) and icon.icns.

Needs Pillow and fonts/GeistSerif-Regular.ttf next to this script (SIL OFL 1.1,
from Vercel's `geist` package).
"""
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

INK = (11, 11, 10)
PAPER = (237, 237, 234)
HERE = Path(__file__).resolve().parent
FONT = HERE / "fonts" / "GeistSerif-Regular.ttf"
LETTERS = "Yg"
OUT = HERE / "out"
ICO_SIZES = (16, 20, 24, 32, 40, 48, 64, 128, 256)
PNG_SIZES = (16, 32, 48, 64, 128, 256, 512)
RAW_SIZES = (16, 24, 32, 48, 256)
SUPERSAMPLE = 4


def coverage(letters, size):
    """How much paper each pixel holds, 0..255, as an 8-bit image."""
    n = size * SUPERSAMPLE
    img = Image.new("L", (n, n), 0)
    d = ImageDraw.Draw(img)
    u = n / 1024
    small = size < 64
    # the frame: a hairline at 35 % (45 % when small, so it survives)
    hair = SUPERSAMPLE if small else max(SUPERSAMPLE, round(6 * u))
    inset = 0 if size < 48 else round(40 * u)
    d.rectangle([inset, inset, n - 1 - inset, n - 1 - inset], outline=round(255 * (0.45 if small else 0.35)), width=hair)
    # the letters, top left; the cap height sets the size
    pad = round(150 * u)
    font = ImageFont.truetype(str(FONT), round(560 * u))
    cap_top = font.getbbox("H")[1]
    d.text((pad - font.getbbox(letters)[0], pad - cap_top + round(10 * u)), letters, font=font, fill=255)
    # the ruler: a thick stroke and, when there is room, four fine ticks
    y = n - pad
    x1 = pad + round(380 * u)
    d.rectangle([pad, y - max(SUPERSAMPLE, round(22 * u)), x1, y], fill=255)
    if not small:
        tick = max(SUPERSAMPLE, round(6 * u))
        for i in range(1, 5):
            x = x1 + i * round(76 * u)
            d.rectangle([x - tick // 2, y - round(44 * u), x + tick // 2, y], fill=round(255 * 0.45))
    return img.resize((size, size), Image.LANCZOS)


def render(letters, size):
    """The icon at one size, RGBA, ink plate edge to edge."""
    cov = coverage(letters, size)
    return Image.composite(Image.new("RGBA", (size, size), PAPER + (255,)),
                           Image.new("RGBA", (size, size), INK + (255,)), cov)


def write(letters, out, raw=False):
    out = Path(out)
    out.mkdir(parents=True, exist_ok=True)
    big = render(letters, 1024)
    big.save(out / "icon.png")
    for s in PNG_SIZES:
        render(letters, s).save(out / f"{s}x{s}.png")
    icos = [render(letters, s) for s in ICO_SIZES]
    icos[-1].save(out / "icon.ico", sizes=[(s, s) for s in ICO_SIZES], append_images=icos[:-1])
    big.save(out / "icon.icns")
    if raw:
        for s in RAW_SIZES:
            (out / f"coverage-{s}.bin").write_bytes(coverage(letters, s).tobytes())
    return out


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if a != "--raw"]
    letters = args[0] if args else LETTERS
    out = args[1] if len(args) > 1 else OUT
    print(write(letters, out, raw="--raw" in sys.argv))
