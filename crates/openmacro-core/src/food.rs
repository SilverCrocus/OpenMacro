//! Foods, servings and quantity parsing.

use crate::nutrients::NutrientProfile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoodSource {
    /// Generic foods (e.g. USDA FoodData Central).
    Common,
    /// Packaged products with a brand and usually a barcode.
    Branded,
    /// Created by the user.
    Custom,
    /// A user recipe flattened into a food.
    Recipe,
}

/// A named portion of a food, e.g. "1 slice" = 36 g.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Serving {
    pub label: String,
    pub grams: f64,
}

impl Serving {
    pub fn new(label: impl Into<String>, grams: f64) -> Self {
        Self {
            label: label.into(),
            grams,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Food {
    pub id: String,
    pub name: String,
    pub brand: Option<String>,
    pub source: FoodSource,
    pub barcode: Option<String>,
    /// Nutrients per 100 g of the food as eaten.
    pub per_100g: NutrientProfile,
    /// Named servings; the first one is the default.
    pub servings: Vec<Serving>,
}

/// How much of a food was eaten.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Amount {
    Grams(f64),
    /// `count` of the food's serving at `index`.
    Servings {
        index: usize,
        count: f64,
    },
}

impl Food {
    /// Resolves an amount to grams, or `None` if the serving index is invalid.
    pub fn grams(&self, amount: Amount) -> Option<f64> {
        match amount {
            Amount::Grams(g) => Some(g),
            Amount::Servings { index, count } => self.servings.get(index).map(|s| s.grams * count),
        }
    }

    pub fn nutrients_for_grams(&self, grams: f64) -> NutrientProfile {
        self.per_100g.scaled(grams / 100.0)
    }

    pub fn nutrients_for(&self, amount: Amount) -> Option<NutrientProfile> {
        self.grams(amount).map(|g| self.nutrients_for_grams(g))
    }

    pub fn default_amount(&self) -> Amount {
        if self.servings.is_empty() {
            Amount::Grams(100.0)
        } else {
            Amount::Servings {
                index: 0,
                count: 1.0,
            }
        }
    }
}

/// Parses a quantity typed on the logging keypad.
///
/// Accepts decimals ("1.5", "1,5"), simple fractions ("3/4") and mixed
/// numbers ("1 1/2"). Returns `None` for empty, negative or malformed input.
pub fn parse_quantity(input: &str) -> Option<f64> {
    let parse_part = |s: &str| -> Option<f64> {
        if let Some((n, d)) = s.split_once('/') {
            let n: f64 = n.trim().parse().ok()?;
            let d: f64 = d.trim().parse().ok()?;
            (d != 0.0).then(|| n / d)
        } else {
            s.replace(',', ".").parse().ok()
        }
    };

    let parts: Vec<&str> = input.split_whitespace().collect();
    let value = match parts.as_slice() {
        [single] => parse_part(single)?,
        [whole, frac] if frac.contains('/') && !whole.contains('/') => {
            parse_part(whole)? + parse_part(frac)?
        }
        _ => return None,
    };
    (value.is_finite() && value >= 0.0).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nutrients::Nutrient;

    fn yogurt() -> Food {
        Food {
            id: "yogurt".into(),
            name: "Greek yogurt, plain, nonfat".into(),
            brand: None,
            source: FoodSource::Common,
            barcode: None,
            per_100g: NutrientProfile::from_macros(61.0, 10.3, 0.4, 3.6),
            servings: vec![
                Serving::new("1 container (170 g)", 170.0),
                Serving::new("1 cup", 245.0),
            ],
        }
    }

    #[test]
    fn serving_scaling() {
        let food = yogurt();
        let n = food
            .nutrients_for(Amount::Servings {
                index: 0,
                count: 1.5,
            })
            .unwrap();
        assert!((n.get(Nutrient::Energy).unwrap() - 155.55).abs() < 1e-9);
        assert_eq!(
            food.grams(Amount::Servings {
                index: 9,
                count: 1.0
            }),
            None
        );
        assert_eq!(
            food.default_amount(),
            Amount::Servings {
                index: 0,
                count: 1.0
            }
        );
    }

    #[test]
    fn quantity_parsing() {
        assert_eq!(parse_quantity("2"), Some(2.0));
        assert_eq!(parse_quantity("1.25"), Some(1.25));
        assert_eq!(parse_quantity("1,5"), Some(1.5));
        assert_eq!(parse_quantity("3/4"), Some(0.75));
        assert_eq!(parse_quantity(" 1 1/2 "), Some(1.5));
        assert_eq!(parse_quantity("1/0"), None);
        assert_eq!(parse_quantity("-1"), None);
        assert_eq!(parse_quantity("1/2 1/2"), None);
        assert_eq!(parse_quantity(""), None);
    }
}
