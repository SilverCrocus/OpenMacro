# Development

How to build, test and release OpenMacro. For how to contribute, see [CONTRIBUTING.md](../CONTRIBUTING.md).

## Layout

| Crate | What it is |
|---|---|
| [`openmacro-core`](../crates/openmacro-core) | The maths and models, with no I/O. Covers 55 nutrients, foods and servings, recipes, BMR equations, trend weight, adaptive expenditure (a Kalman-style estimator with an uncertainty band), goal-driven targets, weekly calorie shifting, macro splits, check-ins, micronutrient reference targets and body measurements. |
| [`openmacro-store`](../crates/openmacro-store) | SQLite storage on the device, with schema migrations and whole-database snapshots for backups. |
| [`openmacro-foods`](../crates/openmacro-foods) | The food database: 58k foods from USDA, the Australian AFCD and Open Food Facts, with full-text search. See [FOOD_DATA.md](FOOD_DATA.md). |
| [`openmacro-ai`](../crates/openmacro-ai) | AI logging: builds each provider's request, reads its reply, and matches foods to the database. Also holds the `food-photo-eval` tool ([AI_PHOTO_EVAL.md](AI_PHOTO_EVAL.md)). |
| [`openmacro-app`](../crates/openmacro-app) | The [Dioxus](https://dioxuslabs.com) app: Android and a web preview. |

Design notes: [ARCHITECTURE.md](ARCHITECTURE.md), [ALGORITHMS.md](ALGORITHMS.md) and [FEATURES.md](FEATURES.md).

## Build and test

You need Rust (stable) and the Dioxus CLI 0.7 (`dx`). Install it with `cargo install dioxus-cli --version 0.7.10`, or download a prebuilt binary from the Dioxus releases page.

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
dx serve --package openmacro-app --platform web
```

The web preview runs in your browser and keeps data in memory only. It's the quickest way to work on the UI: open **More > Replace with demo data** for a filled-in account.

CI runs the same checks, plus a check of the web (wasm) build:

```bash
cargo check -p openmacro-app --target wasm32-unknown-unknown
```

## Android

Set `ANDROID_HOME`, `ANDROID_NDK_HOME` and `JAVA_HOME` (JDK 17+). The build needs Android SDK platform 35, which the Health Connect library compiles against:

```bash
sdkmanager "platforms;android-35"
```

Then build:

```bash
scripts/android-apk.sh aarch64-linux-android
```

It builds with `dx` in release mode with the bundled food database, so build `foods.db` first (see [FOOD_DATA.md](FOOD_DATA.md)). It then swaps in the OpenMacro launcher icon, which `dx` can't set itself, and prints the APK's path. Install the APK with `adb install -r <apk>`. Use `x86_64-linux-android` for the emulator.

- **Manifest and activity:** `crates/openmacro-app/android/` holds the Android manifest and `MainActivity.kt`, which replace the ones `dx` generates. `MainActivity.kt` has the Health Connect bridge and the app's HTTP bridge for AI logging.
- **Icon:** its source is `crates/openmacro-app/android/icon.png`. After changing it, run `python3 scripts/make-android-icons.py` (needs Pillow).

## Releasing

1. Bump `version` in `crates/openmacro-app/Cargo.toml` and commit.
2. Tag with release notes and push:

   ```bash
   git tag -a v0.6.0 -m "What changed in this release"
   git push origin v0.6.0
   ```

3. The [release workflow](../.github/workflows/release.yml) builds the food database, builds and signs the arm64 and x86_64 APKs, and publishes a GitHub Release. Installed apps find the release on their next launch and offer the update.

Releases are signed with the project's release key, which is stored in the repository secrets `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD` and `ANDROID_KEY_ALIAS`. Every update must be signed with the same key.
