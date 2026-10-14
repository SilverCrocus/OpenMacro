//! The food database: generic foods (USDA FoodData Central, the Australian
//! Food Composition Database) and packaged products (Open Food Facts) in one
//! SQLite file with full-text search.
//!
//! Nutrients are stored per 100 g, one column per [`Nutrient`]. Carbohydrate
//! is always *total* carbohydrate including fibre (the US label convention);
//! importers for sources that report available carbohydrate add fibre back.

use std::path::Path;

use openmacro_core::food::{Food, FoodSource, Serving};
use openmacro_core::nutrients::{Nutrient, NutrientProfile};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Row, params, params_from_iter};

#[cfg(feature = "import")]
pub mod import;

pub type Result<T> = rusqlite::Result<T>;

/// Where a food's data comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dataset {
    UsdaFoundation,
    UsdaFndds,
    UsdaSrLegacy,
    Afcd,
    OpenFoodFacts,
}

impl Dataset {
    pub const ALL: [Dataset; 5] = [
        Dataset::UsdaFoundation,
        Dataset::UsdaFndds,
        Dataset::UsdaSrLegacy,
        Dataset::Afcd,
        Dataset::OpenFoodFacts,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Dataset::UsdaFoundation => "usda_foundation",
            Dataset::UsdaFndds => "usda_fndds",
            Dataset::UsdaSrLegacy => "usda_sr_legacy",
            Dataset::Afcd => "afcd",
            Dataset::OpenFoodFacts => "off",
        }
    }

    pub fn from_code(code: &str) -> Option<Dataset> {
        Dataset::ALL.into_iter().find(|d| d.code() == code)
    }

    /// Short label to show next to a search result.
    pub fn label(self) -> &'static str {
        match self {
            Dataset::UsdaFoundation | Dataset::UsdaFndds | Dataset::UsdaSrLegacy => "USDA",
            Dataset::Afcd => "AFCD",
            Dataset::OpenFoodFacts => "Open Food Facts",
        }
    }
}

/// Bit flags for the regions a food is sold or relevant in.
pub mod region {
    pub const AU: u8 = 1;
    pub const US: u8 = 2;
    pub const ALL: u8 = AU | US;
}

/// A food as stored in the database.
#[derive(Debug, Clone, PartialEq)]
pub struct FoodRecord {
    pub dataset: Dataset,
    pub source_id: String,
    pub name: String,
    pub brand: Option<String>,
    pub barcode: Option<String>,
    /// [`region`] flags.
    pub regions: u8,
    pub branded: bool,
    /// Relative popularity (e.g. scan counts); 0 when unknown.
    pub popularity: f64,
    pub per_100g: NutrientProfile,
    pub servings: Vec<Serving>,
}

/// A search result or lookup. `food.id` is the stable key
/// `<dataset>:<source id>` (e.g. `afcd:F002893`), which survives rebuilding
/// the database; `row_id` is only valid for this build.
#[derive(Debug, Clone, PartialEq)]
pub struct FoodHit {
    pub food: Food,
    pub row_id: i64,
    pub dataset: Dataset,
    pub regions: u8,
}

fn nutrient_columns() -> String {
    Nutrient::ALL
        .iter()
        .map(|n| format!("n_{} REAL", n.key()))
        .collect::<Vec<_>>()
        .join(",\n    ")
}

fn schema() -> String {
    format!(
        "CREATE TABLE food (
            id INTEGER PRIMARY KEY,
            dataset TEXT NOT NULL,
            source_id TEXT NOT NULL,
            name TEXT NOT NULL,
            brand TEXT,
            barcode TEXT,
            regions INTEGER NOT NULL,
            branded INTEGER NOT NULL,
            popularity REAL NOT NULL,
            listed INTEGER NOT NULL DEFAULT 1,
            {}
        );
        CREATE TABLE serving (
            food_id INTEGER NOT NULL,
            position INTEGER NOT NULL,
            label TEXT NOT NULL,
            grams REAL NOT NULL,
            PRIMARY KEY (food_id, position)
        ) WITHOUT ROWID;
        CREATE VIRTUAL TABLE food_fts USING fts5(
            name, brand,
            content = 'food', content_rowid = 'id',
            tokenize = 'unicode61 remove_diacritics 2',
            prefix = '2 3'
        );
        CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        nutrient_columns()
    )
}

/// Columns read for every hit, in order.
fn select_columns() -> String {
    let nutrients: Vec<String> = Nutrient::ALL
        .iter()
        .map(|n| format!("f.n_{}", n.key()))
        .collect();
    format!(
        "f.id, f.dataset, f.name, f.brand, f.barcode, f.regions, f.branded, f.source_id, {}",
        nutrients.join(", ")
    )
}

/// Builds a new food database. Everything happens in one transaction that
/// [`FoodDbWriter::finish`] commits.
pub struct FoodDbWriter {
    conn: Connection,
}

impl FoodDbWriter {
    /// Creates a fresh database at `path`, replacing any existing file.
    pub fn create(path: &Path) -> Result<Self> {
        let _ = std::fs::remove_file(path);
        Self::init(Connection::open(path)?)
    }

    pub fn in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF; PRAGMA page_size = 4096;",
        )?;
        conn.execute_batch(&schema())?;
        conn.execute_batch("BEGIN")?;
        Ok(Self { conn })
    }

    pub fn insert(&mut self, food: &FoodRecord) -> Result<i64> {
        let keys: Vec<String> = Nutrient::ALL
            .iter()
            .map(|n| format!("n_{}", n.key()))
            .collect();
        let placeholders: Vec<String> = (9..9 + keys.len()).map(|i| format!("?{i}")).collect();
        let sql = format!(
            "INSERT INTO food (dataset, source_id, name, brand, barcode, regions, branded, popularity, {})
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, {})",
            keys.join(", "),
            placeholders.join(", ")
        );
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let mut values: Vec<rusqlite::types::Value> = vec![
            food.dataset.code().to_string().into(),
            food.source_id.clone().into(),
            food.name.clone().into(),
            food.brand.clone().into(),
            food.barcode.clone().into(),
            i64::from(food.regions).into(),
            i64::from(food.branded).into(),
            food.popularity.into(),
        ];
        for n in Nutrient::ALL {
            values.push(food.per_100g.get(*n).into());
        }
        stmt.execute(params_from_iter(values))?;
        let id = self.conn.last_insert_rowid();

        let mut serving = self.conn.prepare_cached(
            "INSERT INTO serving (food_id, position, label, grams) VALUES (?1, ?2, ?3, ?4)",
        )?;
        for (i, s) in food.servings.iter().enumerate() {
            serving.execute(params![id, i as i64, s.label, s.grams])?;
        }
        Ok(id)
    }

    /// Indexes, records attribution and commits. Returns the open database.
    pub fn finish(self, meta: &[(&str, &str)]) -> Result<Connection> {
        {
            let mut stmt = self
                .conn
                .prepare("INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)")?;
            for (k, v) in meta {
                stmt.execute(params![k, v])?;
            }
        }
        self.conn.execute_batch(
            "-- Several barcodes (pack sizes) often share one name and brand. Keep
             -- them all for barcode lookup but list only the most-scanned in search.
             UPDATE food SET listed = 0
             WHERE branded = 1 AND id NOT IN (
                 SELECT id FROM (
                     SELECT id, ROW_NUMBER() OVER (
                         PARTITION BY lower(name), lower(coalesce(brand, ''))
                         ORDER BY popularity DESC, id
                     ) AS rank
                     FROM food WHERE branded = 1
                 ) WHERE rank = 1
             );
             CREATE INDEX food_barcode ON food (barcode) WHERE barcode IS NOT NULL;
             CREATE INDEX food_key ON food (dataset, source_id);
             INSERT INTO food_fts (food_fts) VALUES ('rebuild');
             INSERT INTO food_fts (food_fts) VALUES ('optimize');
             COMMIT;
             ANALYZE;",
        )?;
        Ok(self.conn)
    }
}

/// Read-only access to a built food database.
pub struct FoodDb {
    conn: Connection,
}

/// Australian/British and American names for the same food, so either
/// spelling finds both datasets.
const VARIANTS: &[&[&str]] = &[
    &["yoghurt", "yogurt"],
    &["flavoured", "flavored"],
    &["fibre", "fiber"],
    &["zucchini", "courgette"],
    &["eggplant", "aubergine"],
    &["coriander", "cilantro"],
    &["prawn", "shrimp"],
    &["prawns", "shrimp"],
    &["capsicum", "pepper"],
    &["rockmelon", "cantaloupe"],
    &["porridge", "oatmeal"],
    &["mince", "ground"],
    &["biscuit", "cookie"],
    &["biscuits", "cookies"],
    &["chickpea", "garbanzo"],
    &["chickpeas", "garbanzo"],
    &["lollies", "candy"],
];

/// Turns free text into an FTS5 query where every word must match as a
/// prefix, allowing regional spelling variants.
fn fts_query(text: &str) -> Option<String> {
    let terms: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| {
            let w = w.to_lowercase();
            let mut forms = vec![w.clone()];
            for group in VARIANTS.iter().filter(|g| g.contains(&w.as_str())) {
                forms.extend(group.iter().filter(|v| **v != w).map(|v| v.to_string()));
            }
            let quoted: Vec<String> = forms.iter().map(|f| format!("\"{f}\"*")).collect();
            if quoted.len() == 1 {
                quoted[0].clone()
            } else {
                format!("({})", quoted.join(" OR "))
            }
        })
        .collect();
    (!terms.is_empty()).then(|| terms.join(" AND "))
}

impl FoodDb {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        Ok(Self { conn })
    }

    pub fn from_connection(conn: Connection) -> Self {
        Self { conn }
    }

    pub fn count(&self) -> Result<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM food", [], |r| r.get(0))
    }

    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        self.conn
            .query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| {
                r.get(0)
            })
            .optional()
    }

    /// Foods matching every word of `query`, best first.
    ///
    /// Generic foods from any region are included, with the user's region
    /// ranked higher; branded products are limited to the user's `regions`.
    pub fn search(&self, query: &str, regions: u8, limit: usize) -> Result<Vec<FoodHit>> {
        let Some(fts) = fts_query(query) else {
            return Ok(Vec::new());
        };
        let sql = format!(
            "SELECT {} FROM food_fts
             JOIN food f ON f.id = food_fts.rowid
             WHERE food_fts MATCH ?1
               AND f.listed = 1
               AND (f.branded = 0 OR (f.regions & ?2) != 0)
             ORDER BY bm25(food_fts, 10.0, 3.0)
                 - CASE WHEN (f.regions & ?2) != 0 THEN 1.5 ELSE 0.0 END
                 - CASE WHEN f.branded = 0 THEN 1.5 ELSE 0.0 END
                 - CASE WHEN f.dataset = 'usda_fndds' THEN 1.0 ELSE 0.0 END
                 - CASE WHEN lower(f.name) LIKE ?4 THEN 2.0 ELSE 0.0 END
                 - min(f.popularity, 10000.0) / 10000.0 * 2.0
                 + length(f.name) * 0.03
             LIMIT ?3",
            select_columns()
        );
        let mut stmt = self.conn.prepare_cached(&sql)?;
        // Names that start with the first word typed are usually what people mean.
        let first_word = query
            .split(|c: char| !c.is_alphanumeric())
            .find(|w| !w.is_empty())
            .unwrap_or_default()
            .to_lowercase();
        let prefix = format!("{first_word}%");
        let rows = stmt.query_map(
            params![fts, i64::from(regions), limit as i64, prefix],
            read_hit,
        )?;
        let mut hits = rows.collect::<Result<Vec<_>>>()?;
        for hit in &mut hits {
            hit.food.servings = self.servings(hit.row_id)?;
        }
        Ok(hits)
    }

    pub fn get(&self, id: i64) -> Result<Option<FoodHit>> {
        let sql = format!("SELECT {} FROM food f WHERE f.id = ?1", select_columns());
        let hit = self
            .conn
            .query_row(&sql, params![id], read_hit)
            .optional()?;
        hit.map(|mut h| {
            h.food.servings = self.servings(id)?;
            Ok(h)
        })
        .transpose()
    }

    /// Looks a food up by its stable key, `<dataset>:<source id>`.
    pub fn get_by_key(&self, key: &str) -> Result<Option<FoodHit>> {
        let Some((dataset, source_id)) = key.split_once(':') else {
            return Ok(None);
        };
        let id: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM food WHERE dataset = ?1 AND source_id = ?2 LIMIT 1",
                params![dataset, source_id],
                |r| r.get(0),
            )
            .optional()?;
        id.map(|id| self.get(id)).transpose().map(Option::flatten)
    }

    pub fn by_barcode(&self, barcode: &str) -> Result<Option<FoodHit>> {
        let id: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM food WHERE barcode = ?1 LIMIT 1",
                params![barcode],
                |r| r.get(0),
            )
            .optional()?;
        id.map(|id| self.get(id)).transpose().map(Option::flatten)
    }

    fn servings(&self, food_id: i64) -> Result<Vec<Serving>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT label, grams FROM serving WHERE food_id = ?1 ORDER BY position",
        )?;
        let rows = stmt.query_map(params![food_id], |r| {
            Ok(Serving::new(r.get::<_, String>(0)?, r.get(1)?))
        })?;
        rows.collect()
    }
}

fn read_hit(row: &Row<'_>) -> Result<FoodHit> {
    let row_id: i64 = row.get(0)?;
    let dataset: String = row.get(1)?;
    let branded: bool = row.get(6)?;
    let source_id: String = row.get(7)?;
    let mut per_100g = NutrientProfile::new();
    for (i, n) in Nutrient::ALL.iter().enumerate() {
        if let Some(v) = row.get::<_, Option<f64>>(8 + i)? {
            per_100g.set(*n, v);
        }
    }
    Ok(FoodHit {
        food: Food {
            id: format!("{dataset}:{source_id}"),
            name: row.get(2)?,
            brand: row.get(3)?,
            source: if branded {
                FoodSource::Branded
            } else {
                FoodSource::Common
            },
            barcode: row.get(4)?,
            per_100g,
            servings: Vec::new(),
        },
        row_id,
        dataset: Dataset::from_code(&dataset).unwrap_or(Dataset::OpenFoodFacts),
        regions: row.get::<_, i64>(5)? as u8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(name: &str, dataset: Dataset, regions: u8, branded: bool, kcal: f64) -> FoodRecord {
        FoodRecord {
            dataset,
            source_id: name.to_string(),
            name: name.to_string(),
            brand: branded.then(|| "Acme".to_string()),
            barcode: branded.then(|| format!("93{:011}", name.bytes().map(u64::from).sum::<u64>())),
            regions,
            branded,
            popularity: 0.0,
            per_100g: NutrientProfile::from_macros(kcal, 10.0, 1.0, 5.0)
                .with(Nutrient::Calcium, 120.0),
            servings: vec![Serving::new("container", 170.0)],
        }
    }

    fn db() -> FoodDb {
        let mut w = FoodDbWriter::in_memory().unwrap();
        w.insert(&record(
            "Greek yogurt, plain, nonfat",
            Dataset::UsdaFndds,
            region::US,
            false,
            59.0,
        ))
        .unwrap();
        w.insert(&record(
            "Yoghurt, Greek style, plain",
            Dataset::Afcd,
            region::AU,
            false,
            97.0,
        ))
        .unwrap();
        w.insert(&record(
            "Greek Yogurt Vanilla",
            Dataset::OpenFoodFacts,
            region::US,
            true,
            90.0,
        ))
        .unwrap();
        w.insert(&record(
            "Greek Yoghurt Honey",
            Dataset::OpenFoodFacts,
            region::AU,
            true,
            110.0,
        ))
        .unwrap();
        FoodDb::from_connection(w.finish(&[("attribution", "test")]).unwrap())
    }

    #[test]
    fn duplicate_products_are_listed_once_but_keep_barcodes() {
        let mut w = FoodDbWriter::in_memory().unwrap();
        let mut small = record("Weet-Bix", Dataset::OpenFoodFacts, region::AU, true, 350.0);
        small.barcode = Some("9300652000000".into());
        small.popularity = 3.0;
        let mut large = small.clone();
        large.barcode = Some("9300652999999".into());
        large.popularity = 40.0;
        w.insert(&small).unwrap();
        w.insert(&large).unwrap();
        let db = FoodDb::from_connection(w.finish(&[]).unwrap());

        let hits = db.search("weet bix", region::AU, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].food.barcode.as_deref(), Some("9300652999999"));
        assert!(db.by_barcode("9300652000000").unwrap().is_some());
    }

    #[test]
    fn fts_query_escapes_and_prefixes() {
        assert_eq!(
            fts_query("greek yog").as_deref(),
            Some(r#""greek"* AND "yog"*"#)
        );
        assert_eq!(fts_query(r#"a"b"#).as_deref(), Some(r#""a"* AND "b"*"#));
        assert_eq!(fts_query("  ,, "), None);
        assert_eq!(
            fts_query("greek yoghurt").as_deref(),
            Some(r#""greek"* AND ("yoghurt"* OR "yogurt"*)"#)
        );
    }

    #[test]
    fn search_respects_region_and_prefixes() {
        let db = db();
        assert_eq!(db.count().unwrap(), 4);
        let names = |regions| {
            db.search("greek yog", regions, 10)
                .unwrap()
                .into_iter()
                .map(|h| h.food.name)
                .collect::<Vec<_>>()
        };
        let au = names(region::AU);
        // Branded US products are hidden in Australia; generic US foods still show.
        assert!(au.contains(&"Greek Yoghurt Honey".to_string()));
        assert!(!au.contains(&"Greek Yogurt Vanilla".to_string()));
        assert!(au.contains(&"Greek yogurt, plain, nonfat".to_string()));
        assert!(au.contains(&"Yoghurt, Greek style, plain".to_string()));

        let us = names(region::US);
        assert_eq!(us[0], "Greek yogurt, plain, nonfat");
        assert!(!us.contains(&"Greek Yoghurt Honey".to_string()));
    }

    #[test]
    fn lookups_return_nutrients_and_servings() {
        let db = db();
        let hit = db
            .search("yoghurt greek style", region::AU, 1)
            .unwrap()
            .remove(0);
        assert_eq!(hit.dataset, Dataset::Afcd);
        assert_eq!(hit.food.per_100g.get(Nutrient::Calcium), Some(120.0));
        assert_eq!(hit.food.per_100g.get(Nutrient::Iron), None);
        assert_eq!(hit.food.servings, vec![Serving::new("container", 170.0)]);

        let by_code = db
            .by_barcode(hit.food.barcode.as_deref().unwrap_or("none"))
            .unwrap();
        assert!(by_code.is_none(), "generic foods have no barcode");
        let branded = db.search("honey", region::AU, 1).unwrap().remove(0);
        let found = db
            .by_barcode(branded.food.barcode.as_deref().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(found.food.name, "Greek Yoghurt Honey");
        assert_eq!(db.meta("attribution").unwrap().as_deref(), Some("test"));

        // Stable keys survive rebuilds and find the same food.
        assert_eq!(found.food.id, "off:Greek Yoghurt Honey");
        let again = db.get_by_key(&found.food.id).unwrap().unwrap();
        assert_eq!(again.food, found.food);
        assert!(db.get_by_key("afcd:missing").unwrap().is_none());
        assert!(db.get_by_key("no-colon").unwrap().is_none());
    }
}
