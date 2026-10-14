//! Logged food and the user's goal and profile.

use chrono::NaiveDate;

use crate::bmr::{ActivityLevel, BmrEquation, BodyStats};
use crate::nutrients::NutrientProfile;

/// One logged food at a date and clock hour. Nutrients are a snapshot taken
/// when it was logged, so later edits to a food don't rewrite history.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LogEntry {
    pub id: i64,
    pub date: NaiveDate,
    pub hour: u32,
    pub name: String,
    /// Human-readable amount, e.g. "1.5 large (75 g)".
    pub amount: String,
    pub nutrients: NutrientProfile,
}

/// Sum of every entry on `date`.
pub fn day_totals(entries: &[LogEntry], date: NaiveDate) -> NutrientProfile {
    entries
        .iter()
        .filter(|e| e.date == date)
        .map(|e| &e.nutrients)
        .sum()
}

/// Inputs to the starting expenditure estimate.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Profile {
    pub stats: BodyStats,
    pub activity: ActivityLevel,
    pub equation: BmrEquation,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Goal {
    pub goal_weight_kg: f64,
    /// Percent of body weight per week; negative to lose.
    pub rate_pct_per_week: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nutrients::Nutrient;

    fn entry(id: i64, day: u32, kcal: f64) -> LogEntry {
        LogEntry {
            id,
            date: NaiveDate::from_ymd_opt(2026, 1, day).unwrap(),
            hour: 8,
            name: "Oats".into(),
            amount: "40 g".into(),
            nutrients: NutrientProfile::from_macros(kcal, 5.0, 3.0, 27.0),
        }
    }

    #[test]
    fn totals_only_count_the_day() {
        let log = [entry(1, 1, 150.0), entry(2, 1, 100.0), entry(3, 2, 999.0)];
        let totals = day_totals(&log, NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        assert_eq!(totals.get(Nutrient::Energy), Some(250.0));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn nutrients_serialize_with_readable_keys() {
        let json = serde_json::to_string(&entry(1, 1, 150.0)).unwrap();
        assert!(json.contains(r#""energy":150.0"#), "{json}");
        let back: LogEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, entry(1, 1, 150.0));
    }
}
