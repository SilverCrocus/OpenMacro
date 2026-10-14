//! Importers that turn the raw public datasets into [`FoodRecord`]s.

pub mod afcd;
pub mod off;
pub mod usda;

use openmacro_core::nutrients::{Nutrient, NutrientProfile};

use crate::FoodRecord;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;

/// Rejects records with missing or physically impossible headline values.
pub fn plausible(food: &FoodRecord) -> bool {
    let n = &food.per_100g;
    let (Some(kcal), Some(p), Some(f), Some(c)) = (
        n.get(Nutrient::Energy),
        n.get(Nutrient::Protein),
        n.get(Nutrient::Fat),
        n.get(Nutrient::Carbs),
    ) else {
        return false;
    };
    let finite = n.iter().all(|(_, v)| v.is_finite() && v >= 0.0);
    finite && !food.name.trim().is_empty() && kcal <= 950.0 && p + f + c <= 110.0
}

/// Sets `nutrient` from the first present value, scaled.
pub fn set_first(
    profile: &mut NutrientProfile,
    nutrient: Nutrient,
    values: &[Option<f64>],
    scale: f64,
) {
    if let Some(v) = values.iter().flatten().next() {
        profile.set(nutrient, v * scale);
    }
}

/// Sums whichever parts are present; `None` if none are.
pub fn sum_present(parts: &[Option<f64>]) -> Option<f64> {
    let present: Vec<f64> = parts.iter().flatten().copied().collect();
    (!present.is_empty()).then(|| present.iter().sum())
}

/// Splits "2 tablespoons" into (2.0, "tablespoons"); text without a leading
/// number is one unit.
pub fn split_count(text: &str) -> (f64, String) {
    let text = text.trim();
    let (head, rest) = text.split_once(' ').unwrap_or((text, ""));
    let count = if let Some((n, d)) = head.split_once('/') {
        n.parse::<f64>()
            .ok()
            .zip(d.parse::<f64>().ok())
            .filter(|(_, d)| *d != 0.0)
            .map(|(n, d)| n / d)
    } else {
        head.parse::<f64>().ok()
    };
    match count {
        Some(c) if c > 0.0 && !rest.trim().is_empty() => (c, rest.trim().to_string()),
        _ => (1.0, text.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_are_split_from_labels() {
        assert_eq!(split_count("1 cup"), (1.0, "cup".into()));
        assert_eq!(split_count("2 tablespoons"), (2.0, "tablespoons".into()));
        assert_eq!(split_count("1/2 cup"), (0.5, "cup".into()));
        assert_eq!(split_count("medium banana"), (1.0, "medium banana".into()));
        assert_eq!(split_count("12"), (1.0, "12".into()));
    }

    #[test]
    fn sums_and_fallbacks() {
        assert_eq!(sum_present(&[Some(1.0), None, Some(2.5)]), Some(3.5));
        assert_eq!(sum_present(&[None, None]), None);
        let mut p = NutrientProfile::new();
        set_first(&mut p, Nutrient::Energy, &[None, Some(418.4)], 1.0 / 4.184);
        assert!((p.get(Nutrient::Energy).unwrap() - 100.0).abs() < 1e-9);
    }
}
