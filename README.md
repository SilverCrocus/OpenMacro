<p align="center">
  <img src="docs/images/icon-rounded.png" width="128" alt="OpenMacro's icon: a smiling blue bowl of broccoli, rice and salmon">
</p>

<h1 align="center">OpenMacro</h1>

<p align="center">
  <b>The open-source nutrition coach.</b><br>
  Calorie and macro targets that adapt to your body, logging in seconds (even from a photo),<br>
  and your data stays on your phone.
</p>

<p align="center">
  <a href="https://github.com/SilverCrocus/OpenMacro/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/SilverCrocus/OpenMacro?label=release&color=4fd1a5"></a>
  <a href="https://github.com/SilverCrocus/OpenMacro/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/SilverCrocus/OpenMacro/actions/workflows/ci.yml/badge.svg"></a>
  <a href="LICENSE"><img alt="MIT license" src="https://img.shields.io/badge/license-MIT-blue"></a>
  <img alt="Android 8+" src="https://img.shields.io/badge/Android-8%2B-3ddc84?logo=android&logoColor=white">
  <img alt="Made with Rust" src="https://img.shields.io/badge/made%20with-Rust-dea584?logo=rust&logoColor=white">
  <a href="https://github.com/SilverCrocus/OpenMacro/stargazers"><img alt="GitHub stars" src="https://img.shields.io/github/stars/SilverCrocus/OpenMacro?style=flat&color=f2c14e"></a>
</p>

<p align="center">
  <a href="https://github.com/SilverCrocus/OpenMacro/releases/latest"><b>Download for Android</b></a>
  &nbsp;·&nbsp;
  <a href="#features">Features</a>
  &nbsp;·&nbsp;
  <a href="#ai-logging">AI logging</a>
  &nbsp;·&nbsp;
  <a href="CONTRIBUTING.md">Contribute</a>
</p>

<p align="center">
  <img src="docs/images/hero.png" width="100%" alt="Five OpenMacro screens: the dashboard, the food log, an AI-logged salmon poke bowl, the weight trend and the plan">
</p>

## Why OpenMacro?

Most calorie apps give you a target from a formula and leave it there. OpenMacro **works out what you actually burn** from what you eat and how your weight changes, and updates your targets every week. It's the same evidence-based approach as the best paid coaching apps, but free and open.

- 🎯 **Coaching that adapts.** Your expenditure is estimated from your own data, with a weekly check-in that adjusts calories and macros to your goal: lose, maintain or gain.
- ⚡ **Fast logging.** Search 58,000 foods, scan barcodes, reuse recipes and recent foods, copy whole meals, or snap a photo and let AI fill in the plate.
- 🔒 **Yours.** No account, no subscription, no ads, no tracking. Your data lives on your phone and works offline, with backups, CSV export and Health Connect sync.
- 🛠️ **Open.** MIT licensed and written in Rust. Read how every number is calculated in [the algorithms doc](docs/ALGORITHMS.md).

## See it in 40 seconds

<p align="center">
  <a href="docs/media/openmacro-tour.mp4"><img src="docs/images/video-thumbnail.png" width="720" alt="Watch the 42-second OpenMacro tour"></a>
</p>

## Features

<table>
  <tr>
    <td align="center"><img src="docs/screenshots/dashboard.png" width="230" alt="Dashboard"><br><b>Dashboard</b><br><sub>Today's intake, expenditure, weight trend and habits at a glance</sub></td>
    <td align="center"><img src="docs/screenshots/food-log.png" width="230" alt="Food log"><br><b>Food log</b><br><sub>A timeline by hour; AI meals show as one row</sub></td>
    <td align="center"><img src="docs/screenshots/ai-logging.png" width="230" alt="AI logging"><br><b>AI logging</b><br><sub>Photo or description in, foods and amounts out</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="docs/screenshots/weight-trend.png" width="230" alt="Weight trend"><br><b>Weight trend</b><br><sub>A smoothed trend that sees past daily water swings</sub></td>
    <td align="center"><img src="docs/screenshots/expenditure.png" width="230" alt="Expenditure"><br><b>Expenditure</b><br><sub>What you really burn, with its likely range</sub></td>
    <td align="center"><img src="docs/screenshots/plan.png" width="230" alt="Plan"><br><b>Plan</b><br><sub>Your goal, rate, diet style and weekly check-in</sub></td>
  </tr>
</table>

**Coaching**
- Adaptive expenditure (TDEE) estimate with an uncertainty band, starting from Mifflin-St Jeor, Katch-McArdle or Cunningham
- Goals to lose, maintain or gain at a chosen rate; coached or manual programs; weekly calorie shifting
- Balanced, low-fat, low-carb and keto macro splits, from low to extra-high protein
- Weekly check-ins that review your week before changing your targets

**Logging**
- 58,000 foods from USDA FoodData Central, the Australian Food Composition Database and Open Food Facts
- Live barcode scanning, with Open Food Facts lookup for products not in the database
- Custom foods, recipes with cooked weights, favourites, recent foods and quick add
- Copy a meal or a whole day; a timeline by hour; water tracking
- AI logging from a photo or a description, with corrections in plain words

**Insights**
- Expenditure, weight trend, energy balance, calories and macros, goal progress and body measurements, with 1W–All ranges
- Fibre, vitamins and minerals against standard targets for your age and sex, with the top sources of each

**Your data**
- Stored on your phone; works offline
- Backups to one file, CSV export, and Health Connect sync (weight and body fat in; food, water and weight out)
- kcal or kJ, kg or lb, dark or light theme

## AI logging

Take a photo of your meal, or just describe it ("two eggs on toast with butter"). The AI lists the foods with their weights and puts them on your plate to check before you log. If something's wrong, tell it in plain words, such as "it was a peanut curry, not chickpea", and it looks again.

**Bring your own key.** Use any of these:
- **Google Gemini:** free tier available.
- **OpenRouter:** one key for hundreds of models.
- **OpenAI** or **Anthropic.**
- **Any OpenAI-compatible service,** including Ollama on your own computer.

A photo costs well under a cent. Nothing is sent anywhere unless you log with AI, and your key never leaves your phone or goes into backups.

**We measured which model to recommend.** We tested 7 small models on [100 photos of weighed meals](docs/AI_PHOTO_EVAL.md):

| Model | Average error | Time per photo | Cost per 1,000 photos |
|---|---|---|---|
| **Gemini 3.1 Flash Lite** (default) | 88 kcal | 2.4 s | $0.67 |
| Gemini 3.8 Flash | 88 kcal | 5.1 s | $2.08 |
| Gemma 4 26B (open weights) | 98 kcal | 4.7 s | $0.08 |
| Claude Haiku 5.5 | 109 kcal | 5.0 s | $0.45 |

## Install

1. Download the latest APK from [Releases](https://github.com/SilverCrocus/OpenMacro/releases/latest). Most phones need the `arm64-v8a` one.
2. Open it, and allow your browser or file manager to install apps if Android asks.
3. That's it. OpenMacro checks for new versions when it opens and updates itself.

You can also add the repository to [Obtainium](https://github.com/ImranR98/Obtainium) to get updates. An iPhone version isn't available yet.

## Contributing

Contributions are very welcome, whether code, design, docs, food data or bug reports.
- **Getting started:** [CONTRIBUTING.md](CONTRIBUTING.md) explains how to set up (the web preview runs with one command) and how to pick something to work on.
- **What's next:** the [roadmap](docs/ROADMAP.md) lists what's planned.
- **Building and releasing:** see [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

Built with [Rust](https://www.rust-lang.org) and [Dioxus](https://dioxuslabs.com), with SQLite for storage.

## Clean-room policy

OpenMacro is an independent project, inspired by adaptive coaching apps such as MacroFactor. It is not affiliated with, endorsed by, or connected to MacroFactor or Stronger By Science.
- **No copied code or assets:** the repository contains no proprietary source code, decompiled code, binaries or assets (icons, images, copy, fonts) from other apps.
- **Written from our own specs:** the specs in `docs/` describe behaviour in our own words, and the code is written from them and from public science.

OpenMacro gives general nutrition information, not medical advice.

## License

The code is under the MIT license; see [LICENSE](LICENSE). The food database has its own licences: USDA (public domain), FSANZ (CC BY 4.0) and Open Food Facts (ODbL). See [docs/FOOD_DATA.md](docs/FOOD_DATA.md).
