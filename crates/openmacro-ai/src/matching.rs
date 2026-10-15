//! Matching the model's foods to the food database.

use openmacro_core::food::{Food, FoodSource};
use openmacro_core::nutrients::{Nutrient, NutrientProfile};
use openmacro_foods::FoodDb;

use crate::Item;

/// A model's item and the database food it matched, if any.
#[derive(Debug, Clone, PartialEq)]
pub struct Matched {
    pub item: Item,
    pub food: Option<Food>,
}

impl Matched {
    /// Nutrients for the item's weight: from the matched food, or the
    /// model's own estimate when nothing matched.
    pub fn nutrients(&self) -> NutrientProfile {
        match &self.food {
            Some(food) => food.per_100g.scaled(self.item.grams / 100.0),
            None => NutrientProfile::from_macros(
                self.item.kcal,
                self.item.protein_g,
                self.item.fat_g,
                self.item.carbs_g,
            ),
        }
    }
}

/// The searches to try for a model's food name, most specific first: all
/// its words, then fewer from the end (dropping descriptions like
/// "steamed"), then fewer from the start.
fn queries(name: &str) -> Vec<String> {
    let words: Vec<&str> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let mut out: Vec<String> = Vec::new();
    for n in (1..=words.len()).rev() {
        out.push(words[..n].join(" "));
    }
    for start in 1..words.len() {
        out.push(words[start..].join(" "));
    }
    out.dedup();
    out
}

/// The database food for `name`: the first result of the most specific
/// search that finds one, preferring generic foods to branded products.
/// Foods without an energy value are skipped.
pub fn best_food(db: &FoodDb, name: &str, regions: u8) -> Option<Food> {
    for query in queries(name) {
        let hits = db.search(&query, regions, 25).unwrap_or_default();
        let usable: Vec<&Food> = hits
            .iter()
            .map(|h| &h.food)
            .filter(|f| f.per_100g.get(Nutrient::Energy).is_some())
            .collect();
        if let Some(food) = usable
            .iter()
            .find(|f| f.source != FoodSource::Branded)
            .or(usable.first())
        {
            return Some((*food).clone());
        }
    }
    None
}

/// Matches each of the model's items to a database food.
pub fn match_items(db: &FoodDb, items: &[Item], regions: u8) -> Vec<Matched> {
    items
        .iter()
        .map(|item| Matched {
            item: item.clone(),
            food: best_food(db, &item.name, regions),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_drop_words_from_either_end() {
        assert_eq!(
            queries("chicken breast, roasted"),
            vec![
                "chicken breast roasted",
                "chicken breast",
                "chicken",
                "breast roasted",
                "roasted",
            ]
        );
        assert_eq!(queries("rice"), vec!["rice"]);
        assert!(queries(" , ").is_empty());
    }

    #[test]
    fn unmatched_items_keep_the_models_estimate() {
        let item = Item {
            name: "mystery stew".into(),
            grams: 300.0,
            kcal: 360.0,
            protein_g: 20.0,
            fat_g: 15.0,
            carbs_g: 30.0,
        };
        let unmatched = Matched {
            item: item.clone(),
            food: None,
        };
        assert_eq!(unmatched.nutrients().get(Nutrient::Energy), Some(360.0));
    }
}
