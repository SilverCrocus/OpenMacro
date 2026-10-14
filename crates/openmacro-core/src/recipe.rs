//! Recipes: ingredients combined into one food that can be logged by the
//! serving or by weight.

use crate::food::{Food, FoodSource, Serving};
use crate::nutrients::NutrientProfile;

/// One ingredient, as it was added: a snapshot of its nutrients for the
/// amount used, so later changes to the food don't change the recipe.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Ingredient {
    pub name: String,
    /// Human-readable amount, e.g. "2 large (100 g)".
    pub amount: String,
    /// Weight of the ingredient, when known (not for quick adds).
    pub grams: Option<f64>,
    pub nutrients: NutrientProfile,
    pub food_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Recipe {
    pub name: String,
    /// How many servings the whole recipe makes.
    pub servings: f64,
    /// Weight of the finished dish, if the user weighed it. Cooking changes
    /// weight (water evaporates or is absorbed), so this beats the raw total.
    pub cooked_grams: Option<f64>,
    pub ingredients: Vec<Ingredient>,
}

/// Nominal weight of one serving when no weight is known, so servings
/// still work; logging by weight then isn't meaningful.
const NOMINAL_SERVING_G: f64 = 100.0;

impl Recipe {
    /// Nutrients of the whole recipe.
    pub fn total(&self) -> NutrientProfile {
        self.ingredients.iter().map(|i| &i.nutrients).sum()
    }

    /// Sum of the ingredient weights, if every ingredient has one.
    pub fn raw_grams(&self) -> Option<f64> {
        self.ingredients
            .iter()
            .map(|i| i.grams)
            .sum::<Option<f64>>()
            .filter(|g| *g > 0.0)
    }

    /// Weight of the whole recipe: as cooked if given, else the raw total.
    pub fn weight(&self) -> Option<f64> {
        self.cooked_grams.filter(|g| *g > 0.0).or(self.raw_grams())
    }

    fn servings(&self) -> f64 {
        if self.servings > 0.0 {
            self.servings
        } else {
            1.0
        }
    }

    /// The recipe as a food. The first serving is one serving of the recipe;
    /// when it makes more than one, the whole recipe is offered too.
    pub fn to_food(&self, id: impl Into<String>) -> Food {
        let servings = self.servings();
        let weight = self.weight().unwrap_or(servings * NOMINAL_SERVING_G);
        let mut portions = vec![Serving::new("serving", weight / servings)];
        if servings != 1.0 {
            portions.push(Serving::new("whole recipe", weight));
        }
        Food {
            id: id.into(),
            name: self.name.clone(),
            brand: None,
            source: FoodSource::Recipe,
            barcode: None,
            per_100g: self.total().scaled(100.0 / weight),
            servings: portions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::food::Amount;
    use crate::nutrients::Nutrient;

    fn ingredient(name: &str, grams: Option<f64>, kcal: f64, protein: f64) -> Ingredient {
        Ingredient {
            name: name.into(),
            amount: grams.map_or("1 serving".into(), |g| format!("{g} g")),
            grams,
            nutrients: NutrientProfile::from_macros(kcal, protein, 0.0, 0.0),
            food_id: None,
        }
    }

    fn chili() -> Recipe {
        Recipe {
            name: "Chili".into(),
            servings: 4.0,
            cooked_grams: None,
            ingredients: vec![
                ingredient("Beef mince", Some(500.0), 1100.0, 100.0),
                ingredient("Kidney beans", Some(400.0), 360.0, 24.0),
                ingredient("Tomatoes", Some(400.0), 80.0, 4.0),
            ],
        }
    }

    #[test]
    fn a_serving_is_a_quarter_of_the_total() {
        let food = chili().to_food("custom:7");
        assert_eq!(food.source, FoodSource::Recipe);
        assert_eq!(food.servings[0], Serving::new("serving", 325.0));
        assert_eq!(food.servings[1], Serving::new("whole recipe", 1300.0));
        let serving = food
            .nutrients_for(Amount::Servings {
                index: 0,
                count: 1.0,
            })
            .unwrap();
        assert!((serving.get_or_zero(Nutrient::Energy) - 385.0).abs() < 1e-9);
        assert!((serving.get_or_zero(Nutrient::Protein) - 32.0).abs() < 1e-9);
    }

    #[test]
    fn cooked_weight_sets_the_density() {
        let mut recipe = chili();
        recipe.cooked_grams = Some(1000.0);
        let food = recipe.to_food("");
        // Same calories per serving, spread over less weight.
        assert_eq!(food.servings[0].grams, 250.0);
        assert!((food.per_100g.get_or_zero(Nutrient::Energy) - 154.0).abs() < 1e-9);
    }

    #[test]
    fn unknown_weight_still_gives_servings() {
        let mut recipe = chili();
        recipe
            .ingredients
            .push(ingredient("Quick add", None, 100.0, 0.0));
        assert_eq!(recipe.raw_grams(), None);
        let food = recipe.to_food("");
        let serving = food
            .nutrients_for(Amount::Servings {
                index: 0,
                count: 1.0,
            })
            .unwrap();
        assert!((serving.get_or_zero(Nutrient::Energy) - 410.0).abs() < 1e-9);
    }

    #[test]
    fn single_serving_has_no_whole_recipe_option() {
        let mut recipe = chili();
        recipe.servings = 1.0;
        assert_eq!(recipe.to_food("").servings.len(), 1);
    }
}
