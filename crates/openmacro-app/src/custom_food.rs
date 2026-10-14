//! Turning the "new food" form into a [`Food`] stored per 100 g.

use openmacro_core::food::{Food, FoodSource, Serving};
use openmacro_core::nutrients::{Nutrient, NutrientProfile};
use openmacro_core::units::KJ_PER_KCAL;

/// What the entered nutrition values are for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Basis {
    #[default]
    Serving,
    Per100g,
    /// Treated as 100 g (density of water).
    Per100ml,
}

/// Australian/NZ and EU labels list carbohydrate without fibre; US labels include it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LabelStyle {
    #[default]
    Australian,
    UnitedStates,
}

/// The form as typed. Numbers are text so half-typed values are allowed.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Input {
    pub name: String,
    pub brand: String,
    pub barcode: String,
    pub basis: Basis,
    pub serving_label: String,
    pub serving_grams: String,
    pub label: LabelStyle,
    pub energy: String,
    pub energy_in_kj: bool,
    pub fat: String,
    pub saturated: String,
    pub carbs: String,
    pub sugars: String,
    pub fiber: String,
    pub protein: String,
    /// Sodium in mg, or salt in g when `salt` is true.
    pub sodium: String,
    pub salt: bool,
}

fn number(text: &str) -> Result<Option<f64>, String> {
    let text = text.trim().replace(',', ".");
    if text.is_empty() {
        return Ok(None);
    }
    match text.parse::<f64>() {
        Ok(v) if v.is_finite() && v >= 0.0 => Ok(Some(v)),
        _ => Err(format!("\"{text}\" isn't a number.")),
    }
}

/// Validates the form and builds the food, or explains what's missing.
pub fn build(input: &Input) -> Result<Food, String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err("Give the food a name.".into());
    }
    let serving_grams = number(&input.serving_grams)?.filter(|g| *g > 0.0);
    let scale = match input.basis {
        Basis::Serving => {
            let grams = serving_grams.ok_or("Enter the serving weight in grams.")?;
            100.0 / grams
        }
        Basis::Per100g | Basis::Per100ml => 1.0,
    };
    let energy = number(&input.energy)?.ok_or("Enter the energy (calories or kilojoules).")?;
    let kcal = if input.energy_in_kj {
        energy / KJ_PER_KCAL
    } else {
        energy
    };

    let fiber = number(&input.fiber)?;
    let carbs = number(&input.carbs)?;
    let mut per_100g = NutrientProfile::from_macros(
        kcal * scale,
        number(&input.protein)?.unwrap_or(0.0) * scale,
        number(&input.fat)?.unwrap_or(0.0) * scale,
        0.0,
    );
    let total_carbs = match input.label {
        LabelStyle::Australian => carbs.unwrap_or(0.0) + fiber.unwrap_or(0.0),
        LabelStyle::UnitedStates => carbs.unwrap_or(0.0),
    };
    per_100g.set(Nutrient::Carbs, total_carbs * scale);
    for (value, nutrient) in [
        (fiber, Nutrient::Fiber),
        (number(&input.saturated)?, Nutrient::Saturated),
        (number(&input.sugars)?, Nutrient::Sugars),
    ] {
        if let Some(v) = value {
            per_100g.set(nutrient, v * scale);
        }
    }
    if let Some(v) = number(&input.sodium)? {
        let sodium_mg = if input.salt { v * 400.0 } else { v };
        per_100g.set(Nutrient::Sodium, sodium_mg * scale);
    }

    let label = match input.serving_label.trim() {
        "" => "serving",
        label => label,
    };
    let barcode = input.barcode.trim();
    Ok(Food {
        id: String::new(),
        name: name.to_string(),
        brand: Some(input.brand.trim().to_string()).filter(|b| !b.is_empty()),
        source: FoodSource::Custom,
        barcode: Some(barcode.to_string()).filter(|b| !b.is_empty()),
        per_100g,
        servings: serving_grams
            .map(|g| Serving::new(label, g))
            .into_iter()
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> Input {
        Input {
            name: " Protein bar ".into(),
            serving_label: "bar".into(),
            serving_grams: "60".into(),
            energy: "900".into(),
            energy_in_kj: true,
            fat: "6".into(),
            carbs: "18".into(),
            fiber: "3".into(),
            protein: "20".into(),
            sodium: "0.3".into(),
            salt: true,
            ..Input::default()
        }
    }

    #[test]
    fn per_serving_values_scale_to_100g() {
        let food = build(&input()).unwrap();
        assert_eq!(food.name, "Protein bar");
        assert_eq!(food.source, FoodSource::Custom);
        let n = &food.per_100g;
        assert!((n.get(Nutrient::Energy).unwrap() - 900.0 / 4.184 * 100.0 / 60.0).abs() < 1e-6);
        assert!((n.get(Nutrient::Protein).unwrap() - 33.333).abs() < 0.001);
        // Australian label: fibre added to carbs.
        assert!((n.get(Nutrient::Carbs).unwrap() - 35.0).abs() < 1e-9);
        // 0.3 g salt = 120 mg sodium per bar.
        assert!((n.get(Nutrient::Sodium).unwrap() - 200.0).abs() < 1e-9);
        assert_eq!(food.servings, vec![Serving::new("bar", 60.0)]);
        assert_eq!(
            food.nutrients_for(food.default_amount())
                .unwrap()
                .get(Nutrient::Protein),
            Some(20.0)
        );
    }

    #[test]
    fn us_labels_and_per_100g_entry() {
        let food = build(&Input {
            basis: Basis::Per100g,
            serving_grams: String::new(),
            label: LabelStyle::UnitedStates,
            energy: "250".into(),
            energy_in_kj: false,
            ..input()
        })
        .unwrap();
        assert_eq!(food.per_100g.get(Nutrient::Energy), Some(250.0));
        assert_eq!(food.per_100g.get(Nutrient::Carbs), Some(18.0));
        assert!(food.servings.is_empty());
    }

    #[test]
    fn missing_or_bad_values_are_explained() {
        assert!(
            build(&Input {
                name: "  ".into(),
                ..input()
            })
            .unwrap_err()
            .contains("name")
        );
        assert!(
            build(&Input {
                serving_grams: String::new(),
                ..input()
            })
            .unwrap_err()
            .contains("serving weight")
        );
        assert!(
            build(&Input {
                energy: String::new(),
                ..input()
            })
            .unwrap_err()
            .contains("energy")
        );
        assert!(
            build(&Input {
                fat: "lots".into(),
                ..input()
            })
            .unwrap_err()
            .contains("lots")
        );
    }
}
