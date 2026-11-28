# Food data

OpenMacro searches one SQLite file, `foods.db`, built from public datasets by the `build-food-db` tool in [`crates/openmacro-foods`](../crates/openmacro-foods). The raw downloads and the built database are not committed to git.

## Sources

| Dataset | What it adds | Licence | Credit |
|---|---|---|---|
| [USDA FoodData Central](https://fdc.nal.usda.gov/download-datasets/): FNDDS, Foundation Foods, SR Legacy | ~13,300 generic foods with portions (cups, slices, pieces) | Public domain (CC0) | "USDA FoodData Central" |
| [Australian Food Composition Database](https://www.foodstandards.gov.au/science-data/food-nutrient-databases/afcd/data-files) Release 3 | 1,588 Australian generic foods | CC BY 4.0 ([FSANZ copyright](https://www.foodstandards.gov.au/legal-policies/copyright)) | "Food Standards Australia New Zealand" |
| [Open Food Facts](https://world.openfoodfacts.org/data) CSV export | Australian and US packaged products with barcodes | Database ODbL, contents DbCL | "Open Food Facts contributors" |

The credit text is stored in the database (`meta.attribution`) and shown in the app under **More → Food database**.

**Open Food Facts licence:** under the ODbL, the bundled `foods.db` is a derived database and must be offered under the ODbL too. The app's code stays MIT.

## Fast-food chain menus

The **Eat out** tab (and search) has the menus of Australian fast-food chains. They aren't in `foods.db`: each chain is one JSON file in [`crates/openmacro-app/data/chains/`](../crates/openmacro-app/data/chains), built into the app.

- **Chains:** Domino's, Guzman y Gomez, Hungry Jack's, KFC, McDonald's, Nando's, Oporto, Pizza Hut, Red Rooster, Schnitz, Subway and Zambrero. Taco Bell isn't included because it publishes no nutrition in Australia. Grill'd and Mad Mex aren't included yet.
- **Source:** each chain's own published nutrition information (its website or the nutrition guide it links to). Every file records the page or PDF and its date, and the app shows them under the chain's menu. These figures aren't openly licensed like the datasets above. They're the chains' public nutrition facts, kept with their source.
- **Values:** per serve, as published: energy in kJ, protein, fat, saturated fat, carbohydrate, sugars, fibre and sodium. Available carbohydrate has fibre added back, as for the AFCD. Most chains don't publish fibre (only GYG and Subway do), so for the rest the carbohydrate is available carbohydrate, slightly under the total. A serve whose weight isn't published is logged by the serve only (no grams), and copying it to a custom food asks for its weight.
- **Leave off:** an item's `leave_off` lists what a customer can ask to leave off it (mayo and other creamy sauces, cheese, bacon, sour cream, guacamole, butter, sweet sauces, dressings). The list comes from the chain's own product description, ingredient list or build. The app offers these as toggles on the item, and logs the item without them under its own id (`…~no-mayo`).
  - **Chain figures:** a part's figures are the chain's own where it publishes them, in the file's `parts` with their source. Some come from the chain's add-on or remove options, or from the difference between two items that differ only by that part.
  - **Energy only:** where a chain publishes only a part's energy, the rest is filled in from a typical serve of that kind, scaled to that energy.
  - **Estimates:** where a chain publishes nothing for a part, the app estimates it from a typical serve and shows it as "about". Items under 300 kcal and pizza slices only get parts with chain figures, because a typical serve is too much for them.

  Typical serves are sized to the median energy of the parts chains do publish. Per 100 g values are from AFCD Release 3, or USDA FNDDS where AFCD has no match:

  | Kind | Serve | Per 100 g from |
  |---|---|---|
  | mayo | 11 g | AFCD F005441 Mayonnaise, traditional |
  | cheese | 15 g | AFCD F002428 Cheese, cheddar, processed |
  | cheese sauce | 30 g | FNDDS 2705808 Cheese sauce |
  | bacon | 15 g | AFCD F000231 Bacon, shortcut, fried |
  | sour cream | 25 g | AFCD F003269 Cream, sour, regular fat |
  | guacamole | 50 g | FNDDS 2709307 Guacamole |
  | butter | 6 g | AFCD F001973 Butter, salted |
  | sweet sauce | 20 g | AFCD F007987 Sauce, barbecue |
  | dressing | 30 g | AFCD F003607 Dressing, French or Italian |
- **Ids:** each item has a fixed `id` in its file. Logged entries and favourites find the item by it, so keep it when refreshing a chain, even if the chain renames the item.
- **What's left out:** limited-time items, anything whose figures don't add up (the energy must be within about 15% of 17 kJ/g protein + 37 kJ/g fat + 17 kJ/g carbohydrate + 8 kJ/g fibre), and anything whose figures disagree with another the chain publishes, such as the kJ on its menu.
- **Suggestions:** meal-sized orders that fit the calories left today, with the most protein per calorie first. See ALGORITHMS.md §7.
- **Updating:** menus change. To refresh a chain, update its items from the chain's current guide, keeping each item's `id`, and update `source.published` and `source.accessed`.

## Building `foods.db`

1. Download the datasets into `data/raw/`:

   ```text
   data/raw/
     usda/   FoodData_Central_survey_food_csv_<date>.zip      (unzipped)
             FoodData_Central_foundation_food_csv_<date>.zip  (unzipped)
             FoodData_Central_sr_legacy_food_csv_2018-04.zip  (unzipped)
     afcd-r3/AFCD Release 3 - Nutrient profiles.xlsx
     off/    en.openfoodfacts.org.products.csv.gz             (~1.3 GB, not unzipped)
   ```

2. Build the database and copy it into the app's assets:

   ```bash
   cargo run --release -p openmacro-foods --features import --bin build-food-db -- data/raw data/foods.db
   cp data/foods.db crates/openmacro-app/assets/foods.db
   ```

   The Open Food Facts file is streamed, so the build takes about 20 seconds and under 100 MB of RAM. Options:
   - `--skip-off`: generic foods only (about 7 MB).
   - `--us-min-scans N`: keep only US products scanned at least N times (default 1).

3. Build the app with the database bundled:

   ```bash
   dx build --package openmacro-app --platform android --target aarch64-linux-android --features bundled-foods
   ```

   Without `bundled-foods` (CI, the web preview), the app falls back to its small built-in sample list.

## Conventions

- **Units:** nutrients are per 100 g in the units of `openmacro_core::nutrients` (kcal, g, mg, µg). Kilojoules are divided by 4.184.
- **Carbohydrate:** stored as *total* carbohydrate including fibre (the US label convention). AFCD and Australian-only Open Food Facts products report available carbohydrate, so the importer adds fibre back. Net carbs are derived in the app as carbs − fibre.
- **Omega-3:** ALA + EPA + DHA + DPA where those are reported. **Omega-6:** linoleic + arachidonic acid.
- **Regions:**
  - USDA foods are tagged US and AFCD foods AU. Open Food Facts products are tagged from `countries_tags`.
  - Generic foods from every region show in search, with the user's region ranked higher.
  - Branded products only show for the user's region.
- **Duplicates:** USDA foods with the same name keep the first dataset imported, in the order FNDDS, then Foundation, then SR Legacy.
- **Plausibility filter:** a food must have energy, protein, fat and carbs; no negative values; at most 950 kcal per 100 g; and protein + fat + carbs at most 110 g per 100 g.

## Current build (October 2026)

| Source | Foods |
|---|---|
| USDA FNDDS | 5,431 |
| USDA Foundation | 298 |
| USDA SR Legacy | 7,611 |
| AFCD Release 3 | 1,588 |
| Open Food Facts (AU 11,678 · US 35,623 · both 135) | 47,436 |
| **Total** | **62,364** (21.5 MB) |
