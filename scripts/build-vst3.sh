#!/bin/sh
# Build the VST 3 plugin for every desktop, on one Linux machine:
#
#   release/vst3/linux/Mjolnir.vst3     x86_64
#   release/vst3/windows/Mjolnir.vst3   x86_64
#   release/vst3/mac/Mjolnir.vst3       Apple silicon and Intel in one
#
#   sh scripts/build-vst3.sh              all three
#   sh scripts/build-vst3.sh linux mac    only those
#
# Needs Rust with the targets x86_64-pc-windows-gnu, aarch64-apple-darwin and
# x86_64-apple-darwin; mingw-w64 (Windows); clang, lld and llvm-lipo (Mac), and
# rcodesign to sign the Mac bundle. The Mac build links against a hand-written
# list of the system's symbols (crates/plugin/mac/libSystem.tbd), so it needs
# no Mac and no Apple SDK.
set -e
cd "$(dirname "$0")/.."
ROOT=$(pwd)
V=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
WANT=${*:-linux windows mac}
OUT="$ROOT/release/vst3"
has() { case " $WANT " in *" $1 "*) return 0 ;; *) return 1 ;; esac; }
tool() { for t in "$@"; do if command -v "$t" >/dev/null 2>&1; then command -v "$t"; return; fi; done; echo "missing: $1" >&2; exit 1; }

if has linux; then
  echo "vst3 linux"
  cargo build --release -p mjolnir-plugin >/dev/null 2>&1
  B="$OUT/linux/Mjolnir.vst3/Contents/x86_64-linux"
  rm -rf "$OUT/linux" && mkdir -p "$B"
  cp target/release/libmjolnir_vst3.so "$B/Mjolnir.so"
fi

if has windows; then
  echo "vst3 windows"
  CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
    cargo build --release -p mjolnir-plugin --target x86_64-pc-windows-gnu >/dev/null 2>&1
  B="$OUT/windows/Mjolnir.vst3/Contents/x86_64-win"
  rm -rf "$OUT/windows" && mkdir -p "$B"
  cp target/x86_64-pc-windows-gnu/release/mjolnir_vst3.dll "$B/Mjolnir.vst3"
fi

if has mac; then
  echo "vst3 mac"
  LLD=$(tool ld64.lld lld lld-18)
  LIPO=$(tool llvm-lipo llvm-lipo-18 lipo)
  SIGN=${RCODESIGN:-rcodesign}
  # a private corner for the linker: its name for Mach-O, and the system stub
  # under every name the compiler asks for
  T="$ROOT/target/mac-link"
  rm -rf "$T" && mkdir -p "$T/bin" "$T/lib"
  ln -s "$(readlink -f "$LLD")" "$T/bin/ld64.lld"
  for l in System c m iconv objc resolv; do cp crates/plugin/mac/libSystem.tbd "$T/lib/lib$l.tbd"; done
  cat > "$T/bin/cc" <<CC
#!/bin/sh
exec clang --target=\$MJ_MAC_TARGET -fuse-ld=lld -B"$T/bin" -L"$T/lib" "\$@"
CC
  chmod +x "$T/bin/cc"
  PATH="$T/bin:$PATH" MJ_MAC_TARGET=arm64-apple-macos11 CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER="$T/bin/cc" \
    cargo build --release -p mjolnir-plugin --target aarch64-apple-darwin >/dev/null 2>&1
  PATH="$T/bin:$PATH" MJ_MAC_TARGET=x86_64-apple-macos10.13 CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER="$T/bin/cc" \
    cargo build --release -p mjolnir-plugin --target x86_64-apple-darwin >/dev/null 2>&1
  B="$OUT/mac/Mjolnir.vst3/Contents"
  rm -rf "$OUT/mac" && mkdir -p "$B/MacOS"
  "$LIPO" -create -output "$B/MacOS/Mjolnir" \
    target/aarch64-apple-darwin/release/libmjolnir_vst3.dylib \
    target/x86_64-apple-darwin/release/libmjolnir_vst3.dylib
  printf 'BNDL????' > "$B/PkgInfo"
  sed "s/@VERSION@/$V/g" crates/plugin/mac/Info.plist > "$B/Info.plist"
  # an ad-hoc signature: without one, Apple silicon refuses to load the plugin
  "$SIGN" sign "$OUT/mac/Mjolnir.vst3" >/dev/null 2>&1
fi
