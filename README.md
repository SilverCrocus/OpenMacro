# OpenMacro

An open-source, local-first nutrition and macro tracker written in Rust, with an adaptive expenditure (TDEE) coach. It is inspired by apps such as MacroFactor.

## Status

Early development.

| Component | State |
|---|---|
| [`openmacro-core`](crates/openmacro-core) | Working and tested: nutrient model (55 nutrients), foods and servings, BMR equations, trend weight, adaptive expenditure (Kalman-style estimator with an uncertainty band), goal-driven calorie targets, weekly calorie shifting, macro splits |
| [`openmacro-app`](crates/openmacro-app) | Preview UI built with [Dioxus](https://dioxuslabs.com) (Rust, runs on web and Android). Includes the dashboard, a food-log timeline, a logging sheet (search, food detail, quick add, plate), and plan settings. Uses demo data and sample foods; nothing is saved yet. |
| Storage, food database import, check-in coach | Planned; see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |

## Docs

- [Feature specification](docs/FEATURES.md): what the app does, with priorities
- [Algorithms](docs/ALGORITHMS.md): trend weight, expenditure, targets and macros
- [Architecture](docs/ARCHITECTURE.md): crates, storage, food data, UI plan

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

## Clean-room policy

OpenMacro is an independent project. It is not affiliated with, endorsed by, or connected to MacroFactor or Stronger By Science.

- No proprietary source code, decompiled code, binaries, or assets (icons, images, copy, fonts) from other apps are committed to this repository.
- The specs in `docs/` describe *behavior* in our own words, and the implementation is written from those specs and public science.
- Reverse-engineering artifacts stay local in `research/private/`, which is git-ignored.

## License

MIT; see [LICENSE](LICENSE). Food data imported from Open Food Facts is licensed separately under the ODbL.
