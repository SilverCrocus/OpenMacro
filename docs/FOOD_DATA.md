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
