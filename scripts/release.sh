#!/bin/sh
# Build everything Mjolnir ships, on one Linux machine:
#
#   release/Mjolnir-<v>-web.zip            the web app (any static host, or installed from a browser)
#   release/Mjolnir-<v>-linux-x64.zip      an AppImage
#   release/Mjolnir-<v>-windows-x64.zip    an installer and a portable exe
#   release/Mjolnir-<v>-mac-arm64.zip      a disk image for Apple silicon
#   release/Mjolnir-<v>-mac-x64.zip        a disk image for Intel Macs
#   release/Mjolnir-<v>-android.zip        an APK
#   release/Mjolnir-<v>-vst3.zip           the plugin, for Windows, macOS and Linux
#
# and docs/, the public site: its front page, and the web app under app/.
#
#   sh scripts/release.sh            everything
#   sh scripts/release.sh web linux  only those (web, linux, windows, mac, android, vst3)
#
# Needs: Rust with the wasm32-unknown-unknown target, Node 22 and pnpm, a JDK,
# python3, zip, xorriso (the Mac disk image), wine with 32-bit support (the
# Windows installer runs its own uninstaller builder), and rcodesign
# (`cargo install apple-codesign`) to sign the Mac app so Apple silicon will
# run it. Nothing here needs a Mac, Windows, Android Studio or a CI service.
set -e
cd "$(dirname "$0")/.."
ROOT=$(pwd)
V=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
WANT=${*:-web linux windows mac android vst3}
OUT="$ROOT/release"
mkdir -p "$OUT"
has() { case " $WANT " in *" $1 "*) return 0 ;; *) return 1 ;; esac; }
pack() { (cd "$2" && shift 2 && rm -f "$OUT/$NAME" && zip -q -r -y "$OUT/$NAME" "$@"); echo "  $NAME"; }

echo "the web app"
pnpm install --silent
pnpm build >/dev/null
node tools/build_demos.mjs >/dev/null
pnpm exec vite build >/dev/null
cp LICENSE dist/LICENSE.txt
# the public site, kept in the repository so any static host (GitHub Pages
# from main, /docs) can serve it: the front page, and the app under app/
sh scripts/build-site.sh
if has web; then NAME="Mjolnir-$V-web.zip" pack x dist .; fi

if has linux || has windows || has mac; then
  echo "the desktop shell"
  (cd desktop && pnpm install --silent && node node_modules/electron/install.js)
  rm -rf desktop/app && cp -r dist desktop/app
  rm -f desktop/app/sw.js
fi

if has linux; then
  echo "linux"
  (cd desktop && pnpm exec electron-builder --linux AppImage --x64 >/dev/null)
  NAME="Mjolnir-$V-linux-x64.zip" pack x "$OUT/desktop" "Mjolnir-$V-x86_64.AppImage"
fi

if has windows; then
  echo "windows"
  (cd desktop && WINEDEBUG=-all pnpm exec electron-builder --win nsis portable --x64 >/dev/null)
  NAME="Mjolnir-$V-windows-x64.zip" pack x "$OUT/desktop" "Mjolnir-$V-setup.exe" "Mjolnir-$V-portable.exe"
fi

if has mac; then
  echo "mac"
  SIGN=${RCODESIGN:-rcodesign}
  for arch in arm64 x64; do
    (cd desktop && pnpm exec electron-packager . Mjolnir --platform=darwin --arch=$arch --out="$OUT/mac" --overwrite \
      --icon=build/icon.icns --app-bundle-id=com.lofisons.mjolnir --app-category-type=public.app-category.music \
      --extend-info=mac-info.plist --asar --ignore='^/node_modules' --ignore='^/after-pack.cjs' \
      --ignore='^/mac-info.plist' --ignore='^/\.npmrc' --ignore='^/pnpm-lock.yaml' >/dev/null 2>&1)
    # an ad-hoc signature: without one, Apple silicon refuses to start the app
    "$SIGN" sign "$OUT/mac/Mjolnir-darwin-$arch/Mjolnir.app" >/dev/null 2>&1
    rm -rf "$OUT/mac/dmg-$arch" && mkdir -p "$OUT/mac/dmg-$arch"
    cp -a "$OUT/mac/Mjolnir-darwin-$arch/Mjolnir.app" "$OUT/mac/dmg-$arch/"
    ln -s /Applications "$OUT/mac/dmg-$arch/Applications"
    xorriso -as mkisofs -V Mjolnir -r -hfsplus -o "$OUT/mac/Mjolnir-$V-mac-$arch.dmg" "$OUT/mac/dmg-$arch" >/dev/null 2>&1
    NAME="Mjolnir-$V-mac-$arch.zip" pack x "$OUT/mac" "Mjolnir-$V-mac-$arch.dmg"
  done
fi

if has android; then
  echo "android"
  sh android/build.sh >/dev/null 2>&1
  NAME="Mjolnir-$V-android.zip" pack x "$OUT/android" "Mjolnir-$V.apk"
fi

if has vst3; then
  sh scripts/build-vst3.sh
  rm -rf "$OUT/vst3/pack" && mkdir -p "$OUT/vst3/pack/Windows" "$OUT/vst3/pack/macOS" "$OUT/vst3/pack/Linux"
  cp -a "$OUT/vst3/windows/Mjolnir.vst3" "$OUT/vst3/pack/Windows/"
  cp -a "$OUT/vst3/mac/Mjolnir.vst3" "$OUT/vst3/pack/macOS/"
  cp -a "$OUT/vst3/linux/Mjolnir.vst3" "$OUT/vst3/pack/Linux/"
  cp crates/plugin/README.txt "$OUT/vst3/pack/README.txt"
  NAME="Mjolnir-$V-vst3.zip" pack x "$OUT/vst3/pack" README.txt Windows macOS Linux
fi

(cd "$OUT" && sha256sum Mjolnir-"$V"-*.zip > SHA256SUMS.txt)
ls -la "$OUT"/*.zip
