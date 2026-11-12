#!/bin/sh
# Build Mjolnir's Android package by hand: no Android Studio, no Gradle, and
# nothing from Google's download servers. The four tools it needs come from
# Maven Central: aapt2 and the framework's resources (inside apktool-lib), the
# platform's classes (android-all), the dex compiler (dalvik-dx) and the
# signer (apksig).
#
#   sh android/build.sh            build release/android/Mjolnir-<version>.apk
#
# Needs: a JDK (17 or newer), python3, curl, unzip, zip, and the web app built in dist/.
# The signing key is made on the first build and kept in android/.keys: every
# later build must use the same one, or Android will refuse to update the app.
set -e
cd "$(dirname "$0")"
ROOT=$(cd .. && pwd)
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)
CODE=${VERSION_CODE:-1}
TOOLS=${MJOLNIR_ANDROID_TOOLS:-$HOME/.cache/mjolnir-android}
OUT="$ROOT/release/android"
WORK="$ROOT/target/android"
CENTRAL=https://repo1.maven.org/maven2

[ -f "$ROOT/dist/index.html" ] || { echo "build the web app first: pnpm build"; exit 1; }
mkdir -p "$TOOLS" "$OUT"
rm -rf "$WORK" && mkdir -p "$WORK/classes" "$WORK/gen" "$WORK/assets/app"

fetch() { [ -f "$TOOLS/$1" ] || curl -sSfL -o "$TOOLS/$1" "$CENTRAL/$2"; }
fetch apktool-lib.jar org/apktool/apktool-lib/2.9.3/apktool-lib-2.9.3.jar
fetch android-all.jar org/robolectric/android-all/14-robolectric-10818077/android-all-14-robolectric-10818077.jar
fetch dx.jar com/jakewharton/android/repackaged/dalvik-dx/16.0.1/dalvik-dx-16.0.1.jar
fetch apksig.jar com/android/tools/build/apksig/2.3.0/apksig-2.3.0.jar
if [ ! -x "$TOOLS/aapt2" ]; then
  unzip -o -q -j "$TOOLS/apktool-lib.jar" prebuilt/linux/aapt2_64 brut/androlib/android-framework.jar -d "$TOOLS"
  mv "$TOOLS/aapt2_64" "$TOOLS/aapt2" && chmod +x "$TOOLS/aapt2"
fi

# the web app, as the package's assets (no service worker: nothing to fetch)
cp -r "$ROOT/dist/." "$WORK/assets/app/"
rm -f "$WORK/assets/app/sw.js"

"$TOOLS/aapt2" compile --dir res -o "$WORK/res.zip"
"$TOOLS/aapt2" link -o "$WORK/base.apk" -I "$TOOLS/android-framework.jar" \
  --manifest AndroidManifest.xml -A "$WORK/assets" --java "$WORK/gen" \
  --min-sdk-version 26 --target-sdk-version 34 \
  --version-code "$CODE" --version-name "$VERSION" "$WORK/res.zip"

javac -nowarn --release 8 -cp "$TOOLS/android-all.jar" -d "$WORK/classes" \
  $(find src "$WORK/gen" -name '*.java') 2>&1 | grep -v "^warning\|^Note:\|^1 warning\|^[0-9]* warnings" || true
[ -f "$WORK/classes/com/lofisons/mjolnir/MainActivity.class" ] || { echo "the Java sources did not compile"; exit 1; }
java -cp "$TOOLS/dx.jar" com.android.dx.command.Main --dex --min-sdk-version=26 \
  --output="$WORK/classes.dex" "$WORK/classes"
(cd "$WORK" && zip -q -j base.apk classes.dex)
python3 tools/zipalign.py "$WORK/base.apk" "$WORK/aligned.apk"

# the key that says every build of Mjolnir comes from the same hands
KEYS="$ROOT/android/.keys"
if [ ! -f "$KEYS/mjolnir.p12" ]; then
  mkdir -p "$KEYS"
  keytool -genkeypair -keystore "$KEYS/mjolnir.p12" -storetype PKCS12 -storepass mjolnir -keypass mjolnir \
    -alias mjolnir -keyalg RSA -keysize 2048 -validity 12000 -dname "CN=Mjolnir, O=lofisons" 2>/dev/null
fi
javac -nowarn -cp "$TOOLS/apksig.jar" -d "$WORK/sign" tools/Sign.java
# this apksig reaches into the JDK's own certificate classes
java --add-exports java.base/sun.security.x509=ALL-UNNAMED \
  -cp "$TOOLS/apksig.jar:$WORK/sign" Sign "$KEYS/mjolnir.p12" mjolnir mjolnir \
  "$WORK/aligned.apk" "$OUT/Mjolnir-$VERSION.apk"
ls -la "$OUT/Mjolnir-$VERSION.apk"
