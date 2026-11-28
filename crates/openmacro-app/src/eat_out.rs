//! Eating out: Australian fast-food chains' menus, with the nutrition each
//! chain publishes, for logging and for suggestions of what to order.
//!
//! The menus are in `data/chains/`, one JSON file per chain, each with the
//! page its numbers were taken from and when (see docs/FOOD_DATA.md).
//! Each item's id is fixed in its file, so what's logged still finds the
//! item after a refresh renames it.

use std::collections::HashMap;
use std::sync::OnceLock;

use openmacro_core::eat_out::MenuChoice;
use openmacro_core::food::{Food, FoodSource, Serving};
use openmacro_core::nutrients::{Nutrient, NutrientProfile};
use openmacro_core::units::KJ_PER_KCAL;
use serde_json::{Map, Value};

/// The menus. The chain list is sorted by name.
const FILES: &[&str] = &[
    include_str!("../data/chains/dominos.json"),
    include_str!("../data/chains/grilld.json"),
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

/// After an item's id, marks each part left off: "chain:kfc:zinger~no-mayo".
const LEFT_OFF: &str = "~no-";

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
    /// What can be left off it, most energy first.
    pub parts: Vec<Part>,
}

/// Something a customer can ask to leave off an item, such as its mayo.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    /// Its key in the id of the item without it: "big-mac-sauce".
    pub key: String,
    /// As the chain calls it: "mayo", "Big Mac sauce".
    pub name: String,
    /// What leaving it off takes away: all of it, if the item has several.
    pub nutrients: NutrientProfile,
    pub grams: f64,
    /// Whether its energy is the chain's own figure. If not, it's estimated
    /// from a typical serve of its kind.
    pub published: bool,
}

impl Part {
    pub fn kcal(&self) -> f64 {
        self.nutrients.get_or_zero(Nutrient::Energy)
    }
}

/// A typical serve of a kind of part, to estimate one the chain gives no
/// figures for, and to fill in the nutrients of one it gives only the
/// energy of. Values are per 100 g (carbohydrate including fibre). Serves
/// are sized to the median energy of the parts chains do publish figures
/// for (see docs/FOOD_DATA.md).
struct Typical {
    kind: &'static str,
    grams: f64,
    kcal: f64,
    protein: f64,
    fat: f64,
    saturated: f64,
    carbs: f64,
    sugars: f64,
    fiber: f64,
    sodium_mg: f64,
}

impl Typical {
    fn nutrients(&self, grams: f64) -> NutrientProfile {
        let f = grams / 100.0;
        NutrientProfile::from_macros(
            self.kcal * f,
            self.protein * f,
            self.fat * f,
            self.carbs * f,
        )
        .with(Nutrient::Saturated, self.saturated * f)
        .with(Nutrient::Sugars, self.sugars * f)
        .with(Nutrient::Fiber, self.fiber * f)
        .with(Nutrient::Sodium, self.sodium_mg * f)
    }
}

// 3.14 below is grams of protein, not pi.
#[allow(clippy::approx_constant)]
#[rustfmt::skip]
const TYPICAL: [Typical; 9] = [
    // AFCD F005441 Mayonnaise, traditional (>65% fat), commercial.
    Typical { kind: "mayo", grams: 11.0, kcal: 634.0, protein: 0.8, fat: 67.2, saturated: 8.28, carbs: 9.2, sugars: 7.9, fiber: 0.5, sodium_mg: 480.0 },
    // AFCD F002428 Cheese, cheddar, processed, regular fat.
    Typical { kind: "cheese", grams: 15.0, kcal: 300.0, protein: 18.2, fat: 23.9, saturated: 16.04, carbs: 1.9, sugars: 1.5, fiber: 0.0, sodium_mg: 1420.0 },
    // USDA FNDDS 2705808 Cheese sauce.
    Typical { kind: "cheese_sauce", grams: 30.0, kcal: 163.0, protein: 3.14, fat: 13.27, saturated: 3.08, carbs: 7.86, sugars: 0.29, fiber: 0.3, sodium_mg: 656.0 },
    // AFCD F000231 Bacon, shortcut, fried, no added fat: leaner than a
    // middle rasher, like the bacon chains publish figures for.
    Typical { kind: "bacon", grams: 15.0, kcal: 250.0, protein: 30.4, fat: 13.9, saturated: 5.25, carbs: 0.9, sugars: 0.9, fiber: 0.0, sodium_mg: 2000.0 },
    // AFCD F003269 Cream, sour, regular fat.
    Typical { kind: "sour_cream", grams: 25.0, kcal: 338.0, protein: 1.9, fat: 36.3, saturated: 23.46, carbs: 2.0, sugars: 2.0, fiber: 0.0, sodium_mg: 19.0 },
    // USDA FNDDS 2709307 Guacamole, NFS.
    Typical { kind: "guacamole", grams: 50.0, kcal: 155.0, protein: 1.95, fat: 14.18, saturated: 2.057, carbs: 8.45, sugars: 0.72, fiber: 6.5, sodium_mg: 344.0 },
    // AFCD F001973 Butter, plain, salted.
    Typical { kind: "butter", grams: 6.0, kcal: 734.0, protein: 1.1, fat: 82.2, saturated: 56.91, carbs: 0.6, sugars: 0.6, fiber: 0.0, sodium_mg: 776.0 },
    // AFCD F007987 Sauce, barbecue, commercial.
    Typical { kind: "sweet_sauce", grams: 20.0, kcal: 173.0, protein: 0.8, fat: 0.1, saturated: 0.0, carbs: 43.1, sugars: 38.4, fiber: 0.9, sodium_mg: 710.0 },
    // AFCD F003607 Dressing, French or Italian, regular fat, commercial.
    Typical { kind: "dressing", grams: 30.0, kcal: 227.0, protein: 0.1, fat: 19.6, saturated: 2.02, carbs: 11.7, sugars: 11.4, fiber: 0.0, sodium_mg: 970.0 },
];

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

    /// Listed per slice, like pizza, so a meal is several of it.
    pub fn by_the_slice(&self) -> bool {
        self.food
            .servings
            .first()
            .is_some_and(|s| s.label == "slice")
    }

    /// The item with the parts in `keys` left off, as a food of its own.
    /// Its id says what was left off, so a logged entry finds it again.
    pub fn without(&self, keys: &[String]) -> Food {
        let parts: Vec<&Part> = self
            .parts
            .iter()
            .filter(|p| keys.contains(&p.key))
            .collect();
        if parts.is_empty() {
            return self.food.clone();
        }
        let grams = self.food.servings.first().map_or(100.0, |s| s.grams);
        let mut serve = self.food.per_100g.scaled(grams / 100.0);
        for part in &parts {
            for (nutrient, amount) in part.nutrients.iter() {
                if let Some(have) = serve.get(nutrient) {
                    serve.set(nutrient, (have - amount).max(0.0));
                }
            }
        }
        // A serve without a published weight keeps its 100 g placeholder.
        let left = if self.weighed {
            (grams - parts.iter().map(|p| p.grams).sum::<f64>()).max(grams / 2.0)
        } else {
            grams
        };
        let mut food = self.food.clone();
        food.id = parts
            .iter()
            .fold(food.id, |id, p| format!("{id}{LEFT_OFF}{}", p.key));
        food.name = format!("{}, no {}", food.name, or_list(&parts));
        food.per_100g = serve.scaled(100.0 / left);
        if let Some(serving) = food.servings.first_mut() {
            serving.grams = left;
        }
        food
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
                by_the_slice: i.by_the_slice(),
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

/// A menu item by its food id, or by the id of it with parts left off.
pub fn item(id: &str) -> Option<&'static MenuItem> {
    let menus = menus();
    let base = id.split(LEFT_OFF).next()?;
    let &(c, i) = menus.by_id.get(base)?;
    menus.chains.get(c)?.items.get(i)
}

/// The keys of the parts a menu food id says are left off.
pub fn left_off(id: &str) -> Vec<String> {
    id.split(LEFT_OFF).skip(1).map(str::to_string).collect()
}

/// A menu item's food by its id, with any parts it says are left off. None
/// if a part is no longer on the menu, so a logged entry isn't silently
/// given back its mayo.
pub fn food(id: &str) -> Option<Food> {
    let item = item(id)?;
    let keys = left_off(id);
    keys.iter()
        .all(|k| item.parts.iter().any(|p| &p.key == k))
        .then(|| item.without(&keys))
}

/// Whether `food` is a menu item logged by the serve only, without a weight.
pub fn unweighed(food: &Food) -> bool {
    food.id.starts_with(PREFIX) && item(&food.id).is_some_and(|i| !i.weighed)
}

/// "mayo", "mayo or cheese", "mayo, cheese or bacon".
fn or_list(parts: &[&Part]) -> String {
    match parts {
        [] => String::new(),
        [one] => one.name.clone(),
        [rest @ .., last] => format!(
            "{} or {}",
            rest.iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            last.name
        ),
    }
}

/// "Big Mac sauce" as "big-mac-sauce".
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

/// A part of an item from its file: `entry` names it and its kind, and
/// `published` holds the chain's own figures for parts, by key.
fn part(entry: &Value, published: &Map<String, Value>) -> Option<Part> {
    let name = entry["name"].as_str()?.trim().to_string();
    let typical = TYPICAL
        .iter()
        .find(|t| Some(t.kind) == entry["kind"].as_str())?;
    let count = entry["count"].as_f64().unwrap_or(1.0).max(1.0);
    let figures = entry["part"].as_str().and_then(|k| published.get(k));
    let kj = figures
        .and_then(|f| f["kj"].as_f64())
        .filter(|kj| *kj > 0.0);
    let (nutrients, grams) = match (figures, kj) {
        (Some(f), Some(kj)) => {
            let kcal = kj / KJ_PER_KCAL * count;
            let value = |key: &str| f[key].as_f64().map(|v| v * count);
            // As much of a typical serve as has the published energy.
            let like = kcal / typical.kcal * 100.0;
            let grams = value("grams").filter(|g| *g > 0.0).unwrap_or(like);
            let nutrients = match (value("protein_g"), value("fat_g"), value("carbs_g")) {
                (Some(protein), Some(fat), Some(carbs)) => {
                    let fibre = value("fibre_g");
                    let mut n = NutrientProfile::from_macros(
                        kcal,
                        protein,
                        fat,
                        carbs + fibre.unwrap_or(0.0),
                    );
                    for (key, nutrient) in [
                        ("saturated_fat_g", Nutrient::Saturated),
                        ("sugars_g", Nutrient::Sugars),
                        ("fibre_g", Nutrient::Fiber),
                        ("sodium_mg", Nutrient::Sodium),
                    ] {
                        if let Some(v) = value(key) {
                            n.set(nutrient, v);
                        }
                    }
                    n
                }
                // Only the energy is published.
                _ => typical.nutrients(like),
            };
            (nutrients, grams)
        }
        _ => {
            let grams = typical.grams * count;
            (typical.nutrients(grams), grams)
        }
    };
    Some(Part {
        key: slug(&name),
        name,
        nutrients,
        grams,
        published: kj.is_some(),
    })
}

/// Menu items whose name, chain and section contain every word of `query`,
/// so "dominos pizza" finds Domino's pizzas.
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
                let text =
                    format!("{} {} {}", item.food.name, chain.name, item.category).to_lowercase();
                // "mcdonalds" finds "McDonald's".
                let text = text.replace(['\'', '’'], "");
                words
                    .iter()
                    .all(|w| text.contains(&w.replace(['\'', '’'], "")))
            })
        })
        .collect()
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
    let published = v["parts"].as_object().cloned().unwrap_or_default();
    let mut items = Vec::new();
    for raw in v["items"].as_array()? {
        let item_name = raw["name"].as_str()?.trim().to_string();
        let item_id = format!("{PREFIX}{id}:{}", raw["id"].as_str()?);
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
            parts: parts(raw, &published),
        });
    }
    Some(Chain {
        id,
        name,
        source,
        items,
    })
}

/// Items under this many kcal are too small for a typical serve of a part.
const SMALL_KCAL: f64 = 300.0;

/// What can be left off an item, most energy first, each part once.
///
/// A typical serve is too much for a small item such as a single taco or a
/// slice of pizza, so those only get parts the chain gives figures for.
fn parts(raw: &Value, published: &Map<String, Value>) -> Vec<Part> {
    let kcal = raw["kj"].as_f64().unwrap_or(0.0) / KJ_PER_KCAL;
    let slice = serve_label(raw["serving"].as_str().unwrap_or_default()) == "slice";
    let estimates = kcal >= SMALL_KCAL && !slice;
    let mut parts: Vec<Part> = Vec::new();
    for part in raw["leave_off"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| part(entry, published))
        .filter(|p| p.published || estimates)
    {
        if !parts.iter().any(|p| p.key == part.key) {
            parts.push(part);
        }
    }
    parts.sort_by(|a, b| b.kcal().total_cmp(&a.kcal()));
    parts
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
    fn parts_can_be_left_off() {
        let text = r#"{"id": "test", "name": "Test Burgers", "source": {"url": "https://example.com",
            "title": "Nutrition guide", "published": null, "accessed": "2026-11-28"},
            "parts": {
              "mayo": {"name": "Mayo", "grams": 20, "kj": 600, "protein_g": 0.2, "fat_g": 15.0,
                       "saturated_fat_g": 2.0, "carbs_g": 1.0, "sugars_g": 0.8, "fibre_g": null,
                       "sodium_mg": 100},
              "cheese": {"name": "Cheese (1 slice)", "grams": null, "kj": 170, "protein_g": null,
                         "fat_g": null, "carbs_g": null}
            },
            "items": [
              {"id": "big-burger", "name": "Big Burger", "category": "Burgers", "serving": "1 burger",
               "grams": 250, "kj": 2510, "protein_g": 31.2, "fat_g": 27.0, "carbs_g": 52.3,
               "leave_off": [
                 {"name": "cheese", "kind": "cheese", "count": 2, "part": "cheese"},
                 {"name": "mayo", "kind": "mayo", "count": 1, "part": "mayo"},
                 {"name": "bacon", "kind": "bacon", "count": 1, "part": null},
                 {"name": "mayo", "kind": "mayo", "count": 1, "part": null},
                 {"name": "lettuce", "kind": "salad", "count": 1, "part": null}
               ]}
            ]}"#;
        let chain = parse(text).unwrap();
        let burger = &chain.items[0];
        // Most energy first, each once, and only known kinds.
        let names: Vec<&str> = burger.parts.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["mayo", "cheese", "bacon"]);
        let [mayo, cheese, bacon] = &burger.parts[..] else {
            unreachable!()
        };
        // The chain's own figures.
        assert!(mayo.published);
        assert!((mayo.kcal() - 600.0 / 4.184).abs() < 1e-9);
        assert!((mayo.nutrients.get_or_zero(Nutrient::Fat) - 15.0).abs() < 1e-9);
        assert_eq!(mayo.grams, 20.0);
        // Only the energy published (for each of two slices): the rest is
        // a typical cheese's, scaled to it.
        assert!(cheese.published);
        assert!((cheese.kcal() - 340.0 / 4.184).abs() < 1e-9);
        let protein_share = cheese.nutrients.get_or_zero(Nutrient::Protein) / cheese.kcal();
        assert!((protein_share - 18.2 / 300.0).abs() < 1e-9);
        // No figures: a typical serve, estimated.
        assert!(!bacon.published);
        assert!((bacon.kcal() - 0.15 * 250.0).abs() < 1e-9);

        let food = burger.without(&["bacon".to_string(), "mayo".to_string()]);
        assert_eq!(food.id, "chain:test:big-burger~no-mayo~no-bacon");
        assert_eq!(food.name, "Big Burger, no mayo or bacon");
        assert_eq!(food.servings, vec![Serving::new("burger", 215.0)]);
        let serve = food.nutrients_for(food.default_amount()).unwrap();
        let kcal = burger.kcal() - mayo.kcal() - bacon.kcal();
        assert!((serve.get_or_zero(Nutrient::Energy) - kcal).abs() < 1e-6);
        let fat = 27.0 - 15.0 - 0.15 * 13.9;
        assert!((serve.get_or_zero(Nutrient::Fat) - fat).abs() < 1e-6);
        // Nothing left off is the item itself.
        assert_eq!(burger.without(&[]), burger.food);
        assert_eq!(
            left_off("chain:test:big-burger~no-mayo~no-bacon"),
            ["mayo", "bacon"]
        );
    }

    #[test]
    fn small_items_only_get_parts_the_chain_gives_figures_for() {
        let text = r#"{"id": "test", "name": "Test Tacos", "source": {"url": "https://example.com",
            "title": "Nutrition guide", "published": null, "accessed": "2026-11-28"},
            "parts": {"guac": {"name": "Guacamole", "kj": 200}},
            "items": [
              {"id": "taco", "name": "Taco", "category": "Tacos & burritos", "serving": "1 taco",
               "grams": 100, "kj": 900, "protein_g": 10, "fat_g": 10, "carbs_g": 20,
               "leave_off": [
                 {"name": "sour cream", "kind": "sour_cream", "count": 1, "part": null},
                 {"name": "guac", "kind": "guacamole", "count": 1, "part": "guac"}
               ]}
            ]}"#;
        let chain = parse(text).unwrap();
        let names: Vec<&str> = chain.items[0]
            .parts
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, ["guac"]);
    }

    #[test]
    fn a_logged_item_without_a_part_is_found_again() {
        let item = chains()
            .iter()
            .flat_map(|c| &c.items)
            .find(|i| !i.parts.is_empty())
            .unwrap();
        let keys = vec![item.parts[0].key.clone()];
        let variant = item.without(&keys);
        assert_eq!(food(&variant.id), Some(variant.clone()));
        assert_eq!(super::item(&variant.id), Some(item));
        assert_eq!(unweighed(&variant), !item.weighed);
        assert_eq!(food(&format!("{}~no-unicorn", item.food.id)), None);
    }

    #[test]
    fn ids_are_unique() {
        let mut chain_ids: Vec<&str> = chains().iter().map(|c| c.id.as_str()).collect();
        chain_ids.sort_unstable();
        chain_ids.dedup();
        assert_eq!(chain_ids.len(), chains().len(), "two chains share an id");
        let items: usize = chains().iter().map(|c| c.items.len()).sum();
        assert_eq!(menus().by_id.len(), items, "two items share an id");
    }

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
                // What can be left off is less than the whole item.
                let parts: f64 = item.parts.iter().map(Part::kcal).sum();
                assert!(
                    parts < item.kcal() * 0.9,
                    "{}: its parts are {parts:.0} of {:.0} kcal",
                    item.food.id,
                    item.kcal()
                );
            }
        }
    }

    #[test]
    fn items_keep_their_published_values_per_serve() {
        let text = r#"{"id": "test", "name": "Test Chicken", "source": {"url": "https://example.com",
            "title": "Nutrition guide", "published": null, "accessed": "2026-11-28"},
            "items": [
              {"id": "big-burger", "name": "Big Burger", "category": "Burgers", "serving": "1 burger", "grams": 250,
               "kj": 2510, "protein_g": 31.2, "fat_g": 27.0, "sat_fat_g": 9.1, "carbs_g": 52.3,
               "sugars_g": 9.4, "fibre_g": 3.0, "sodium_mg": 1480},
              {"id": "big-burger-pack", "name": "Big Burger", "category": "Burgers", "serving": "6 pieces", "grams": null,
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
        assert_eq!(pieces.food.id, "chain:test:big-burger-pack");
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
    fn labels_and_dates() {
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
