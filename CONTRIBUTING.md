# Contributing to OpenMacro

Thanks for helping! OpenMacro is a free, open-source nutrition coach, and contributions of every size are welcome: bug reports, ideas, food data fixes, docs, design and code.

## Ways to help

- **Use it and report what's wrong or missing.** [Open an issue](https://github.com/SilverCrocus/OpenMacro/issues/new/choose). Screenshots help a lot.
- **Pick up an issue.** Ones labelled [`good first issue`](https://github.com/SilverCrocus/OpenMacro/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22) are small and self-contained; [`help wanted`](https://github.com/SilverCrocus/OpenMacro/issues?q=is%3Aissue+is%3Aopen+label%3A%22help+wanted%22) ones are bigger. Comment on an issue before starting, so two people don't build the same thing.
- **Build something from the [roadmap](docs/ROADMAP.md).** For anything large, open an issue first to agree on the approach.
- **Improve the docs or translations,** or add foods from your country's food composition database.

## Setting up

You need Rust (stable) and the Dioxus CLI 0.7 (`cargo install dioxus-cli --version 0.7.10`). Then:

```bash
git clone https://github.com/SilverCrocus/OpenMacro
cd OpenMacro
cargo test
dx serve --package openmacro-app --platform web
```

The web preview is the fastest way to work on the UI; tap **Explore with demo data** for a filled-in account. Building for Android is covered in [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## Making a change

1. Fork the repository and create a branch.
2. Keep the change focused: one fix or feature per pull request.
3. Add or update tests for logic changes. The maths in `openmacro-core` should always have tests.
4. Before pushing, run the same checks as CI:

   ```bash
   cargo fmt --all
   cargo clippy --all-targets -- -D warnings
   cargo test
   cargo check -p openmacro-app --target wasm32-unknown-unknown
   ```

5. Open a pull request saying what changed and why, with screenshots for UI changes.

## Style

- Match the code around you: its naming, comment density and idioms.
- Comments explain *why*, not what.
- Write UI text in plain language a non-technical person understands, in sentence case.
- Algorithms are documented in [docs/ALGORITHMS.md](docs/ALGORITHMS.md). Update it when you change one.

## Clean-room rule

OpenMacro is an independent, clean-room project. **Never contribute code, assets, text or data copied from another app**, including decompiled or reverse-engineered code, icons, screenshots and proprietary food databases. Describe behaviour in your own words, build from public science and openly licensed data, and say where data comes from. Pull requests that don't follow this can't be accepted.

## Conduct

Be kind and constructive. This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md).

## Licence

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
