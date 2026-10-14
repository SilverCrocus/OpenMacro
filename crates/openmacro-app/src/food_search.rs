//! Food search: the full food database on native builds when it is bundled,
//! otherwise the small built-in sample list.

use openmacro_core::food::Food;
#[cfg(not(target_arch = "wasm32"))]
use openmacro_core::food::FoodSource;

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

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Results {
    pub common: Vec<FoundFood>,
    pub branded: Vec<FoundFood>,
}

impl Results {
    pub fn is_empty(&self) -> bool {
        self.common.is_empty() && self.branded.is_empty()
    }
}

#[cfg(not(target_arch = "wasm32"))]
const PER_SECTION: usize = 15;

/// Searches the database if there is one, otherwise the sample foods.
pub fn search(db: Option<&Database>, query: &str, region: FoodRegion) -> Results {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(db) = db {
        let hits = db
            .search(query, region.flags(), PER_SECTION * 3)
            .unwrap_or_default();
        let mut results = Results::default();
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

/// Copies the bundled food database into the app's data folder (once per
/// build of the database) and opens it. `None` when no database is bundled.
pub async fn open_bundled() -> Option<Database> {
    #[cfg(all(not(target_arch = "wasm32"), feature = "bundled-foods"))]
    {
        use std::hash::{Hash, Hasher};

        use dioxus::logger::tracing::warn;

        let dir = crate::persist::data_dir()?;
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
            // Remove copies from earlier builds.
            for entry in std::fs::read_dir(&dir).ok()?.flatten() {
                let file = entry.file_name().to_string_lossy().into_owned();
                if file.starts_with("foods-") && file != name {
                    let _ = std::fs::remove_file(entry.path());
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
        let results = search(None, "yogurt", FoodRegion::Australia);
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
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/foods-full.db");
        let Ok(db) = Database::open(&path) else { return };
        if db.count().unwrap_or(0) == 0 {
            return;
        }
        let au = search(Some(&db), "weet bix", FoodRegion::Australia);
        assert!(!au.branded.is_empty(), "Australian branded cereal expected");
        assert!(au.branded.iter().all(|f| f.source == "Open Food Facts"));
        assert!(au.common.iter().any(|f| f.source == "AFCD"));

        let us = search(Some(&db), "greek yoghurt", FoodRegion::UnitedStates);
        assert!(us.common.iter().any(|f| f.food.name.to_lowercase().contains("yogurt")));
        assert!(attribution(Some(&db)).unwrap().contains("Food Standards Australia New Zealand"));
    }

    #[test]
    fn region_codes_round_trip() {
        for r in FoodRegion::ALL {
            assert_eq!(FoodRegion::from_code(r.code()), Some(r));
        }
    }
}
