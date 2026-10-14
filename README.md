# OpenMacro

An open-source, local-first nutrition and macro tracker written in Rust, with an adaptive expenditure (TDEE) coach. It is inspired by apps such as MacroFactor.

## Status

Early development.

| Component | State |
|---|---|
| [`openmacro-core`](crates/openmacro-core) | Working and tested: nutrient model (55 nutrients), foods and servings, BMR equations, trend weight, adaptive expenditure (Kalman-style estimator with an uncertainty band), goal-driven calorie targets, weekly calorie shifting, macro splits |
| [`openmacro-store`](crates/openmacro-store) | SQLite storage for the food log, weigh-ins, day status and settings, on the device |
| [`openmacro-foods`](crates/openmacro-foods) | Food database: 62k foods from USDA, the Australian AFCD and Open Food Facts (AU + US), with full-text search; see [docs/FOOD_DATA.md](docs/FOOD_DATA.md) |
| [`openmacro-app`](crates/openmacro-app) | Dioxus app (Rust; Android and web preview) with a dashboard, food log timeline, logging sheet (search, number pad, quick add, plate) and plan settings. Data is saved on the device. |
| Barcode scanning, check-in coach, Health Connect | Planned; see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |

## Docs

- [Feature specification](docs/FEATURES.md): what the app does, with priorities
- [Algorithms](docs/ALGORITHMS.md): trend weight, expenditure, targets and macros
- [Architecture](docs/ARCHITECTURE.md): crates, storage, food data, UI plan
- [Food data](docs/FOOD_DATA.md): sources, licences and how to build `foods.db`
- [Roadmap](docs/ROADMAP.md): what's done and what's next

## Development

Requires Rust (stable) and the Dioxus CLI 0.7 (`dx`). Install `dx` with `cargo install dioxus-cli --version 0.7.10`, or download a prebuilt binary from the Dioxus releases page.

```bash
cargo test
cargo clippy --all-targets -- -D warnings
dx serve --package openmacro-app --platform web
```

To build for Android, set `ANDROID_HOME`, `ANDROID_NDK_HOME` and `JAVA_HOME` (JDK 17+), then run:

```bash
dx build --package openmacro-app --platform android --target aarch64-linux-android
```

The APK lands in `target/dx/open-macro/debug/android/app/app/build/outputs/apk/debug/`. Install it with `adb install -r <apk>`.

## Clean-room policy

OpenMacro is an independent project. It is not affiliated with, endorsed by, or connected to MacroFactor or Stronger By Science.

- No proprietary source code, decompiled code, binaries, or assets (icons, images, copy, fonts) from other apps are committed to this repository.
- The specs in `docs/` describe *behavior* in our own words, and the implementation is written from those specs and public science.
- Reverse-engineering artifacts stay local in `research/private/`, which is git-ignored.

## License

Code: MIT; see [LICENSE](LICENSE). The food database has its own licences (USDA public domain, FSANZ CC BY 4.0, Open Food Facts ODbL); see [docs/FOOD_DATA.md](docs/FOOD_DATA.md).
