//! SQLite persistence for the user's own data: food log, weigh-ins, day
//! status and settings (profile, goal, program).
//!
//! Every user row carries `updated_at`, and food log entries and weigh-ins are
//! soft-deleted (`deleted_at`), so an optional sync server can merge per row
//! later without a schema change. The schema version lives in SQLite's
//! `user_version` pragma.

use std::collections::BTreeMap;
use std::path::Path;

use chrono::{NaiveDate, Utc};
use openmacro_core::checkin::CheckIn;
use openmacro_core::expenditure::DayStatus;
use openmacro_core::food::{Food, FoodSource, Serving};
use openmacro_core::log::LogEntry;
use openmacro_core::nutrients::NutrientProfile;
use openmacro_core::recipe::Recipe;
use openmacro_core::trend::WeighIn;
use rusqlite::{Connection, OptionalExtension, params};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub use rusqlite::Error as SqliteError;

#[derive(Debug)]
pub enum Error {
    Sqlite(rusqlite::Error),
    Json(serde_json::Error),
    /// A stored value that doesn't parse, e.g. a malformed date.
    Corrupt(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Sqlite(e) => write!(f, "database error: {e}"),
            Error::Json(e) => write!(f, "stored JSON is invalid: {e}"),
            Error::Corrupt(what) => write!(f, "stored data is invalid: {what}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Sqlite(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// One of the user's own foods with its stored id (which log entries and
/// favorites refer to as `custom:<id>`), and its ingredients if a recipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedFood {
    pub id: i64,
    pub food: Food,
    #[serde(default)]
    pub recipe: Option<Recipe>,
}

/// Everything the user has saved, for backups. Settings are kept as raw
/// JSON by key so a backup carries settings this version doesn't know.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Snapshot {
    pub log: Vec<LogEntry>,
    pub weigh_ins: Vec<WeighIn>,
    pub day_status: BTreeMap<NaiveDate, DayStatus>,
    pub check_ins: Vec<CheckIn>,
    pub custom_foods: Vec<SavedFood>,
    pub favorites: Vec<Food>,
    pub settings: BTreeMap<String, serde_json::Value>,
}

/// Ordered schema migrations; index + 1 is the version each one produces.
const MIGRATIONS: &[&str] = &[
    "
    CREATE TABLE log_entry (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        date TEXT NOT NULL,
        hour INTEGER NOT NULL,
        name TEXT NOT NULL,
        amount TEXT NOT NULL,
        nutrients TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        deleted_at TEXT
    );
    CREATE INDEX log_entry_date ON log_entry (date) WHERE deleted_at IS NULL;

    CREATE TABLE weigh_in (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        date TEXT NOT NULL,
        kg REAL NOT NULL,
        updated_at TEXT NOT NULL,
        deleted_at TEXT
    );

    CREATE TABLE day_status (
        date TEXT PRIMARY KEY,
        status TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );

    CREATE TABLE setting (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
",
    "
    CREATE TABLE custom_food (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        brand TEXT,
        barcode TEXT,
        nutrients TEXT NOT NULL,
        servings TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        deleted_at TEXT
    );
    CREATE INDEX custom_food_barcode ON custom_food (barcode) WHERE deleted_at IS NULL;
",
    "
    ALTER TABLE log_entry ADD COLUMN food_id TEXT;
",
    "
    CREATE TABLE favorite (
        food_id TEXT PRIMARY KEY,
        food TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        deleted_at TEXT
    );
",
    "
    CREATE TABLE check_in (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        date TEXT NOT NULL,
        data TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        deleted_at TEXT
    );
",
    "
    ALTER TABLE custom_food ADD COLUMN recipe TEXT;
",
];

pub struct Store {
    conn: Connection,
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn parse_date(text: &str) -> Result<NaiveDate> {
    text.parse()
        .map_err(|_| Error::Corrupt(format!("date {text:?}")))
}

fn status_name(status: DayStatus) -> &'static str {
    match status {
        DayStatus::Normal => "normal",
        DayStatus::Fasted => "fasted",
        DayStatus::Partial => "partial",
    }
}

fn parse_status(text: &str) -> Result<DayStatus> {
    match text {
        "normal" => Ok(DayStatus::Normal),
        "fasted" => Ok(DayStatus::Fasted),
        "partial" => Ok(DayStatus::Partial),
        other => Err(Error::Corrupt(format!("day status {other:?}"))),
    }
}

impl Store {
    /// Opens (creating if needed) the database at `path` and migrates it.
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        let mut store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn schema_version(&self) -> Result<usize> {
        let v: i64 = self
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))?;
        Ok(v as usize)
    }

    fn migrate(&mut self) -> Result<()> {
        let current = self.schema_version()?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(())
    }

    /// True if the user has never saved anything.
    pub fn is_empty(&self) -> Result<bool> {
        let rows: i64 = self.conn.query_row(
            "SELECT (SELECT COUNT(*) FROM log_entry) + (SELECT COUNT(*) FROM weigh_in)
                  + (SELECT COUNT(*) FROM setting)",
            [],
            |r| r.get(0),
        )?;
        Ok(rows == 0)
    }

    /// Inserts an entry and returns its new id (the entry's own `id` is ignored).
    pub fn insert_log_entry(&self, entry: &LogEntry) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO log_entry (date, hour, name, amount, nutrients, food_id, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                entry.date.to_string(),
                entry.hour,
                entry.name,
                entry.amount,
                serde_json::to_string(&entry.nutrients)?,
                entry.food_id,
                now()
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn delete_log_entry(&self, id: i64) -> Result<()> {
        let at = now();
        self.conn.execute(
            "UPDATE log_entry SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![at, id],
        )?;
        Ok(())
    }

    /// Every live entry, oldest first.
    pub fn log_entries(&self) -> Result<Vec<LogEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, date, hour, name, amount, nutrients, food_id FROM log_entry
             WHERE deleted_at IS NULL ORDER BY date, hour, id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, u32>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, Option<String>>(6)?,
            ))
        })?;
        rows.map(|row| {
            let (id, date, hour, name, amount, nutrients, food_id) = row?;
            Ok(LogEntry {
                id,
                date: parse_date(&date)?,
                hour,
                name,
                amount,
                nutrients: serde_json::from_str::<NutrientProfile>(&nutrients)?,
                food_id,
            })
        })
        .collect()
    }

    pub fn insert_weigh_in(&self, weigh_in: &WeighIn) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO weigh_in (date, kg, updated_at) VALUES (?1, ?2, ?3)",
            params![weigh_in.date.to_string(), weigh_in.kg, now()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn weigh_ins(&self) -> Result<Vec<WeighIn>> {
        let mut stmt = self
            .conn
            .prepare("SELECT date, kg FROM weigh_in WHERE deleted_at IS NULL ORDER BY date, id")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?)))?;
        rows.map(|row| {
            let (date, kg) = row?;
            Ok(WeighIn {
                date: parse_date(&date)?,
                kg,
            })
        })
        .collect()
    }

    /// Records a day's status; `Normal` removes any override.
    pub fn set_day_status(&self, date: NaiveDate, status: DayStatus) -> Result<()> {
        if status == DayStatus::Normal {
            self.conn.execute(
                "DELETE FROM day_status WHERE date = ?1",
                params![date.to_string()],
            )?;
        } else {
            self.conn.execute(
                "INSERT INTO day_status (date, status, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT (date) DO UPDATE SET status = excluded.status, updated_at = excluded.updated_at",
                params![date.to_string(), status_name(status), now()],
            )?;
        }
        Ok(())
    }

    pub fn day_statuses(&self) -> Result<BTreeMap<NaiveDate, DayStatus>> {
        let mut stmt = self.conn.prepare("SELECT date, status FROM day_status")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.map(|row| {
            let (date, status) = row?;
            Ok((parse_date(&date)?, parse_status(&status)?))
        })
        .collect()
    }

    pub fn set_setting<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.conn.execute(
            "INSERT INTO setting (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, serde_json::to_string(value)?, now()],
        )?;
        Ok(())
    }

    pub fn setting<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let json: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM setting WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?;
        Ok(json.map(|j| serde_json::from_str(&j)).transpose()?)
    }

    /// Saves a food the user created; returns its id. Nutrients are per 100 g.
    pub fn insert_custom_food(&self, food: &Food) -> Result<i64> {
        self.insert_food_row(food, None)
    }

    /// Saves a recipe as a custom food that keeps its ingredients; returns its id.
    pub fn insert_recipe(&self, recipe: &Recipe) -> Result<i64> {
        self.insert_food_row(&recipe.to_food(""), Some(serde_json::to_string(recipe)?))
    }

    fn insert_food_row(&self, food: &Food, recipe: Option<String>) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO custom_food (name, brand, barcode, nutrients, servings, recipe, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                food.name,
                food.brand,
                food.barcode,
                serde_json::to_string(&food.per_100g)?,
                serde_json::to_string(&food.servings)?,
                recipe,
                now()
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// The ingredients behind a recipe food, if `id` is a live recipe.
    pub fn recipe(&self, id: i64) -> Result<Option<Recipe>> {
        let json: Option<Option<String>> = self
            .conn
            .query_row(
                "SELECT recipe FROM custom_food WHERE id = ?1 AND deleted_at IS NULL",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(json
            .flatten()
            .map(|j| serde_json::from_str(&j))
            .transpose()?)
    }

    pub fn delete_custom_food(&self, id: i64) -> Result<()> {
        let at = now();
        self.conn.execute(
            "UPDATE custom_food SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![at, id],
        )?;
        Ok(())
    }

    /// Every live custom food, newest first, with ids of the form `custom:<id>`.
    pub fn custom_foods(&self) -> Result<Vec<Food>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, brand, barcode, nutrients, servings, recipe IS NOT NULL
             FROM custom_food WHERE deleted_at IS NULL ORDER BY id DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, bool>(6)?,
            ))
        })?;
        rows.map(|row| {
            let (id, name, brand, barcode, nutrients, servings, is_recipe) = row?;
            Ok(Food {
                id: format!("custom:{id}"),
                name,
                brand,
                source: if is_recipe {
                    FoodSource::Recipe
                } else {
                    FoodSource::Custom
                },
                barcode,
                per_100g: serde_json::from_str::<NutrientProfile>(&nutrients)?,
                servings: serde_json::from_str::<Vec<Serving>>(&servings)?,
            })
        })
        .collect()
    }

    /// Stars or unstars a food. A copy of the food is kept so the favorite
    /// still works offline or after the food database changes.
    pub fn set_favorite(&self, food: &Food, favorite: bool) -> Result<()> {
        let at = now();
        if favorite {
            self.conn.execute(
                "INSERT INTO favorite (food_id, food, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT (food_id) DO UPDATE SET food = excluded.food, updated_at = excluded.updated_at, deleted_at = NULL",
                params![food.id, serde_json::to_string(food)?, at],
            )?;
        } else {
            self.conn.execute(
                "UPDATE favorite SET deleted_at = ?1, updated_at = ?1 WHERE food_id = ?2",
                params![at, food.id],
            )?;
        }
        Ok(())
    }

    /// Favorite foods, most recently starred first.
    pub fn favorites(&self) -> Result<Vec<Food>> {
        let mut stmt = self.conn.prepare(
            "SELECT food FROM favorite WHERE deleted_at IS NULL ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|json| Ok(serde_json::from_str::<Food>(&json?)?))
            .collect()
    }

    pub fn insert_check_in(&self, check_in: &CheckIn) -> Result<()> {
        self.conn.execute(
            "INSERT INTO check_in (date, data, updated_at) VALUES (?1, ?2, ?3)",
            params![
                check_in.date.to_string(),
                serde_json::to_string(check_in)?,
                now()
            ],
        )?;
        Ok(())
    }

    /// Every check-in, oldest first.
    pub fn check_ins(&self) -> Result<Vec<CheckIn>> {
        let mut stmt = self
            .conn
            .prepare("SELECT data FROM check_in WHERE deleted_at IS NULL ORDER BY date, id")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|json| Ok(serde_json::from_str::<CheckIn>(&json?)?))
            .collect()
    }

    /// Replaces the whole food log, weigh-ins and day statuses in one
    /// transaction (used for loading demo data or clearing everything).
    /// Returns the log with its new ids.
    pub fn replace_history(
        &mut self,
        log: &[LogEntry],
        weigh_ins: &[WeighIn],
        statuses: &BTreeMap<NaiveDate, DayStatus>,
    ) -> Result<Vec<LogEntry>> {
        let tx = self.conn.transaction()?;
        tx.execute_batch(
            "DELETE FROM log_entry; DELETE FROM weigh_in; DELETE FROM day_status; DELETE FROM check_in;",
        )?;
        let at = now();
        let mut stored = Vec::with_capacity(log.len());
        {
            let mut insert_entry = tx.prepare(
                "INSERT INTO log_entry (date, hour, name, amount, nutrients, food_id, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for entry in log {
                insert_entry.execute(params![
                    entry.date.to_string(),
                    entry.hour,
                    entry.name,
                    entry.amount,
                    serde_json::to_string(&entry.nutrients)?,
                    entry.food_id,
                    at
                ])?;
                stored.push(LogEntry {
                    id: tx.last_insert_rowid(),
                    ..entry.clone()
                });
            }
            let mut insert_weight =
                tx.prepare("INSERT INTO weigh_in (date, kg, updated_at) VALUES (?1, ?2, ?3)")?;
            for w in weigh_ins {
                insert_weight.execute(params![w.date.to_string(), w.kg, at])?;
            }
            let mut insert_status = tx
                .prepare("INSERT INTO day_status (date, status, updated_at) VALUES (?1, ?2, ?3)")?;
            for (date, status) in statuses {
                if *status != DayStatus::Normal {
                    insert_status.execute(params![date.to_string(), status_name(*status), at])?;
                }
            }
        }
        tx.commit()?;
        Ok(stored)
    }

    /// Everything the user has saved.
    pub fn snapshot(&self) -> Result<Snapshot> {
        let custom_foods = self
            .custom_foods()?
            .into_iter()
            .rev()
            .map(|food| {
                let id = food
                    .id
                    .strip_prefix("custom:")
                    .and_then(|n| n.parse().ok())
                    .ok_or_else(|| Error::Corrupt(format!("custom food id {:?}", food.id)))?;
                Ok(SavedFood {
                    id,
                    recipe: self.recipe(id)?,
                    food,
                })
            })
            .collect::<Result<_>>()?;
        let mut stmt = self.conn.prepare("SELECT key, value FROM setting")?;
        let settings = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .map(|row| {
                let (key, json) = row?;
                Ok((key, serde_json::from_str(&json)?))
            })
            .collect::<Result<_>>()?;
        Ok(Snapshot {
            log: self.log_entries()?,
            weigh_ins: self.weigh_ins()?,
            day_status: self.day_statuses()?,
            check_ins: self.check_ins()?,
            custom_foods,
            favorites: self.favorites()?.into_iter().rev().collect(),
            settings,
        })
    }

    /// Replaces everything with `snapshot`, in one transaction. Log entry
    /// and custom food ids are kept, so references between them still work.
    pub fn restore(&mut self, snapshot: &Snapshot) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute_batch(
            "DELETE FROM log_entry; DELETE FROM weigh_in; DELETE FROM day_status;
             DELETE FROM check_in; DELETE FROM custom_food; DELETE FROM favorite;
             DELETE FROM setting;",
        )?;
        let at = now();
        {
            let mut insert_entry = tx.prepare(
                "INSERT INTO log_entry (id, date, hour, name, amount, nutrients, food_id, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for e in &snapshot.log {
                insert_entry.execute(params![
                    e.id,
                    e.date.to_string(),
                    e.hour,
                    e.name,
                    e.amount,
                    serde_json::to_string(&e.nutrients)?,
                    e.food_id,
                    at
                ])?;
            }
            let mut insert_weight =
                tx.prepare("INSERT INTO weigh_in (date, kg, updated_at) VALUES (?1, ?2, ?3)")?;
            for w in &snapshot.weigh_ins {
                insert_weight.execute(params![w.date.to_string(), w.kg, at])?;
            }
            let mut insert_status = tx
                .prepare("INSERT INTO day_status (date, status, updated_at) VALUES (?1, ?2, ?3)")?;
            for (date, status) in &snapshot.day_status {
                if *status != DayStatus::Normal {
                    insert_status.execute(params![date.to_string(), status_name(*status), at])?;
                }
            }
            let mut insert_check_in =
                tx.prepare("INSERT INTO check_in (date, data, updated_at) VALUES (?1, ?2, ?3)")?;
            for c in &snapshot.check_ins {
                insert_check_in.execute(params![
                    c.date.to_string(),
                    serde_json::to_string(c)?,
                    at
                ])?;
            }
            let mut insert_food = tx.prepare(
                "INSERT INTO custom_food (id, name, brand, barcode, nutrients, servings, recipe, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for saved in &snapshot.custom_foods {
                let f = &saved.food;
                insert_food.execute(params![
                    saved.id,
                    f.name,
                    f.brand,
                    f.barcode,
                    serde_json::to_string(&f.per_100g)?,
                    serde_json::to_string(&f.servings)?,
                    saved
                        .recipe
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()?,
                    at
                ])?;
            }
            // Oldest first, so the most recent favorite stays first.
            let mut insert_favorite = tx.prepare(
                "INSERT INTO favorite (food_id, food, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT (food_id) DO NOTHING",
            )?;
            for (i, food) in snapshot.favorites.iter().enumerate() {
                let stamp = format!("{at}#{i:06}");
                insert_favorite.execute(params![food.id, serde_json::to_string(food)?, stamp])?;
            }
            let mut insert_setting =
                tx.prepare("INSERT INTO setting (key, value, updated_at) VALUES (?1, ?2, ?3)")?;
            for (key, value) in &snapshot.settings {
                insert_setting.execute(params![key, value.to_string(), at])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openmacro_core::log::Goal;
    use openmacro_core::nutrients::Nutrient;

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    fn entry(day: u32, hour: u32, kcal: f64) -> LogEntry {
        LogEntry {
            id: 0,
            date: date(day),
            hour,
            name: "Greek yogurt".into(),
            amount: "1 container (170 g)".into(),
            nutrients: NutrientProfile::from_macros(kcal, 17.3, 0.7, 6.1)
                .with(Nutrient::Calcium, 189.0),
            food_id: Some("afcd:F009999".into()),
        }
    }

    #[test]
    fn fresh_database_is_migrated_and_empty() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.schema_version().unwrap(), MIGRATIONS.len());
        assert!(store.is_empty().unwrap());
    }

    #[test]
    fn reopening_keeps_data_and_does_not_remigrate() {
        let dir = std::env::temp_dir().join(format!("openmacro-store-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("user.db");
        {
            let store = Store::open(&path).unwrap();
            store.insert_log_entry(&entry(14, 8, 100.0)).unwrap();
        }
        let store = Store::open(&path).unwrap();
        assert_eq!(store.log_entries().unwrap().len(), 1);
        assert_eq!(store.schema_version().unwrap(), MIGRATIONS.len());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn log_entries_round_trip_and_soft_delete() {
        let store = Store::open_in_memory().unwrap();
        let a = store.insert_log_entry(&entry(14, 8, 100.0)).unwrap();
        let b = store.insert_log_entry(&entry(14, 12, 250.0)).unwrap();
        assert_ne!(a, b);

        let entries = store.log_entries().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0],
            LogEntry {
                id: a,
                ..entry(14, 8, 100.0)
            }
        );

        store.delete_log_entry(a).unwrap();
        let entries = store.log_entries().unwrap();
        assert_eq!(entries.iter().map(|e| e.id).collect::<Vec<_>>(), [b]);
        // Soft delete keeps the row for future sync.
        let rows: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM log_entry", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 2);
    }

    #[test]
    fn weigh_ins_and_day_status() {
        let store = Store::open_in_memory().unwrap();
        store
            .insert_weigh_in(&WeighIn {
                date: date(13),
                kg: 70.4,
            })
            .unwrap();
        store
            .insert_weigh_in(&WeighIn {
                date: date(14),
                kg: 70.1,
            })
            .unwrap();
        assert_eq!(
            store.weigh_ins().unwrap()[1],
            WeighIn {
                date: date(14),
                kg: 70.1
            }
        );

        store.set_day_status(date(13), DayStatus::Fasted).unwrap();
        store.set_day_status(date(13), DayStatus::Partial).unwrap();
        store.set_day_status(date(14), DayStatus::Fasted).unwrap();
        store.set_day_status(date(14), DayStatus::Normal).unwrap();
        let statuses = store.day_statuses().unwrap();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[&date(13)], DayStatus::Partial);
    }

    #[test]
    fn settings_round_trip() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.setting::<Goal>("goal").unwrap(), None);
        let goal = Goal {
            goal_weight_kg: 68.0,
            rate_pct_per_week: -0.5,
        };
        store.set_setting("goal", &goal).unwrap();
        store
            .set_setting(
                "goal",
                &Goal {
                    rate_pct_per_week: -0.25,
                    ..goal
                },
            )
            .unwrap();
        assert_eq!(
            store
                .setting::<Goal>("goal")
                .unwrap()
                .unwrap()
                .rate_pct_per_week,
            -0.25
        );
        assert!(!store.is_empty().unwrap());
    }

    #[test]
    fn custom_foods_round_trip_and_soft_delete() {
        let store = Store::open_in_memory().unwrap();
        let food = Food {
            id: String::new(),
            name: "Mum's lasagne".into(),
            brand: None,
            source: FoodSource::Custom,
            barcode: Some("9300000000017".into()),
            per_100g: NutrientProfile::from_macros(160.0, 9.0, 7.0, 15.0),
            servings: vec![Serving::new("slice", 250.0)],
        };
        let id = store.insert_custom_food(&food).unwrap();
        let saved = store.custom_foods().unwrap();
        assert_eq!(
            saved,
            vec![Food {
                id: format!("custom:{id}"),
                ..food
            }]
        );
        store.delete_custom_food(id).unwrap();
        assert!(store.custom_foods().unwrap().is_empty());
    }

    #[test]
    fn recipes_are_custom_foods_that_keep_their_ingredients() {
        use openmacro_core::recipe::Ingredient;
        let store = Store::open_in_memory().unwrap();
        let recipe = Recipe {
            name: "Overnight oats".into(),
            servings: 2.0,
            cooked_grams: None,
            ingredients: vec![
                Ingredient {
                    name: "Oats".into(),
                    amount: "80 g".into(),
                    grams: Some(80.0),
                    nutrients: NutrientProfile::from_macros(300.0, 10.0, 6.0, 50.0),
                    food_id: Some("afcd:F006121".into()),
                },
                Ingredient {
                    name: "Milk".into(),
                    amount: "250 ml (258 g)".into(),
                    grams: Some(258.0),
                    nutrients: NutrientProfile::from_macros(160.0, 8.5, 8.0, 12.0),
                    food_id: None,
                },
            ],
        };
        let id = store.insert_recipe(&recipe).unwrap();
        let foods = store.custom_foods().unwrap();
        assert_eq!(foods, vec![recipe.to_food(format!("custom:{id}"))]);
        assert_eq!(foods[0].source, FoodSource::Recipe);
        assert_eq!(store.recipe(id).unwrap(), Some(recipe));

        // Plain custom foods have no recipe.
        let plain = store.insert_custom_food(&foods[0]).unwrap();
        assert_eq!(store.recipe(plain).unwrap(), None);
        store.delete_custom_food(id).unwrap();
        assert_eq!(store.recipe(id).unwrap(), None);
    }

    #[test]
    fn snapshot_restores_into_a_fresh_store() {
        use openmacro_core::recipe::Ingredient;
        use openmacro_core::targets::MacroTargets;

        let source = Store::open_in_memory().unwrap();
        let first = source.insert_log_entry(&entry(12, 8, 400.0)).unwrap();
        source.delete_log_entry(first).unwrap();
        let soup = Recipe {
            name: "Soup".into(),
            servings: 3.0,
            cooked_grams: Some(900.0),
            ingredients: vec![Ingredient {
                name: "Lentils".into(),
                amount: "200 g".into(),
                grams: Some(200.0),
                nutrients: NutrientProfile::from_macros(700.0, 50.0, 2.0, 100.0),
                food_id: None,
            }],
        };
        let soup_id = source.insert_recipe(&soup).unwrap();
        let mut logged = entry(13, 12, 233.0);
        logged.food_id = Some(format!("custom:{soup_id}"));
        source.insert_log_entry(&logged).unwrap();
        source
            .insert_weigh_in(&WeighIn {
                date: date(13),
                kg: 71.2,
            })
            .unwrap();
        source.set_day_status(date(12), DayStatus::Fasted).unwrap();
        source
            .insert_check_in(&CheckIn {
                date: date(13),
                expenditure_kcal: 2300.0,
                trend_kg: Some(71.0),
                week: vec![MacroTargets {
                    kcal: 1900.0,
                    protein_g: 120.0,
                    fat_g: 63.0,
                    carbs_g: 212.0,
                }],
            })
            .unwrap();
        let soup_food = source.custom_foods().unwrap().remove(0);
        source.set_favorite(&soup_food, true).unwrap();
        source
            .set_setting(
                "goal",
                &Goal {
                    goal_weight_kg: 68.0,
                    rate_pct_per_week: -0.5,
                },
            )
            .unwrap();

        let snapshot = source.snapshot().unwrap();
        assert_eq!(snapshot.log.len(), 1);
        assert_eq!(snapshot.custom_foods[0].recipe, Some(soup.clone()));

        // Through JSON, as in a backup file, into a store with other data.
        let json = serde_json::to_string(&snapshot).unwrap();
        let mut target = Store::open_in_memory().unwrap();
        target.insert_log_entry(&entry(1, 9, 999.0)).unwrap();
        target
            .restore(&serde_json::from_str(&json).unwrap())
            .unwrap();
        assert_eq!(target.snapshot().unwrap(), snapshot);
        // The log still points at the recipe, under the same id.
        let restored = target.log_entries().unwrap();
        assert_eq!(restored[0].food_id, Some(format!("custom:{soup_id}")));
        assert_eq!(target.recipe(soup_id).unwrap(), Some(soup));
        // New rows don't collide with restored ids.
        let next = target.insert_log_entry(&entry(14, 8, 100.0)).unwrap();
        assert!(next > restored[0].id);
    }

    #[test]
    fn favorites_star_unstar_and_restar() {
        let store = Store::open_in_memory().unwrap();
        let food = Food {
            id: "afcd:F002893".into(),
            name: "Banana, cavendish, peeled, raw".into(),
            brand: None,
            source: FoodSource::Common,
            barcode: None,
            per_100g: NutrientProfile::from_macros(93.0, 1.3, 0.3, 23.0),
            servings: vec![Serving::new("medium", 118.0)],
        };
        store.set_favorite(&food, true).unwrap();
        store.set_favorite(&food, true).unwrap();
        assert_eq!(store.favorites().unwrap(), vec![food.clone()]);
        store.set_favorite(&food, false).unwrap();
        assert!(store.favorites().unwrap().is_empty());
        store.set_favorite(&food, true).unwrap();
        assert_eq!(store.favorites().unwrap().len(), 1);
    }

    #[test]
    fn check_ins_round_trip_in_date_order() {
        use openmacro_core::targets::MacroTargets;
        let store = Store::open_in_memory().unwrap();
        let targets = MacroTargets {
            kcal: 1800.0,
            protein_g: 120.0,
            fat_g: 60.0,
            carbs_g: 190.0,
        };
        let later = CheckIn {
            date: date(19),
            expenditure_kcal: 2250.0,
            trend_kg: Some(70.1),
            week: vec![targets; 7],
        };
        let earlier = CheckIn {
            date: date(12),
            expenditure_kcal: 2300.0,
            trend_kg: None,
            week: vec![targets; 7],
        };
        store.insert_check_in(&later).unwrap();
        store.insert_check_in(&earlier).unwrap();
        assert_eq!(store.check_ins().unwrap(), vec![earlier, later]);
    }

    #[test]
    fn replace_history_swaps_everything_atomically() {
        let mut store = Store::open_in_memory().unwrap();
        store.insert_log_entry(&entry(1, 8, 1.0)).unwrap();
        store
            .insert_weigh_in(&WeighIn {
                date: date(1),
                kg: 80.0,
            })
            .unwrap();

        let statuses = BTreeMap::from([(date(14), DayStatus::Fasted)]);
        let stored = store
            .replace_history(
                &[entry(14, 8, 100.0), entry(14, 9, 50.0)],
                &[WeighIn {
                    date: date(14),
                    kg: 70.0,
                }],
                &statuses,
            )
            .unwrap();

        assert_eq!(stored.len(), 2);
        assert_eq!(store.log_entries().unwrap(), stored);
        assert_eq!(
            store.weigh_ins().unwrap(),
            [WeighIn {
                date: date(14),
                kg: 70.0
            }]
        );
        assert_eq!(store.day_statuses().unwrap(), statuses);
    }
}
