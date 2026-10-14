//! Exporting the user's history as CSV files.
//!
//! Three files: every food log entry with its nutrients, every weigh-in, and
//! one row per day with intake, weight trend, expenditure and targets.
//! Android saves them to Downloads/OpenMacro, the web preview downloads them,
//! and desktop builds write them to ~/Downloads/OpenMacro.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use openmacro_core::checkin::CheckIn;
use openmacro_core::expenditure::DayStatus;
use openmacro_core::nutrients::Nutrient;
use openmacro_core::targets::MacroTargets;
use openmacro_core::trend;

use openmacro_core::trend::WeighIn;

use crate::state::{CoachSnapshot, Data, LogEntry, weekday_index};

/// Where exports are saved, under the user's Downloads folder.
#[cfg(not(target_arch = "wasm32"))]
pub const FOLDER: &str = "OpenMacro";

#[derive(Debug, Clone, PartialEq)]
pub struct ExportFile {
    pub name: String,
    pub contents: String,
}

/// The CSV files for everything in `data`, named with today's date.
pub fn files(data: &Data, coach: &CoachSnapshot, today: NaiveDate) -> Vec<ExportFile> {
    let file = |kind: &str, contents: String| ExportFile {
        name: format!("openmacro-{kind}-{today}.csv"),
        contents,
    };
    vec![
        file("food-log", food_log(&data.log)),
        file("weigh-ins", weigh_ins(&data.weigh_ins)),
        file("daily", daily(data, coach, today)),
    ]
}

/// A CSV document under construction.
#[derive(Default)]
struct Csv(String);

impl Csv {
    fn row<S: AsRef<str>>(&mut self, fields: impl IntoIterator<Item = S>) {
        for (i, field) in fields.into_iter().enumerate() {
            if i > 0 {
                self.0.push(',');
            }
            self.0.push_str(&quote(field.as_ref()));
        }
        self.0.push_str("\r\n");
    }
}

/// Quotes a field when it holds a separator, quote or line break.
fn quote(field: &str) -> String {
    if field.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

/// Free text from foods and labels. Spreadsheets run cells starting with
/// these characters as formulas, so they get a leading apostrophe.
fn text(value: &str) -> String {
    if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{value}")
    } else {
        value.to_string()
    }
}

/// A number with at most `decimals` places and no trailing zeros.
fn num(value: f64, decimals: usize) -> String {
    let s = format!("{value:.decimals$}");
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        &s
    };
    if s == "-0" { "0".into() } else { s.into() }
}

fn opt(value: Option<f64>, decimals: usize) -> String {
    value.map_or_else(String::new, |v| num(v, decimals))
}

fn food_log(log: &[LogEntry]) -> String {
    let mut entries: Vec<&LogEntry> = log.iter().collect();
    entries.sort_by_key(|e| (e.date, e.hour, e.id));
    // Only nutrients that some entry has, in the standard order.
    let present: BTreeSet<Nutrient> = entries
        .iter()
        .flat_map(|e| e.nutrients.iter().map(|(n, _)| n))
        .collect();
    let nutrients: Vec<Nutrient> = Nutrient::ALL
        .iter()
        .copied()
        .filter(|n| present.contains(n))
        .collect();

    let mut csv = Csv::default();
    csv.row(
        ["date", "time", "food", "amount", "food_id"]
            .into_iter()
            .map(String::from)
            .chain(
                nutrients
                    .iter()
                    .map(|n| format!("{}_{}", n.key(), n.unit().symbol())),
            ),
    );
    for e in entries {
        csv.row(
            [
                e.date.to_string(),
                format!("{:02}:00", e.hour),
                text(&e.name),
                text(&e.amount),
                e.food_id.as_deref().map(text).unwrap_or_default(),
            ]
            .into_iter()
            .chain(nutrients.iter().map(|n| opt(e.nutrients.get(*n), 2))),
        );
    }
    csv.0
}

fn weigh_ins(weigh_ins: &[WeighIn]) -> String {
    let mut sorted = weigh_ins.to_vec();
    sorted.sort_by_key(|w| w.date);
    let mut csv = Csv::default();
    csv.row(["date", "weight_kg"]);
    for w in sorted {
        csv.row([w.date.to_string(), num(w.kg, 2)]);
    }
    csv.0
}

/// The targets in force on `date`: from the check-in active then, or the
/// current targets from the latest check-in onwards. None before the first.
fn target_on(
    date: NaiveDate,
    check_ins: &[CheckIn],
    coach: &CoachSnapshot,
) -> Option<MacroTargets> {
    let active = check_ins
        .iter()
        .filter(|c| c.date <= date)
        .max_by_key(|c| c.date)?;
    if coach.last_check_in.as_ref() == Some(active) {
        Some(coach.target_for(date))
    } else {
        active.week.get(weekday_index(date)).copied()
    }
}

fn daily(data: &Data, coach: &CoachSnapshot, today: NaiveDate) -> String {
    let mut intake: BTreeMap<NaiveDate, Vec<&LogEntry>> = BTreeMap::new();
    for e in &data.log {
        intake.entry(e.date).or_default().push(e);
    }
    let weights = trend::daily_weights(&data.weigh_ins);
    let trend: BTreeMap<NaiveDate, f64> =
        coach.trend.iter().map(|t| (t.date, t.trend_kg)).collect();
    let expenditure: BTreeMap<NaiveDate, f64> =
        coach.expenditure.iter().map(|e| (e.date, e.kcal)).collect();

    let mut csv = Csv::default();
    csv.row([
        "date",
        "day_status",
        "intake_kcal",
        "protein_g",
        "fat_g",
        "carbs_g",
        "weight_kg",
        "trend_weight_kg",
        "expenditure_kcal",
        "target_kcal",
        "target_protein_g",
        "target_fat_g",
        "target_carbs_g",
    ]);
    let Some(first) = intake.keys().chain(weights.keys()).min().copied() else {
        return csv.0;
    };
    for date in first.iter_days().take_while(|d| *d <= today) {
        let status = match data.day_status.get(&date).copied().unwrap_or_default() {
            DayStatus::Normal => "normal",
            DayStatus::Fasted => "fasted",
            DayStatus::Partial => "partial",
        };
        let totals = intake.get(&date).map(|entries| {
            entries
                .iter()
                .map(|e| &e.nutrients)
                .sum::<openmacro_core::nutrients::NutrientProfile>()
        });
        let total = |n: Nutrient| totals.as_ref().map(|t| t.get_or_zero(n));
        let target = target_on(date, &data.check_ins, coach);
        let goal = |f: fn(&MacroTargets) -> f64| target.as_ref().map(f);
        csv.row([
            date.to_string(),
            status.to_string(),
            opt(total(Nutrient::Energy), 0),
            opt(total(Nutrient::Protein), 1),
            opt(total(Nutrient::Fat), 1),
            opt(total(Nutrient::Carbs), 1),
            opt(weights.get(&date).copied(), 2),
            opt(trend.get(&date).copied(), 2),
            opt(expenditure.get(&date).copied(), 0),
            opt(goal(|t| t.kcal), 0),
            opt(goal(|t| t.protein_g), 0),
            opt(goal(|t| t.fat_g), 0),
            opt(goal(|t| t.carbs_g), 0),
        ]);
    }
    csv.0
}

/// Saves the files and says where they went.
pub async fn save(files: Vec<ExportFile>) -> Result<String, String> {
    platform::save(files).await
}

#[cfg(target_os = "android")]
mod platform {
    use super::{ExportFile, FOLDER};

    pub async fn save(files: Vec<ExportFile>) -> Result<String, String> {
        if crate::android::sdk_version().is_some_and(|v| v < 29) {
            return Err("Exporting needs Android 10 or later.".into());
        }
        for file in &files {
            crate::android::save_to_downloads(
                &file.name,
                "text/csv",
                FOLDER,
                file.contents.as_bytes(),
            )
            .ok_or_else(|| format!("Couldn't save {}.", file.name))?;
        }
        Ok(format!(
            "Saved {} files to Downloads/{FOLDER}.",
            files.len()
        ))
    }
}

#[cfg(target_arch = "wasm32")]
mod platform {
    use dioxus::prelude::document;

    use super::ExportFile;

    pub async fn save(files: Vec<ExportFile>) -> Result<String, String> {
        let pairs: Vec<(&str, &str)> = files
            .iter()
            .map(|f| (f.name.as_str(), f.contents.as_str()))
            .collect();
        let json = serde_json::to_string(&pairs).map_err(|e| e.to_string())?;
        // The script must return a value for the eval to complete.
        document::eval(&format!(
            "for (const [name, contents] of {json}) {{
                 const a = document.createElement('a');
                 a.href = URL.createObjectURL(new Blob([contents], {{ type: 'text/csv' }}));
                 a.download = name;
                 a.click();
                 setTimeout(() => URL.revokeObjectURL(a.href), 1000);
             }}
             return true;"
        ))
        .await
        .map_err(|e| format!("Couldn't download the files: {e}"))?;
        Ok(format!("Downloaded {} files.", files.len()))
    }
}

#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
mod platform {
    use std::path::PathBuf;

    use super::{ExportFile, FOLDER};

    pub async fn save(files: Vec<ExportFile>) -> Result<String, String> {
        let home = std::env::var_os("HOME").ok_or("No home folder to save to.")?;
        let dir = PathBuf::from(home).join("Downloads").join(FOLDER);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Couldn't create {}: {e}", dir.display()))?;
        for file in &files {
            let path = dir.join(&file.name);
            std::fs::write(&path, &file.contents)
                .map_err(|e| format!("Couldn't write {}: {e}", path.display()))?;
        }
        Ok(format!("Saved {} files to {}.", files.len(), dir.display()))
    }
}

#[cfg(test)]
mod tests {
    use openmacro_core::nutrients::NutrientProfile;

    use super::*;

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    #[test]
    fn fields_are_quoted_and_defused() {
        assert_eq!(quote("plain"), "plain");
        assert_eq!(quote("Milk, 2%"), "\"Milk, 2%\"");
        assert_eq!(quote("12\" sub"), "\"12\"\" sub\"");
        assert_eq!(text("=HYPERLINK(1)"), "'=HYPERLINK(1)");
        assert_eq!(text("Oats"), "Oats");
    }

    #[test]
    fn numbers_drop_trailing_zeros() {
        assert_eq!(num(155.0, 2), "155");
        assert_eq!(num(12.50, 2), "12.5");
        assert_eq!(num(0.333, 2), "0.33");
        assert_eq!(num(-0.001, 2), "0");
        assert_eq!(num(1894.4, 0), "1894");
    }

    #[test]
    fn food_log_has_a_column_per_nutrient_present() {
        let entry = |id, hour, name: &str, nutrients| LogEntry {
            id,
            date: date(14),
            hour,
            name: name.into(),
            amount: "1 cup (240 g)".into(),
            nutrients,
            food_id: Some("usda_sr_legacy:1".into()),
        };
        let log = vec![
            entry(
                2,
                12,
                "Milk, 2%",
                NutrientProfile::from_macros(122.0, 8.1, 4.8, 11.7),
            ),
            entry(
                1,
                8,
                "Oats",
                NutrientProfile::new()
                    .with(Nutrient::Energy, 150.0)
                    .with(Nutrient::Fiber, 4.0),
            ),
        ];
        let csv = food_log(&log);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(
            lines[0],
            "date,time,food,amount,food_id,energy_kcal,protein_g,fat_g,carbs_g,fiber_g"
        );
        assert_eq!(
            lines[1],
            "2026-10-14,08:00,Oats,1 cup (240 g),usda_sr_legacy:1,150,,,,4"
        );
        assert_eq!(
            lines[2],
            "2026-10-14,12:00,\"Milk, 2%\",1 cup (240 g),usda_sr_legacy:1,122,8.1,4.8,11.7,"
        );
    }

    #[test]
    fn daily_rows_cover_every_day_with_targets_after_a_check_in() {
        let today = date(15);
        let mut data = crate::seed::demo(today);
        let first = data.log.iter().map(|e| e.date).min().unwrap();
        data.check_ins.push(CheckIn {
            date: date(12),
            expenditure_kcal: 2200.0,
            trend_kg: Some(71.0),
            week: Vec::new(),
        });
        let coach = CoachSnapshot::compute(&data, today);
        let csv = daily(&data, &coach, today);
        let rows: Vec<Vec<&str>> = csv
            .lines()
            .skip(1)
            .map(|l| l.split(',').collect())
            .collect();
        assert_eq!(rows.len() as i64, (today - first).num_days() + 1);
        assert_eq!(rows.last().unwrap()[0], "2026-10-15");
        // Before the check-in there's no recorded target; after it, the active one.
        let before = rows.iter().find(|r| r[0] == "2026-10-11").unwrap();
        assert_eq!(before[9], "");
        let after = rows.iter().find(|r| r[0] == "2026-10-13").unwrap();
        assert_eq!(after[9], num(coach.target_for(date(13)).kcal, 0));
        // Every day has an expenditure estimate and every row the same width.
        assert!(rows.iter().all(|r| r.len() == 13 && !r[8].is_empty()));
    }

    #[test]
    fn three_dated_files() {
        let today = date(15);
        let data = crate::seed::demo(today);
        let coach = CoachSnapshot::compute(&data, today);
        let names: Vec<String> = files(&data, &coach, today)
            .into_iter()
            .map(|f| f.name)
            .collect();
        assert_eq!(
            names,
            [
                "openmacro-food-log-2026-10-15.csv",
                "openmacro-weigh-ins-2026-10-15.csv",
                "openmacro-daily-2026-10-15.csv"
            ]
        );
    }
}
