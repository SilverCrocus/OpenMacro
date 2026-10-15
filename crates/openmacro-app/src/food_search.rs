//! Food search: the full food database on native builds when it is bundled,
//! otherwise the small built-in sample list.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use openmacro_core::food::{Food, FoodSource, Serving};
use openmacro_core::log::LogEntry;
use openmacro_core::nutrients::NutrientProfile;

use crate::foods::{FOODS, search as search_samples};

/// The region whose products and spellings to prefer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FoodRegion {
    #[default]
    Australia,
    UnitedStates,
}

impl FoodRegion {
    pub const ALL: [FoodRegion; 2] = [FoodRegion::Australia, FoodRegion::UnitedStates];

    pub fn code(self) -> &'static str {
        match self {
            FoodRegion::Australia => "au",
            FoodRegion::UnitedStates => "us",
        }
    }

    pub fn from_code(code: &str) -> Option<FoodRegion> {
        Self::ALL.into_iter().find(|r| r.code() == code)
    }

    pub fn label(self) -> &'static str {
        match self {
            FoodRegion::Australia => "Australia",
            FoodRegion::UnitedStates => "United States",
        }
    }

    /// Region flags as stored in the food database.
    #[cfg(not(target_arch = "wasm32"))]
    fn flags(self) -> u8 {
        match self {
            FoodRegion::Australia => openmacro_foods::region::AU,
            FoodRegion::UnitedStates => openmacro_foods::region::US,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use openmacro_foods::FoodDb as Database;

/// The web preview has no SQLite, so it never has a database.
#[cfg(target_arch = "wasm32")]
pub struct Database;

/// A search result with where its data came from.
#[derive(Debug, Clone, PartialEq)]
pub struct FoundFood {
    pub food: Food,
    pub source: &'static str,
}

/// One of the user's own foods or recipes.
fn yours(food: &Food) -> FoundFood {
    FoundFood {
        food: food.clone(),
        source: if food.source == FoodSource::Recipe {
            "Your recipe"
        } else {
            "Your food"
        },
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Results {
    /// The user's own custom foods.
    pub yours: Vec<FoundFood>,
    pub common: Vec<FoundFood>,
    pub branded: Vec<FoundFood>,
}

impl Results {
    pub fn is_empty(&self) -> bool {
        self.yours.is_empty() && self.common.is_empty() && self.branded.is_empty()
    }
}

/// Custom foods whose name or brand contains every word of `query`.
pub fn search_custom(custom: &[Food], query: &str) -> Vec<FoundFood> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return Vec::new();
    }
    custom
        .iter()
        .filter(|f| {
            let text =
                format!("{} {}", f.name, f.brand.as_deref().unwrap_or_default()).to_lowercase();
            words.iter().all(|w| text.contains(w.as_str()))
        })
        .map(yours)
        .collect()
}

/// Results kept per section; the list shows a few and offers the rest.
#[cfg(not(target_arch = "wasm32"))]
const PER_SECTION: usize = 50;

/// Searches the user's custom foods and the database if there is one,
/// otherwise the sample foods. Within each section, foods the user has
/// logged before move up (most-logged first), keeping text rank otherwise.
pub fn search(
    db: Option<&Database>,
    custom: &[Food],
    history: &HashMap<String, usize>,
    query: &str,
    region: FoodRegion,
) -> Results {
    let mut results = search_unranked(db, custom, query, region);
    for section in [
        &mut results.yours,
        &mut results.common,
        &mut results.branded,
    ] {
        section.sort_by_key(|f| Reverse(history.get(&f.food.id).copied().unwrap_or(0)));
    }
    results
}

fn search_unranked(
    db: Option<&Database>,
    custom: &[Food],
    query: &str,
    region: FoodRegion,
) -> Results {
    let yours = search_custom(custom, query);
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(db) = db {
        let hits = db
            .search(query, region.flags(), PER_SECTION * 3)
            .unwrap_or_default();
        let mut results = Results {
            yours,
            ..Results::default()
        };
        for hit in hits {
            let found = FoundFood {
                food: hit.food,
                source: hit.dataset.label(),
            };
            let section = if found.food.source == FoodSource::Branded {
                &mut results.branded
            } else {
                &mut results.common
            };
            if section.len() < PER_SECTION {
                section.push(found);
            }
        }
        return results;
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (db, region);
    #[cfg(not(target_arch = "wasm32"))]
    let _ = region;

    Results {
        yours,
        common: search_samples(&FOODS, query)
            .into_iter()
            .map(|(_, food)| FoundFood {
                food: food.clone(),
                source: "Sample",
            })
            .collect(),
        branded: Vec::new(),
    }
}

/// How many times each food (by stable id) appears in the log.
pub fn history_counts(log: &[LogEntry]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for id in log.iter().filter_map(|e| e.food_id.as_ref()) {
        *counts.entry(id.clone()).or_insert(0) += 1;
    }
    counts
}

/// Finds a food by its stable id in the user's foods, the samples or the database.
pub fn resolve(db: Option<&Database>, custom: &[Food], id: &str) -> Option<FoundFood> {
    if id.starts_with("custom:") {
        return custom.iter().find(|f| f.id == id).map(yours);
    }
    if id.starts_with("sample:") {
        return crate::foods::food_by_id(id).map(|f| FoundFood {
            food: f.clone(),
            source: "Sample",
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(hit) = db.and_then(|db| db.get_by_key(id).ok().flatten()) {
        return Some(FoundFood {
            food: hit.food,
            source: hit.dataset.label(),
        });
    }
    let _ = db;
    None
}

/// Grams in an amount label such as "1.5 large (75 g)" or "250 g".
fn grams_in(amount: &str) -> Option<f64> {
    let inner = match (amount.rfind('('), amount.rfind(" g)")) {
        (Some(open), Some(end)) if open < end => &amount[open + 1..end],
        _ => amount.strip_suffix(" g")?,
    };
    inner.trim().parse().ok().filter(|g: &f64| *g > 0.0)
}

/// Rebuilds a food from something logged or on the plate when the original
/// can't be found (an online-only product while offline, or an AI
/// estimate), if its amount says how many grams it was.
pub fn rebuild(
    id: &str,
    name: &str,
    amount: &str,
    nutrients: &NutrientProfile,
) -> Option<FoundFood> {
    let grams = grams_in(amount)?;
    Some(FoundFood {
        food: Food {
            id: id.to_string(),
            name: name.to_string(),
            brand: None,
            source: FoodSource::Common,
            barcode: None,
            per_100g: nutrients.scaled(100.0 / grams),
            servings: vec![Serving::new("portion", grams)],
        },
        source: "Logged",
    })
}

fn from_entry(entry: &LogEntry) -> Option<FoundFood> {
    rebuild(
        entry.food_id.as_deref()?,
        &entry.name,
        &entry.amount,
        &entry.nutrients,
    )
}

/// The food a log entry came from, or one rebuilt from the entry.
pub fn resolve_entry(
    db: Option<&Database>,
    custom: &[Food],
    entry: &LogEntry,
) -> Option<FoundFood> {
    resolve(db, custom, entry.food_id.as_deref()?).or_else(|| from_entry(entry))
}

/// Foods logged most recently, newest first, without repeats.
pub fn recent_foods(
    db: Option<&Database>,
    custom: &[Food],
    log: &[LogEntry],
    limit: usize,
) -> Vec<FoundFood> {
    let mut entries: Vec<&LogEntry> = log.iter().filter(|e| e.food_id.is_some()).collect();
    entries.sort_by_key(|e| Reverse((e.date, e.hour, e.id)));
    let mut seen = HashSet::new();
    let mut recent = Vec::new();
    for entry in entries {
        let id = entry.food_id.as_deref().unwrap_or_default();
        if !seen.insert(id.to_string()) {
            continue;
        }
        if let Some(found) = resolve(db, custom, id).or_else(|| from_entry(entry)) {
            recent.push(found);
            if recent.len() == limit {
                break;
            }
        }
    }
    recent
}

/// A product by barcode: the user's custom foods first, then the database,
/// trying both UPC and EAN forms.
pub fn by_barcode(db: Option<&Database>, custom: &[Food], code: &str) -> Option<FoundFood> {
    let variants = crate::barcode::lookup_variants(code);
    if let Some(food) = custom
        .iter()
        .find(|f| f.barcode.as_ref().is_some_and(|b| variants.contains(b)))
    {
        return Some(yours(food));
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(db) = db {
        for variant in &variants {
            if let Ok(Some(hit)) = db.by_barcode(variant) {
                return Some(FoundFood {
                    food: hit.food,
                    source: hit.dataset.label(),
                });
            }
        }
    }
    let _ = (db, variants);
    None
}

/// Data credits recorded in the database, if any.
pub fn attribution(db: Option<&Database>) -> Option<String> {
    #[cfg(not(target_arch = "wasm32"))]
    return db.and_then(|db| db.meta("attribution").ok().flatten());
    #[cfg(target_arch = "wasm32")]
    {
        let _ = db;
        None
    }
}

/// Number of foods available to search.
pub fn food_count(db: Option<&Database>) -> usize {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(count) = db.and_then(|db| db.count().ok()) {
        return count as usize;
    }
    let _ = db;
    FOODS.len()
}

#[cfg(all(not(target_arch = "wasm32"), feature = "bundled-foods"))]
mod bundled {
    use dioxus::prelude::*;

    /// Built with `build-food-db`; see docs/FOOD_DATA.md.
    pub static FOOD_DB: Asset = asset!("/assets/foods.db");
}

/// Copies the bundled food database into a private folder that isn't backed
/// up (once per build of the database) and opens it. `None` when no
/// database is bundled.
pub async fn open_bundled() -> Option<Database> {
    #[cfg(all(not(target_arch = "wasm32"), feature = "bundled-foods"))]
    {
        use std::hash::{Hash, Hasher};

        use dioxus::logger::tracing::warn;

        let dir = crate::persist::no_backup_dir()?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bundled::FOOD_DB.to_string().hash(&mut hasher);
        let name = format!("foods-{:016x}.db", hasher.finish());
        let path = dir.join(&name);
        if !path.exists() {
            let bytes = dioxus::asset_resolver::read_asset_bytes(&bundled::FOOD_DB)
                .await
                .map_err(|e| warn!("could not read the bundled food database: {e}"))
                .ok()?;
            std::fs::create_dir_all(&dir).ok()?;
            let partial = path.with_extension("partial");
            std::fs::write(&partial, &bytes)
                .and_then(|_| std::fs::rename(&partial, &path))
                .map_err(|e| warn!("could not install the food database: {e}"))
                .ok()?;
            // Remove copies from earlier builds, including ones that older
            // versions kept in the backed-up data folder.
            let old_dirs = [Some(dir.clone()), crate::persist::data_dir()];
            for old in old_dirs.iter().flatten() {
                for entry in std::fs::read_dir(old).into_iter().flatten().flatten() {
                    let file = entry.file_name().to_string_lossy().into_owned();
                    if file.starts_with("foods-") && entry.path() != path {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }
        return Database::open(&path)
            .map_err(|e| warn!("could not open the food database: {e}"))
            .ok();
    }
    #[allow(unreachable_code)]
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_fallback_without_database() {
        let results = search(None, &[], &HashMap::new(), "yogurt", FoodRegion::Australia);
        assert!(results.yours.is_empty());
        assert_eq!(results.common.len(), 1);
        assert_eq!(results.common[0].source, "Sample");
        assert!(results.branded.is_empty());
        assert_eq!(food_count(None), FOODS.len());
    }

    /// Runs against a locally built database when one exists
    /// (`data/foods-full.db`); skipped otherwise, e.g. in CI.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn real_database_sections_and_regions() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/foods-full.db");
        let Ok(db) = Database::open(&path) else {
            return;
        };
        if db.count().unwrap_or(0) == 0 {
            return;
        }
        let au = search(
            Some(&db),
            &[],
            &HashMap::new(),
            "weet bix",
            FoodRegion::Australia,
        );
        assert!(!au.branded.is_empty(), "Australian branded cereal expected");
        assert!(au.branded.iter().all(|f| f.source == "Open Food Facts"));
        assert!(au.common.iter().any(|f| f.source == "AFCD"));

        let us = search(
            Some(&db),
            &[],
            &HashMap::new(),
            "greek yoghurt",
            FoodRegion::UnitedStates,
        );
        assert!(
            us.common
                .iter()
                .any(|f| f.food.name.to_lowercase().contains("yogurt"))
        );
        assert!(
            attribution(Some(&db))
                .unwrap()
                .contains("Food Standards Australia New Zealand")
        );
    }

    #[test]
    fn custom_foods_match_and_win_barcode_lookups() {
        let mine = Food {
            id: "custom:1".into(),
            name: "Mum's lasagne".into(),
            brand: Some("Home".into()),
            source: FoodSource::Custom,
            barcode: Some("041196910759".into()),
            per_100g: Default::default(),
            servings: Vec::new(),
        };
        let custom = [mine.clone()];
        assert_eq!(search_custom(&custom, "lasagne home").len(), 1);
        assert!(search_custom(&custom, "pizza").is_empty());
        let found = by_barcode(None, &custom, "0041196910759").unwrap();
        assert_eq!(found.food, mine);
        assert!(by_barcode(None, &custom, "9300652010053").is_none());
    }

    fn logged(id: i64, day: u32, food_id: &str, amount: &str) -> LogEntry {
        LogEntry {
            id,
            date: chrono::NaiveDate::from_ymd_opt(2026, 10, day).unwrap(),
            hour: 8,
            name: food_id.to_string(),
            amount: amount.to_string(),
            nutrients: openmacro_core::nutrients::NutrientProfile::from_macros(
                150.0, 10.0, 5.0, 15.0,
            ),
            food_id: Some(food_id.to_string()),
        }
    }

    #[test]
    fn recents_are_newest_first_without_repeats() {
        let log = [
            logged(1, 10, "sample:oats", "1 cup (80 g)"),
            logged(2, 12, "sample:banana", "1 medium (118 g)"),
            logged(3, 14, "sample:oats", "0.5 cup (40 g)"),
            logged(4, 13, "off:000000", "1 serving (30 g)"),
        ];
        let recent = recent_foods(None, &[], &log, 10);
        let ids: Vec<&str> = recent.iter().map(|f| f.food.id.as_str()).collect();
        assert_eq!(ids, ["sample:oats", "off:000000", "sample:banana"]);
        // The online product isn't available offline, so it is rebuilt from the entry.
        assert_eq!(recent[1].source, "Logged");
        assert_eq!(
            recent[1]
                .food
                .per_100g
                .get(openmacro_core::nutrients::Nutrient::Energy),
            Some(500.0)
        );
        assert_eq!(recent_foods(None, &[], &log, 1).len(), 1);
    }

    #[test]
    fn history_moves_logged_foods_up() {
        let plain = search(None, &[], &HashMap::new(), "rice", FoodRegion::Australia);
        assert_eq!(plain.common[0].food.id, "sample:brown-rice");
        let log = [
            logged(1, 10, "sample:white-rice", "1 cup (158 g)"),
            logged(2, 11, "sample:white-rice", "1 cup (158 g)"),
        ];
        let results = search(
            None,
            &[],
            &history_counts(&log),
            "rice",
            FoodRegion::Australia,
        );
        assert_eq!(results.common[0].food.id, "sample:white-rice");
    }

    #[test]
    fn grams_are_read_from_amount_labels() {
        assert_eq!(grams_in("1.5 large (75 g)"), Some(75.0));
        assert_eq!(grams_in("250 g"), Some(250.0));
        assert_eq!(grams_in("2 oz (56.7 g)"), Some(56.7));
        assert_eq!(grams_in("1 entry"), None);
    }

    #[test]
    fn region_codes_round_trip() {
        for r in FoodRegion::ALL {
            assert_eq!(FoodRegion::from_code(r.code()), Some(r));
        }
    }
}
