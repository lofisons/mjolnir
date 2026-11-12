# Install

Mjolnir has no account, no network use and no telemetry. Sets stay on your device.

None of the packages is signed with a paid developer certificate, so each system asks once whether you trust it. The steps below say how to answer.

## In a browser (any device)

Open the web app. To keep it like an app:

- **Chrome, Edge (desktop and Android)**: the install icon in the address bar, or the menu → *Install Mjolnir*.
- **Safari on iPhone and iPad**: Share → *Add to Home Screen*.
- **Safari on a Mac**: File → *Add to Dock*.

After the first visit it opens with no network.

To host it yourself, serve the `docs/` folder of this repository (or the `web` zip) from any static host. On GitHub: Settings → Pages → *Deploy from a branch* → `main`, `/docs`.

## Linux

```bash
chmod +x Mjolnir-0.1.0-x86_64.AppImage
```

```bash
./Mjolnir-0.1.0-x86_64.AppImage
```

If it does not start for want of FUSE, run it with `--appimage-extract-and-run`.

## Windows

Run `Mjolnir-0.1.0-setup.exe`, or just start `Mjolnir-0.1.0-portable.exe`, which installs nothing. SmartScreen will say the publisher is unknown: *More info* → *Run anyway*.

## macOS

Open the disk image for your Mac (`arm64` for Apple silicon, `x64` for Intel) and drag Mjolnir to Applications. The first time, right-click the app → *Open* → *Open*. If macOS says the app is damaged, it is only the download's quarantine mark:

```bash
xattr -cr /Applications/Mjolnir.app
```

## Android

Open `Mjolnir-0.1.0.apk` on the phone and allow your file manager or browser to install apps when asked. It needs Android 8 or newer. Exports land in `Downloads/Mjolnir`.

## Build every package yourself

One script builds them all on a Linux machine, with no Mac, no Windows, no Android Studio and no CI service:

```bash
sh scripts/release.sh
```

It needs Rust with the `wasm32-unknown-unknown` target, Node 22 and pnpm, a JDK, python3, zip, xorriso, wine with 32-bit support and `rcodesign` (`cargo install apple-codesign`). To build only some: `sh scripts/release.sh web linux`.

The Android key is made on the first build and kept in `android/.keys`. Keep that folder: an update must be signed with the same key.

## What has been tried, and where

The web app and the desktop app are driven by the test suite in Chromium on Linux, with the real engine. The Windows, macOS and Android packages are built and inspected (signatures, manifests, contents) but were made on a machine that cannot run them: the first launch on each is yours.
