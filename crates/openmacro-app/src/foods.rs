//! A small built-in food list used until the full USDA / Open Food Facts import
//! lands. Values are per 100 g and approximate USDA FoodData Central figures.

use std::sync::LazyLock;

use openmacro_core::food::{Amount, Food, FoodSource, Serving};
use openmacro_core::nutrients::{Nutrient, NutrientProfile};

use crate::state::PlateItem;

pub static FOODS: LazyLock<Vec<Food>> = LazyLock::new(sample_foods);

/// (id, name, kcal, protein, fat, carbs, fiber, servings as (label, grams))
type Row = (
    &'static str,
    &'static str,
    f64,
    f64,
    f64,
    f64,
    f64,
    &'static [(&'static str, f64)],
);

const ROWS: &[Row] = &[
    (
        "almonds",
        "Almonds",
        579.0,
        21.2,
        49.9,
        21.6,
        12.5,
        &[("oz", 28.0)],
    ),
    (
        "apple",
        "Apple, raw",
        52.0,
        0.3,
        0.2,
        13.8,
        2.4,
        &[("medium", 182.0)],
    ),
    (
        "avocado",
        "Avocado, raw",
        160.0,
        2.0,
        14.7,
        8.5,
        6.7,
        &[("half", 68.0)],
    ),
    (
        "banana",
        "Banana, raw",
        89.0,
        1.1,
        0.3,
        22.8,
        2.6,
        &[("medium", 118.0)],
    ),
    (
        "black-beans",
        "Black beans, cooked",
        132.0,
        8.9,
        0.5,
        23.7,
        8.7,
        &[("cup", 172.0)],
    ),
    (
        "broccoli",
        "Broccoli, cooked",
        35.0,
        2.4,
        0.4,
        7.2,
        3.3,
        &[("cup", 156.0)],
    ),
    (
        "brown-rice",
        "Brown rice, cooked",
        123.0,
        2.7,
        1.0,
        25.6,
        1.6,
        &[("cup", 195.0)],
    ),
    (
        "butter",
        "Butter, salted",
        717.0,
        0.9,
        81.1,
        0.1,
        0.0,
        &[("tbsp", 14.0)],
    ),
    (
        "cheddar",
        "Cheddar cheese",
        403.0,
        24.9,
        33.1,
        1.3,
        0.0,
        &[("slice", 28.0)],
    ),
    (
        "chicken-breast",
        "Chicken breast, roasted, skinless",
        165.0,
        31.0,
        3.6,
        0.0,
        0.0,
        &[("serving", 150.0)],
    ),
    (
        "coffee",
        "Coffee, brewed",
        1.0,
        0.1,
        0.0,
        0.0,
        0.0,
        &[("cup", 237.0)],
    ),
    (
        "eggs",
        "Egg, whole, cooked",
        155.0,
        12.6,
        10.6,
        1.1,
        0.0,
        &[("large", 50.0)],
    ),
    (
        "greek-yogurt",
        "Greek yogurt, plain, nonfat",
        59.0,
        10.2,
        0.4,
        3.6,
        0.0,
        &[("container", 170.0)],
    ),
    (
        "ground-beef",
        "Ground beef 90% lean, cooked",
        217.0,
        26.1,
        11.7,
        0.0,
        0.0,
        &[("serving", 113.0)],
    ),
    (
        "milk",
        "Milk, 2%",
        50.0,
        3.3,
        2.0,
        4.8,
        0.0,
        &[("cup", 244.0)],
    ),
    (
        "oats",
        "Oats, rolled, dry",
        379.0,
        13.2,
        6.5,
        67.7,
        10.1,
        &[("cup", 80.0)],
    ),
    (
        "olive-oil",
        "Olive oil",
        884.0,
        0.0,
        100.0,
        0.0,
        0.0,
        &[("tbsp", 13.5)],
    ),
    (
        "orange-juice",
        "Orange juice",
        45.0,
        0.7,
        0.2,
        10.4,
        0.2,
        &[("cup", 248.0)],
    ),
    (
        "pasta",
        "Pasta, cooked",
        158.0,
        5.8,
        0.9,
        30.9,
        1.8,
        &[("cup", 140.0)],
    ),
    (
        "peanut-butter",
        "Peanut butter, smooth",
        588.0,
        25.1,
        50.0,
        20.0,
        6.0,
        &[("tbsp", 16.0)],
    ),
    (
        "potato",
        "Potato, baked, with skin",
        93.0,
        2.5,
        0.1,
        21.2,
        2.2,
        &[("medium", 173.0)],
    ),
    (
        "salmon",
        "Salmon, Atlantic, cooked",
        206.0,
        22.1,
        12.4,
        0.0,
        0.0,
        &[("fillet", 150.0)],
    ),
    (
        "sweet-potato",
        "Sweet potato, baked",
        90.0,
        2.0,
        0.2,
        20.7,
        3.3,
        &[("medium", 114.0)],
    ),
    (
        "tofu",
        "Tofu, firm",
        144.0,
        17.3,
        8.7,
        2.8,
        2.3,
        &[("cup", 252.0)],
    ),
    (
        "whey",
        "Whey protein powder",
        400.0,
        80.0,
        6.0,
        8.0,
        0.0,
        &[("scoop", 30.0)],
    ),
    (
        "white-rice",
        "White rice, cooked",
        130.0,
        2.7,
        0.3,
        28.2,
        0.4,
        &[("cup", 158.0)],
    ),
    (
        "whole-wheat-bread",
        "Whole wheat bread",
        252.0,
        12.4,
        3.5,
        42.7,
        6.0,
        &[("slice", 32.0)],
    ),
];

fn sample_foods() -> Vec<Food> {
    ROWS.iter()
        .map(
            |&(id, name, kcal, protein, fat, carbs, fiber, servings)| Food {
                id: id.into(),
                name: name.into(),
                brand: None,
                source: FoodSource::Common,
                barcode: None,
                per_100g: NutrientProfile::from_macros(kcal, protein, fat, carbs)
                    .with(Nutrient::Fiber, fiber),
                servings: servings
                    .iter()
                    .map(|&(label, grams)| Serving::new(label, grams))
                    .collect(),
            },
        )
        .collect()
}

pub fn food_by_id(id: &str) -> Option<&'static Food> {
    FOODS.iter().find(|f| f.id == id)
}

/// Foods whose name contains every word of `query`, best matches first.
pub fn search(foods: &'static [Food], query: &str) -> Vec<(usize, &'static Food)> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<(usize, &Food)> = foods
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            let name = f.name.to_lowercase();
            words.iter().all(|w| name.contains(w.as_str()))
        })
        .collect();
    hits.sort_by_key(|(_, f)| {
        (
            !f.name.to_lowercase().starts_with(words[0].as_str()),
            f.name.len(),
        )
    });
    hits
}

/// Formats a quantity without trailing zeros: 1, 1.5, 0.25.
pub fn fmt_qty(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub fn amount_label(food: &Food, amount: Amount) -> Option<String> {
    match amount {
        Amount::Grams(g) => Some(format!("{} g", fmt_qty(g))),
        Amount::Servings { index, count } => food.servings.get(index).map(|s| {
            format!(
                "{} {} ({} g)",
                fmt_qty(count),
                s.label,
                fmt_qty(s.grams * count)
            )
        }),
    }
}

pub fn plate_item(food: &Food, amount: Amount) -> Option<PlateItem> {
    Some(PlateItem {
        name: food.name.clone(),
        amount: amount_label(food, amount)?,
        nutrients: food.nutrients_for(amount)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_matches_all_words() {
        let hits = search(&FOODS, "rice cooked");
        let names: Vec<&str> = hits.iter().map(|(_, f)| f.name.as_str()).collect();
        assert_eq!(names, ["Brown rice, cooked", "White rice, cooked"]);
        assert!(search(&FOODS, "  ").is_empty());
    }

    #[test]
    fn labels() {
        let eggs = food_by_id("eggs").unwrap();
        let item = plate_item(
            eggs,
            Amount::Servings {
                index: 0,
                count: 2.0,
            },
        )
        .unwrap();
        assert_eq!(item.amount, "2 large (100 g)");
        assert_eq!(item.nutrients.get(Nutrient::Energy), Some(155.0));
        assert_eq!(fmt_qty(1.5), "1.5");
        assert_eq!(fmt_qty(0.25), "0.25");
    }
}
