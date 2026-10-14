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
| ✅ Favorites | Star foods on their detail page; favorites appear in the Library and rank first in search |
| ✅ Barcode scanning | Live camera scanning (EAN/UPC, decoded in Rust with rxing), torch, manual entry; local database first, then Open Food Facts online |
| ✅ Weekly check-in | Check-in day setting, due card on Plan and Dashboard. Three steps: weigh-in, review of unlogged and partly logged days, then new targets. Targets stay fixed until the next check-in |
| ✅ Data export | CSV files for the food log (every nutrient), weigh-ins and a daily summary (intake, weight, trend, expenditure, targets). Saved to Downloads/OpenMacro on Android |
| ✅ Insight pages | Tap the Expenditure, Weight trend or Energy balance tile for a chart with 1W–All ranges. Expenditure shows its likely range; weight shows scale weigh-ins and trend change over 3–90 days; energy balance shows each complete day's surplus or deficit and the weight change it implies |
| ✅ Recipes | Put the ingredients on the plate and tap *Save as recipe*: name, servings and an optional cooked weight. Recipes appear in search and the Library and log by the serving, the whole recipe or by weight. Ingredients are kept for editing later |
| ✅ Backup and restore | One JSON file with everything (log, weigh-ins, check-ins, your foods and recipes, favorites, settings), saved to Downloads/OpenMacro and restored with the system file picker after a confirmation. The food database is kept out of Android's automatic backup so the user's own data fits its 25 MB limit |
| ✅ Units and theme | kcal or kJ, kg or lb, 12- or 24-hour clock; dark, light or system theme. Data stays in kcal and kg; only display and input convert |
| ✅ Manual program | Program style *Coached* or *Manual*. Manual uses calories and macros you enter, every day; check-ins still update expenditure but leave them alone |
| ✅ Plate improvements | Back keeps the plate (the + button shows how many items wait), discarding asks first, plate items can be re-measured, and any food can be copied to your own foods |
| ✅ Macros and goal insights | *Calories and macros*: daily intake against target for calories, protein, fat or carbs, with averages and days on target. *Goal progress*: trend weight against the goal with a projection at the goal rate and an estimated date. Both have dashboard tiles |
| ✅ Copy meals | Copy an hour's food or a whole day to another date, from the food log |
| ✅ Edit foods and recipes | The Library's edit button reopens a custom food's form, or puts a recipe's ingredients back on the plate to change, then *Update recipe*. Log entries keep the values they were logged with |
| ✅ Edit log entries | Tap a logged food to change its amount (with its servings and the number pad) or time; quick adds edit their calories and macros |
| ✅ Android Back | Closes the open detail, form or sheet, then returns to the Dashboard, then leaves the app |

## Next

| # | Feature | Plan |
|---|---|---|
| 1 | ⏭ **Micronutrient targets** | Floors, targets and ceilings for vitamins and minerals, with a nutrient explorer. |
| 2 | ⏭ **Goal history and maintenance** | Keep past goals; a maintain goal that corrects gently when the trend leaves a band (spec P0). |
| 3 | ⏭ **Steps and body measurements** | Manual entry first; Health Connect later. |

## Later

| Feature | Notes |
|---|---|
| 🕓 Nutrition label scan | Photograph a label to fill in a custom food. Needs an AI vision model (see below). |
| 🕓 AI photo logging | *Snap* a meal or *Describe* it. The model names foods and portions, and the app matches them to real foods in the database. **Bring your own API key** in settings; the first provider is still to be chosen (Claude, Gemini or OpenAI). Nothing is sent unless the user turns it on. |
| 🕓 More recipe features | Import from a link (schema.org Recipe), split a logged recipe into ingredients, recipes inside recipes |
| 🕓 Weight trend V2 | Kalman smoother with a noise band; calibrated expenditure uncertainty band |
| 🕓 Body metrics and progress photos | Measurements, front/side/back photos with comparison |
| 🕓 Health Connect | Read weight, body fat and steps; write nutrition |
| 🕓 Google Play / F-Droid | Silent store updates for non-technical users |
