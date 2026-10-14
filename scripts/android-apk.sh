#!/usr/bin/env bash
# Builds the Android APK for one Rust target with OpenMacro's launcher icon.
#
# dx writes its default Dioxus icons into the Gradle project on every build
# and has no setting to change them, so this builds with dx, copies the
# icons from crates/openmacro-app/android/res over them, and repackages.
#
# Usage: scripts/android-apk.sh <rust target> [output.apk]
# e.g.   scripts/android-apk.sh aarch64-linux-android dist/unsigned-arm64-v8a.apk
# Needs ANDROID_HOME and ANDROID_NDK_HOME, like dx itself. Prints the APK path.
set -euo pipefail

target="${1:?usage: scripts/android-apk.sh <rust target> [output.apk]}"
out="${2:-}"
root="$(cd "$(dirname "$0")/.." && pwd)"
project="$root/target/dx/open-macro/release/android/app"

# dx reuses one Gradle project: clear the previous target's native library
# and APK (repackaging over an old APK leaves unused space in it).
rm -rf "$project/app/src/main/jniLibs" "$project/app/build/outputs/apk"

(cd "$root" && dx build --release --package openmacro-app --platform android \
    --target "$target" --features bundled-foods >&2)

cp -r "$root/crates/openmacro-app/android/res/." "$project/app/src/main/res/"
(cd "$project" && ./gradlew --quiet assembleDebug >&2)

apk="$project/app/build/outputs/apk/debug/app-debug.apk"
if [ -n "$out" ]; then
    mkdir -p "$(dirname "$out")"
    cp "$apk" "$out"
    apk="$out"
fi
echo "$apk"
