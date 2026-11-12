#!/usr/bin/env python3
"""Draw Mjolnir's app icon: "Mj" in Geist Serif on the VSGARD plate, the same
icon as every lofisons app (tools/icon/make_icon.py draws it).

Writes the icons of the web app (favicon, home-screen and maskable icons),
of the desktop apps (desktop/build) and of the Android app (android/res).

Usage:  python3 tools/build_icon.py
Needs:  Pillow
"""

import os
import sys

from PIL import Image

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "icon"))
from make_icon import ICO_SIZES, render  # noqa: E402

ROOT = os.path.join(os.path.dirname(__file__), "..")
WEB = os.path.join(ROOT, "ui", "public")
DESKTOP = os.path.join(ROOT, "desktop", "build")
ANDROID = os.path.join(ROOT, "android", "res")
LETTERS = "Mj"
INK = (11, 11, 10, 255)


def padded(size, inner):
    """The icon with ink around it, for launchers that crop to a circle."""
    plate = Image.new("RGBA", (size, size), INK)
    icon = render(LETTERS, inner)
    plate.paste(icon, ((size - inner) // 2, (size - inner) // 2))
    return plate


def main():
    for d in (WEB, DESKTOP):
        os.makedirs(d, exist_ok=True)
    render(LETTERS, 32).save(os.path.join(WEB, "favicon.png"))
    render(LETTERS, 180).save(os.path.join(WEB, "apple-touch-icon.png"))
    render(LETTERS, 192).save(os.path.join(WEB, "icon-192.png"))
    render(LETTERS, 512).save(os.path.join(WEB, "icon-512.png"))
    padded(512, 384).save(os.path.join(WEB, "icon-maskable-512.png"))

    big = render(LETTERS, 1024)
    big.save(os.path.join(DESKTOP, "icon.png"))
    icos = [render(LETTERS, s) for s in ICO_SIZES]
    icos[-1].save(os.path.join(DESKTOP, "icon.ico"), sizes=[(s, s) for s in ICO_SIZES], append_images=icos[:-1])
    big.save(os.path.join(DESKTOP, "icon.icns"))

    for name, size in [("mdpi", 48), ("hdpi", 72), ("xhdpi", 96), ("xxhdpi", 144), ("xxxhdpi", 192)]:
        d = os.path.join(ANDROID, f"mipmap-{name}")
        os.makedirs(d, exist_ok=True)
        padded(size, int(size * 0.78)).save(os.path.join(d, "ic_launcher.png"))


if __name__ == "__main__":
    main()
