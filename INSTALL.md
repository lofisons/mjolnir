# Install

Mjolnir has no account, no network use and no telemetry. Sets stay on your device.

None of the packages is signed with a paid developer certificate, so each system asks once whether you trust it. The steps below say how to answer.

## In a browser (any device)

Open the web app. To keep it like an app:

- **Chrome, Edge (desktop and Android)**: the install icon in the address bar, or the menu → *Install Mjolnir*.
- **Safari on iPhone and iPad**: Share → *Add to Home Screen*.
- **Safari on a Mac**: File → *Add to Dock*.

After the first visit it opens with no network.

To host it yourself, serve the `docs/` folder of this repository from any static host: it is the whole public site, with a front page and the app under `app/` (the `web` zip is the app alone). On GitHub: Settings → Pages → *Deploy from a branch* → `main`, `/docs`. For a domain of your own, put its name in `site/CNAME` and rebuild. The front page links its downloads to one address, `DOWNLOADS` at the bottom of `site/index.html`: by default the latest GitHub release, where the zips need to be uploaded.

## Linux

```bash
chmod +x Mjolnir-0.2.0-x86_64.AppImage
```

```bash
./Mjolnir-0.2.0-x86_64.AppImage
```

If it does not start for want of FUSE, run it with `--appimage-extract-and-run`.

## Windows

Run `Mjolnir-0.2.0-setup.exe`, or just start `Mjolnir-0.2.0-portable.exe`, which installs nothing. SmartScreen will say the publisher is unknown: *More info* → *Run anyway*.

## macOS

Open the disk image for your Mac (`arm64` for Apple silicon, `x64` for Intel) and drag Mjolnir to Applications. The first time, right-click the app → *Open* → *Open*. If macOS says the app is damaged, it is only the download's quarantine mark:

```bash
xattr -cr /Applications/Mjolnir.app
```

## Android

Open `Mjolnir-0.2.0.apk` on the phone and allow your file manager or browser to install apps when asked. It needs Android 8 or newer. Exports land in `Downloads/Mjolnir`.

## VST 3 (in a DAW)

Unzip `Mjolnir-0.2.0-vst3.zip` and copy the `Mjolnir.vst3` of your system to the plugin folder:

| System | Copy | To |
|---|---|---|
| Windows | `Windows\Mjolnir.vst3` | `C:\Program Files\Common Files\VST3\` |
| macOS | `macOS/Mjolnir.vst3` | `~/Library/Audio/Plug-Ins/VST3/` |
| Linux | `Linux/Mjolnir.vst3` | `~/.vst3/` |

Rescan plugins in the DAW; Mjolnir appears under instruments. On macOS, if the DAW refuses it, run once:

```bash
xattr -dr com.apple.quarantine ~/Library/Audio/Plug-Ins/VST3/Mjolnir.vst3
```

It has no window of its own: the DAW shows its parameters (Sound, Beat, the seven knobs, Swing, Tape, Vinyl, Warmth, Room size). Drum kits play from C1 (note 36) up. A Beat plays while the DAW's song plays, in its tempo.

## Build every package yourself

One script builds them all on a Linux machine, with no Mac, no Windows, no Android Studio and no CI service:

```bash
sh scripts/release.sh
```

It needs Rust with the `wasm32-unknown-unknown` target, Node 22 and pnpm, a JDK, python3, zip, xorriso, wine with 32-bit support and `rcodesign` (`cargo install apple-codesign`). The plugin also needs the Rust targets `x86_64-pc-windows-gnu`, `aarch64-apple-darwin` and `x86_64-apple-darwin`, mingw-w64, clang, lld and llvm-lipo. To build only some: `sh scripts/release.sh web linux vst3`.

The Android key is made on the first build and kept in `android/.keys`. Keep that folder: an update must be signed with the same key.

## What has been tried, and where

The web app and the desktop app are driven by the test suite in Chromium on Linux, with the real engine. The Windows, macOS and Android packages are built and inspected (signatures, manifests, contents) but were made on a machine that cannot run them: the first launch on each is yours.

The plugin is driven by a test host through the real VST 3 interfaces, on Linux and, under Wine, as the Windows build: every sound answers a note, a beat follows the transport, a state comes back. On Linux, Carla finds it and reads it as an instrument. It has not been opened in a commercial DAW, and the macOS build, linked without a Mac, has not been loaded at all.
