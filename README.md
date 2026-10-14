# OpenMacro

An open-source, local-first nutrition and macro tracker written in Rust, with an adaptive expenditure (TDEE) coach. It is inspired by apps such as MacroFactor.

## Status

Early development.

| Component | State |
|---|---|
| [`openmacro-core`](crates/openmacro-core) | Working and tested: nutrient model (55 nutrients), foods and servings, BMR equations, trend weight, adaptive expenditure, goal-driven calorie targets, weekly calorie shifting, macro splits |
| Storage, food database, check-in coach, app UI | Planned; see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |

## Docs

- [Feature specification](docs/FEATURES.md): what the app does, with priorities
- [Algorithms](docs/ALGORITHMS.md): trend weight, expenditure, targets and macros
- [Architecture](docs/ARCHITECTURE.md): crates, storage, food data, UI plan

## Development

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

## Clean-room policy

OpenMacro is an independent project. It is not affiliated with, endorsed by, or connected to MacroFactor or Stronger By Science.

- No proprietary source code, decompiled code, binaries, or assets (icons, images, copy, fonts) from other apps are committed to this repository.
- The specs in `docs/` describe *behavior* in our own words, and the implementation is written from those specs and public science.
- Reverse-engineering artifacts stay local in `research/private/`, which is git-ignored.

## License

MIT; see [LICENSE](LICENSE). Food data imported from Open Food Facts is licensed separately under the ODbL.
