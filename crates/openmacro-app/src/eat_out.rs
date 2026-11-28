//! Eating out: Australian fast-food chains' menus, with the nutrition each
//! chain publishes, for logging and for suggestions of what to order.
//!
//! The menus are in `data/chains/`, one JSON file per chain, each with the
//! page its numbers were taken from and when (see docs/FOOD_DATA.md).

use std::collections::HashMap;
use std::sync::OnceLock;

use openmacro_core::eat_out::MenuChoice;
use openmacro_core::food::{Food, FoodSource, Serving};
use openmacro_core::nutrients::{Nutrient, NutrientProfile};
use serde_json::Value;

/// The menus. The chain list is sorted by name.
const FILES: &[&str] = &[
    include_str!("../data/chains/dominos.json"),
    include_str!("../data/chains/guzman-y-gomez.json"),
    include_str!("../data/chains/hungry-jacks.json"),
    include_str!("../data/chains/kfc.json"),
    include_str!("../data/chains/mcdonalds.json"),
    include_str!("../data/chains/nandos.json"),
    include_str!("../data/chains/oporto.json"),
    include_str!("../data/chains/pizza-hut.json"),
    include_str!("../data/chains/red-rooster.json"),
    include_str!("../data/chains/schnitz.json"),
    include_str!("../data/chains/subway.json"),
    include_str!("../data/chains/zambrero.json"),
];

/// Menu food ids start with this, then the chain's id and the item's.
const PREFIX: &str = "chain:";

/// The order of a menu's sections. Anything else goes last.
const CATEGORIES: [&str; 12] = [
    "Burgers",
    "Chicken",
    "Wraps & rolls",
    "Pizza",
    "Tacos & burritos",
    "Bowls & salads",
    "Breakfast",
    "Kids",
    "Sides",
    "Desserts",
    "Drinks",
    "Sauces",
];

/// Sections that are a meal in themselves, so worth suggesting.
const MAINS: [&str; 7] = [
    "Burgers",
    "Chicken",
    "Wraps & rolls",
    "Pizza",
    "Tacos & burritos",
    "Bowls & salads",
    "Breakfast",
];

/// Where a chain's numbers came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    pub title: String,
    pub url: String,
    /// The date printed on it, if any, such as "2026-08".
    pub published: Option<String>,
    /// When it was read, such as "2026-11-28".
    pub accessed: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chain {
    pub id: String,
    pub name: String,
    pub source: Source,
    pub items: Vec<MenuItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MenuItem {
    pub food: Food,
    pub category: String,
    /// Whether the chain publishes the serve's weight. If it doesn't, the
    /// item is logged by the serve only, and the 100 g stored for its serve
    /// is just a placeholder.
    pub weighed: bool,
}

impl MenuItem {
    fn per_serve(&self, nutrient: Nutrient) -> f64 {
        let grams = self.food.servings.first().map_or(100.0, |s| s.grams);
        self.food.per_100g.get_or_zero(nutrient) * grams / 100.0
    }

    pub fn kcal(&self) -> f64 {
        self.per_serve(Nutrient::Energy)
    }

    pub fn protein_g(&self) -> f64 {
        self.per_serve(Nutrient::Protein)
    }

    /// Whether it's a meal in itself. Breakfast counts only in the morning.
    pub fn is_main(&self, hour: u32) -> bool {
        match self.category.as_str() {
            "Breakfast" => hour < 11,
            category => MAINS.contains(&category),
        }
    }
}

impl Chain {
    /// The menu's sections in display order, each with its items.
    pub fn sections(&self) -> Vec<(&str, Vec<&MenuItem>)> {
        let rank = |c: &str| {
            CATEGORIES
                .iter()
                .position(|k| *k == c)
                .unwrap_or(usize::MAX)
        };
        let mut sections: Vec<(&str, Vec<&MenuItem>)> = Vec::new();
        for item in &self.items {
            match sections.iter_mut().find(|(c, _)| *c == item.category) {
                Some((_, items)) => items.push(item),
                None => sections.push((&item.category, vec![item])),
            }
        }
        sections.sort_by_key(|(c, _)| rank(c));
        sections
    }

    /// The menu as the suggestions see it, at `hour` of the day.
    pub fn choices(&self, hour: u32) -> Vec<MenuChoice> {
        self.items
            .iter()
            .map(|i| MenuChoice {
                kcal: i.kcal(),
                protein_g: i.protein_g(),
                main: i.is_main(hour),
            })
            .collect()
    }

    /// "Schnitz's published nutrition information, August 2026", or the
    /// date it was read when the source has none.
    pub fn source_line(&self) -> String {
        let date = self
            .source
            .published
            .as_deref()
            .unwrap_or(&self.source.accessed);
        format!("{}, {}", self.source.title, month_year(date))
    }
}

/// "2026-08" or "2026-08-14" as "August 2026".
fn month_year(date: &str) -> String {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let mut parts = date.split('-');
    let year = parts.next().unwrap_or_default();
    match parts
        .next()
        .and_then(|m| m.parse::<usize>().ok())
        .and_then(|m| MONTHS.get(m.wrapping_sub(1)))
    {
        Some(month) => format!("{month} {year}"),
        None => date.to_string(),
    }
}

struct Menus {
    chains: Vec<Chain>,
    /// Each item's chain and position, by food id.
    by_id: HashMap<String, (usize, usize)>,
}

fn menus() -> &'static Menus {
    static MENUS: OnceLock<Menus> = OnceLock::new();
    MENUS.get_or_init(|| {
        let mut chains: Vec<Chain> = FILES.iter().filter_map(|text| parse(text)).collect();
        chains.sort_by_key(|c| c.name.to_lowercase());
        let mut by_id = HashMap::new();
        for (c, chain) in chains.iter().enumerate() {
            for (i, item) in chain.items.iter().enumerate() {
                by_id.insert(item.food.id.clone(), (c, i));
            }
        }
        Menus { chains, by_id }
    })
}

/// Every chain, in list order.
pub fn chains() -> &'static [Chain] {
    &menus().chains
}

pub fn chain(id: &str) -> Option<&'static Chain> {
    chains().iter().find(|c| c.id == id)
}

/// A menu item by its food id.
pub fn item(id: &str) -> Option<&'static MenuItem> {
    let menus = menus();
    let &(c, i) = menus.by_id.get(id)?;
    menus.chains.get(c)?.items.get(i)
}

/// Whether `food` is a menu item logged by the serve only, without a weight.
pub fn unweighed(food: &Food) -> bool {
    food.id.starts_with(PREFIX) && item(&food.id).is_some_and(|i| !i.weighed)
}

/// Menu items whose name and chain contain every word of `query`.
pub fn search(query: &str) -> Vec<&'static MenuItem> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return Vec::new();
    }
    chains()
        .iter()
        .flat_map(|chain| {
            let words = &words;
            chain.items.iter().filter(move |item| {
                let text = format!("{} {}", item.food.name, chain.name).to_lowercase();
                // "mcdonalds" finds "McDonald's".
                let text = text.replace(['\'', '’'], "");
                words
                    .iter()
                    .all(|w| text.contains(&w.replace(['\'', '’'], "")))
            })
        })
        .collect()
}

/// "Bondi Burger" as "bondi-burger".
fn slug(name: &str) -> String {
    let mut slug = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_string()
}

/// The serve's name for the amount picker: "1 burger" (or "1 burger
/// (double fillet)" or "1 burger, mild") as "burger", and anything else, such as "6 pieces" or
/// a long description, as "serve". The item's name says the rest.
fn serve_label(serving: &str) -> String {
    let noun = serving
        .trim()
        .strip_prefix("1 ")
        .map(|s| s.split([',', '(']).next().unwrap_or(s).trim())
        .unwrap_or_default();
    if noun.is_empty() || noun.split_whitespace().count() > 3 {
        "serve".to_string()
    } else {
        noun.to_string()
    }
}

/// One chain's file, or `None` if it's malformed (a test checks they all
/// read).
fn parse(text: &str) -> Option<Chain> {
    let v: Value = serde_json::from_str(text).ok()?;
    let id = v["id"].as_str()?.to_string();
    let name = v["name"].as_str()?.to_string();
    let source = &v["source"];
    let source = Source {
        title: source["title"].as_str()?.to_string(),
        url: source["url"].as_str()?.to_string(),
        published: source["published"].as_str().map(str::to_string),
        accessed: source["accessed"].as_str()?.to_string(),
    };
    let mut items = Vec::new();
    for raw in v["items"].as_array()? {
        let item_name = raw["name"].as_str()?.trim().to_string();
        let mut item_id = format!("{PREFIX}{id}:{}", slug(&item_name));
        // Two items with the same name (such as two sizes) stay apart.
        let mut n = 2;
        while items.iter().any(|i: &MenuItem| i.food.id == item_id) {
            item_id = format!("{PREFIX}{id}:{}-{n}", slug(&item_name));
            n += 1;
        }
        let grams = raw["grams"].as_f64().filter(|g| *g > 0.0);
        let per_100g = per_100g(raw, grams.unwrap_or(100.0))?;
        items.push(MenuItem {
            food: Food {
                id: item_id,
                name: item_name,
                brand: Some(name.clone()),
                source: FoodSource::Branded,
                barcode: None,
                per_100g,
                servings: vec![Serving::new(
                    serve_label(raw["serving"].as_str().unwrap_or_default()),
                    grams.unwrap_or(100.0),
                )],
            },
            category: raw["category"].as_str()?.to_string(),
            weighed: grams.is_some(),
        });
    }
    Some(Chain {
        id,
        name,
        source,
        items,
    })
}

/// An item's nutrients per 100 g, from its per-serve values and the serve's
/// weight in grams.
fn per_100g(raw: &Value, grams: f64) -> Option<NutrientProfile> {
    let factor = 100.0 / grams;
    let value = |key: &str| raw[key].as_f64().map(|v| v * factor);
    let fibre = value("fibre_g");
    // Australian labels give available carbohydrate; OpenMacro stores the
    // total, with fibre (see docs/FOOD_DATA.md).
    let carbs = value("carbs_g")? + fibre.unwrap_or(0.0);
    let mut n = NutrientProfile::from_macros(
        value("kj")? / 4.184,
        value("protein_g")?,
        value("fat_g")?,
        carbs,
    );
    for (key, nutrient) in [
        ("sat_fat_g", Nutrient::Saturated),
        ("sugars_g", Nutrient::Sugars),
        ("fibre_g", Nutrient::Fiber),
        ("sodium_mg", Nutrient::Sodium),
    ] {
        if let Some(v) = value(key) {
            n.set(nutrient, v);
        }
    }
    Some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_menu_file_reads() {
        assert_eq!(chains().len(), FILES.len());
        for chain in chains() {
            assert!(!chain.items.is_empty(), "{} has no items", chain.id);
            for item in &chain.items {
                assert!(item.kcal() > 0.0, "{} has no energy", item.food.id);
                assert!(
                    CATEGORIES.contains(&item.category.as_str()),
                    "{} is in an unknown section, {}",
                    item.food.id,
                    item.category
                );
                assert_eq!(super::item(&item.food.id), Some(item));
            }
        }
    }

    #[test]
    fn items_keep_their_published_values_per_serve() {
        let text = r#"{"id": "test", "name": "Test Chicken", "source": {"url": "https://example.com",
            "title": "Nutrition guide", "published": null, "accessed": "2026-11-28"},
            "items": [
              {"name": "Big Burger", "category": "Burgers", "serving": "1 burger", "grams": 250,
               "kj": 2510, "protein_g": 31.2, "fat_g": 27.0, "sat_fat_g": 9.1, "carbs_g": 52.3,
               "sugars_g": 9.4, "fibre_g": 3.0, "sodium_mg": 1480},
              {"name": "Big Burger", "category": "Burgers", "serving": "6 pieces", "grams": null,
               "kj": 1000, "protein_g": 10, "fat_g": 10, "carbs_g": 20}
            ]}"#;
        let chain = parse(text).unwrap();
        let burger = &chain.items[0];
        assert_eq!(burger.food.id, "chain:test:big-burger");
        assert_eq!(burger.food.servings, vec![Serving::new("burger", 250.0)]);
        assert!(burger.weighed);
        assert!((burger.kcal() - 2510.0 / 4.184).abs() < 1e-9);
        assert!((burger.protein_g() - 31.2).abs() < 1e-9);
        let carbs = burger.food.per_100g.get_or_zero(Nutrient::Carbs) * 2.5;
        assert!((carbs - 55.3).abs() < 1e-9, "fibre is added back");
        let sodium = burger.food.per_100g.get_or_zero(Nutrient::Sodium) * 2.5;
        assert!((sodium - 1480.0).abs() < 1e-9);

        let pieces = &chain.items[1];
        assert_eq!(pieces.food.id, "chain:test:big-burger-2");
        assert_eq!(pieces.food.servings, vec![Serving::new("serve", 100.0)]);
        assert!(!pieces.weighed);
        assert!((pieces.protein_g() - 10.0).abs() < 1e-9);
        assert_eq!(chain.source_line(), "Nutrition guide, November 2026");
    }

    #[test]
    fn breakfast_is_a_main_only_in_the_morning() {
        let mut item = chains()[0].items[0].clone();
        item.category = "Breakfast".into();
        assert!(item.is_main(8));
        assert!(!item.is_main(13));
        item.category = "Drinks".into();
        assert!(!item.is_main(8));
    }

    #[test]
    fn labels_and_slugs() {
        assert_eq!(slug("McChicken® Burger (Large)"), "mcchicken-burger-large");
        assert_eq!(serve_label("1 wrap"), "wrap");
        assert_eq!(serve_label("Regular"), "serve");
        assert_eq!(serve_label("1 burger (double fillet)"), "burger");
        assert_eq!(serve_label("1 burrito, mild"), "burrito");
        assert_eq!(serve_label("1 whole chicken"), "whole chicken");
        assert_eq!(
            serve_label("1 serve added to a burrito, bowl or other product"),
            "serve"
        );
        assert_eq!(month_year("2026-08"), "August 2026");
        assert_eq!(month_year("2026-11-28"), "November 2026");
        assert_eq!(month_year("sometime"), "sometime");
    }
}
