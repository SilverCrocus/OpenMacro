# OpenMacro

An open-source, local-first nutrition and macro tracker written in Rust, with an adaptive expenditure (TDEE) coach. It is inspired by apps such as MacroFactor.

## Status

Early development.

| Component | State |
|---|---|
| [`openmacro-core`](crates/openmacro-core) | Working and tested: nutrient model (55 nutrients), foods and servings, recipes, BMR equations, trend weight, adaptive expenditure (Kalman-style estimator with an uncertainty band), goal-driven targets with maintenance, weekly calorie shifting, macro splits, weekly check-ins, micronutrient reference targets, body measurements |
| [`openmacro-store`](crates/openmacro-store) | SQLite storage on the device: food log, weigh-ins, day status, custom foods and recipes, favorites, check-ins, measurements and settings, with whole-database snapshots for backups |
| [`openmacro-foods`](crates/openmacro-foods) | Food database: 58k foods from USDA, the Australian AFCD and Open Food Facts (AU + US), with full-text search; see [docs/FOOD_DATA.md](docs/FOOD_DATA.md) |
| [`openmacro-app`](crates/openmacro-app) | Dioxus app (Rust; Android and web preview):<ul><li>**Logging:** food log timeline with copy and edit; logging sheet with search, barcode scanning, quick add, custom foods, recipes and favorites; water.</li><li>**Coaching:** plan with coached or manual programs and Lose/Maintain/Gain goals; weekly check-ins.</li><li>**Insights:** pages for expenditure, weight, energy balance, macros, goal progress, nutrients and body measurements.</li><li>**Data and settings:** CSV export and backups; kJ/lb units and light theme.</li></ul>Data is saved on the device, and the app updates itself from GitHub Releases. |
| AI photo logging, progress photos, Health Connect | Planned; see [docs/ROADMAP.md](docs/ROADMAP.md) |

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

## Releasing

1. Bump `version` in `crates/openmacro-app/Cargo.toml` and commit.
2. Tag with release notes and push:

   ```bash
   git tag -a v0.3.0 -m "What changed in this release"
   git push origin v0.3.0
   ```

3. The [release workflow](.github/workflows/release.yml) builds the food database, builds and signs the arm64 and x86_64 APKs, and publishes a GitHub Release. Installed apps find it on their next launch and offer the update.

Releases are signed with the project's release key, stored in the repository secrets `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD` and `ANDROID_KEY_ALIAS`. Every update must be signed with the same key.

## Clean-room policy

OpenMacro is an independent project. It is not affiliated with, endorsed by, or connected to MacroFactor or Stronger By Science.

- No proprietary source code, decompiled code, binaries, or assets (icons, images, copy, fonts) from other apps are committed to this repository.
- The specs in `docs/` describe *behavior* in our own words, and the implementation is written from those specs and public science.
- Reverse-engineering artifacts stay local in `research/private/`, which is git-ignored.

## License

Code: MIT; see [LICENSE](LICENSE). The food database has its own licences (USDA public domain, FSANZ CC BY 4.0, Open Food Facts ODbL); see [docs/FOOD_DATA.md](docs/FOOD_DATA.md).
