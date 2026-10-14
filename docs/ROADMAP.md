# OpenMacro roadmap

Status of the features in [FEATURES.md](FEATURES.md), in the order we plan to build them.

**Legend:** ✅ done · 🚧 in progress · ⏭ next · 🕓 later

## Done

| Feature | Notes |
|---|---|
| ✅ Core maths | Trend weight, adaptive expenditure (Kalman-style), BMR, goal targets, calorie shifting, macro splits |
| ✅ App shell | Dioxus app for Android and web preview: dashboard, food log, plan, more |
| ✅ Food log | Timeline by hour with a fixed header; day status (normal, partial, fasted) |
| ✅ Logging sheet | Search, food detail with number pad (fractions, unit chips), quick add, plate |
| ✅ On-device storage | SQLite: food log, weigh-ins, day status, profile, goal, program |
| ✅ Food database | 58k foods: USDA, AFCD (Australia) and Open Food Facts (AU + US). Common and Branded sections, region setting, spelling variants. See [FOOD_DATA.md](FOOD_DATA.md) |
| ✅ Releases and in-app updates | Signed APKs built by GitHub Actions on version tags. The app checks GitHub Releases, downloads the update and opens Android's installer |
| ✅ Custom foods | Library tab and a new-food form (per serving / 100 g / 100 ml, Australian or US label, kJ or kcal, salt or sodium); shown first in search and barcode lookups |
| ✅ Recent foods | Log entries remember their food; search shows recent foods when empty and ranks foods you log often higher |
| ✅ Barcode scanning | Live camera scanning (EAN/UPC, decoded in Rust with rxing), torch, manual entry; local database first, then Open Food Facts online |

## Next

| # | Feature | Plan |
|---|---|---|
| 1 | ⏭ **Favorites** | Star foods on their detail page; favorites appear in the Library. |
| 2 | ⏭ **Weekly check-in** | Targets lock in at a weekly check-in on a chosen day. The check-in steps through weigh-in, partial days, fasted days and program update. |
| 3 | ⏭ **Data export** | CSV export of the food log, weigh-ins, expenditure and trend. |

## Later

| Feature | Notes |
|---|---|
| 🕓 Nutrition label scan | Photograph a label to fill in a custom food. Needs an AI vision model (see below). |
| 🕓 AI photo logging | *Snap* a meal or *Describe* it. The model names foods and portions, and the app matches them to real foods in the database. **Bring your own API key** in settings; the first provider is still to be chosen (Claude, Gemini or OpenAI). Nothing is sent unless the user turns it on. |
| 🕓 Recipes | Build from ingredients, import from a link (schema.org Recipe), split a logged recipe into ingredients |
| 🕓 Weight trend V2 | Kalman smoother with a noise band; calibrated expenditure uncertainty band |
| 🕓 Body metrics and progress photos | Measurements, front/side/back photos with comparison |
| 🕓 Health Connect | Read weight, body fat and steps; write nutrition |
| 🕓 Insight detail pages | Expenditure, weight trend and energy balance charts with 1W–All ranges and insights (e.g. weight change over 3/7/14/30/90 days) |
| 🕓 Google Play / F-Droid | Silent store updates for non-technical users |
