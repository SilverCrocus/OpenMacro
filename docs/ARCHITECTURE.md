# OpenMacro architecture

OpenMacro is written in Rust. All domain logic is pure and UI-agnostic, so it can be tested in isolation and reused by any front end.

## Workspace layout

```text
crates/
  openmacro-core/    pure domain logic: nutrients, foods, trend, expenditure, targets   [exists]
  openmacro-coach/   goals, programs, weekly check-in orchestration                     [planned]
  openmacro-store/   SQLite persistence, migrations, repositories                       [planned]
  openmacro-foods/   food-database importers and local search                           [planned]
  openmacro-app/     cross-platform UI with Dioxus (web + Android)                      [preview]
```

Dependencies only point downward:

```text
app → coach → core
app → store → core
app → foods → core
```

`core` has no I/O and no platform code.

## Storage (`openmacro-store`)

- **Database:** SQLite through `rusqlite` with the bundled build, with versioned migrations.
- **Local-first:** the app works fully offline, and the local database is the source of truth.
- **Sync-ready rows:** every user-data row carries `id` (UUID v7), `updated_at` and `deleted_at`. Deletions are soft (tombstones), which lets an optional sync server merge per row (last writer wins) later without a schema change.
- **Main tables:**
  - `food_log_entry`: date, time, food snapshot, amount, nutrients.
  - `day`: status (normal, fasted or partial), plus a note.
  - `weigh_in`, `body_measurement`, `progress_photo`.
  - `custom_food`, `recipe`, `recipe_ingredient`, `favorite`.
  - `goal`, `program`, `weekly_plan`, `check_in`.
  - `nutrient_goal`: dated floor, target and ceiling per nutrient.
  - `settings`.
- **Snapshots:** a log entry stores a copy of the food's nutrients, so later edits to a custom food or database updates don't rewrite history.

## Food database (`openmacro-foods`)

| Source | License | Use |
|---|---|---|
| USDA FoodData Central: Foundation, SR Legacy, FNDDS | Public domain | Generic "common" foods |
| USDA FoodData Central: Branded | Public domain | US packaged foods |
| Open Food Facts | ODbL (attribution; derived DB stays open) | Worldwide packaged foods and barcodes |

How it works:
- Importers turn the bulk dumps into a compact SQLite file that ships with the app or downloads on first run.
- **Search:** SQLite FTS5, with a trigram tokenizer for typo tolerance.
- **Ranking:**
  1. text relevance;
  2. a boost for foods in the user's history, weighted by recency and frequency;
  3. a boost for the user's custom foods and recipes.
- **Barcodes:** lookup in the local index first, then fall back to the Open Food Facts API when online.

## UI (`openmacro-app`)

**Chosen: [Dioxus](https://dioxuslabs.com) 0.7.** It is a single Rust codebase for Android, iOS, desktop and web, with React-like components and hot reload.

| Option | Pros | Cons |
|---|---|---|
| **Dioxus** (recommended) | All Rust; one codebase for every platform; good DX | Mobile support is younger than the alternatives |
| Tauri 2 + web front end | Mature mobile support, rich plugin ecosystem | UI written in TS/HTML; two languages |
| Slint | Native-feeling, small binary | Smaller component ecosystem for charts and lists |
| Rust core + native Kotlin/Swift UI via UniFFI | Best platform fidelity | Two UIs to maintain |

Platform features that need native code go behind small plugin interfaces:

| Feature | Approach |
|---|---|
| Health Connect / Apple Health | Kotlin/Swift shims called over JNI or FFI |
| Barcode scanning | Camera frames decoded with `rxing` (a pure-Rust ZXing port), or ML Kit through a Kotlin shim on Android |
| Home-screen widgets | Native widget code reading a small shared summary file |

## AI logging (optional)

AI logging sits behind an `AiFoodRecognizer` trait. Its input is a photo and/or text; its output is candidate items as `{name, quantity, unit}`. Each candidate is then **matched against the local food database**, so the nutrient values come from real data rather than the model. The user brings their own model endpoint or API key, and nothing is sent anywhere unless they turn the feature on.

## Testing

- `openmacro-core`: unit tests for every algorithm, including simulated-person tests that check the expenditure estimator recovers a known true value.
- `openmacro-store`: migration tests against a temporary database.
- CI: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`.
