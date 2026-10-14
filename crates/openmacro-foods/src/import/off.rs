//! Open Food Facts CSV export (tab-separated, gzip). Database licensed ODbL,
//! contents DbCL; attribution to Open Food Facts contributors.
//!
//! The export is several gigabytes uncompressed, so it is streamed row by row.

use std::collections::{HashMap, HashSet};
use std::io::BufReader;
use std::path::Path;

use flate2::read::MultiGzDecoder;
use openmacro_core::food::Serving;
use openmacro_core::nutrients::{Nutrient, NutrientProfile};

use super::{Result, plausible};
use crate::{Dataset, FoodDbWriter, FoodRecord, region};

/// OFF stores every per-100 g value in grams (energy aside); scale to our unit.
const DIRECT: &[(&str, Nutrient, f64)] = &[
    ("proteins_100g", Nutrient::Protein, 1.0),
    ("fat_100g", Nutrient::Fat, 1.0),
    ("fiber_100g", Nutrient::Fiber, 1.0),
    ("sugars_100g", Nutrient::Sugars, 1.0),
    ("added-sugars_100g", Nutrient::AddedSugars, 1.0),
    ("saturated-fat_100g", Nutrient::Saturated, 1.0),
    ("monounsaturated-fat_100g", Nutrient::Monounsaturated, 1.0),
    ("polyunsaturated-fat_100g", Nutrient::Polyunsaturated, 1.0),
    ("omega-3-fat_100g", Nutrient::Omega3, 1.0),
    ("trans-fat_100g", Nutrient::TransFat, 1.0),
    ("alcohol_100g", Nutrient::Alcohol, 1.0),
    ("water_100g", Nutrient::Water, 1.0),
    ("cholesterol_100g", Nutrient::Cholesterol, 1_000.0),
    ("sodium_100g", Nutrient::Sodium, 1_000.0),
    ("calcium_100g", Nutrient::Calcium, 1_000.0),
    ("iron_100g", Nutrient::Iron, 1_000.0),
    ("potassium_100g", Nutrient::Potassium, 1_000.0),
    ("magnesium_100g", Nutrient::Magnesium, 1_000.0),
    ("zinc_100g", Nutrient::Zinc, 1_000.0),
    ("vitamin-c_100g", Nutrient::VitaminC, 1_000.0),
    ("caffeine_100g", Nutrient::Caffeine, 1_000.0),
    ("vitamin-a_100g", Nutrient::VitaminA, 1_000_000.0),
    ("vitamin-d_100g", Nutrient::VitaminD, 1_000_000.0),
];

/// Which products to keep.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// US products need at least this many unique scans (the US catalogue is
    /// huge and much of it is rarely used); 0 keeps everything.
    pub us_min_scans: u64,
}

impl Default for Options {
    fn default() -> Self {
        Self { us_min_scans: 1 }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub rows: u64,
    pub in_region: u64,
    pub imported: u64,
}

fn regions_of(countries: &str) -> u8 {
    let mut r = 0;
    for tag in countries.split(',') {
        match tag.trim() {
            "en:australia" => r |= region::AU,
            "en:united-states" => r |= region::US,
            _ => {}
        }
    }
    r
}

/// One CSV row's fields by column name.
struct Row<'a> {
    columns: &'a HashMap<String, usize>,
    record: &'a csv::ByteRecord,
}

impl Row<'_> {
    fn text(&self, name: &str) -> &str {
        self.columns
            .get(name)
            .and_then(|&i| self.record.get(i))
            .and_then(|b| std::str::from_utf8(b).ok())
            .map(str::trim)
            .unwrap_or("")
    }

    fn number(&self, name: &str) -> Option<f64> {
        self.text(name)
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
    }
}

fn profile(row: &Row<'_>, regions: u8) -> NutrientProfile {
    let mut p = NutrientProfile::new();
    let energy = row
        .number("energy-kcal_100g")
        .or_else(|| row.number("energy_100g").map(|kj| kj / 4.184));
    if let Some(kcal) = energy {
        p.set(Nutrient::Energy, kcal);
    }
    for (column, nutrient, scale) in DIRECT {
        if let Some(v) = row.number(column) {
            p.set(*nutrient, v * scale);
        }
    }
    if p.get(Nutrient::Sodium).is_none() {
        if let Some(salt) = row.number("salt_100g") {
            p.set(Nutrient::Sodium, salt / 2.5 * 1_000.0);
        }
    }
    // Australian labels give available carbohydrate; store the total like US labels.
    if let Some(carbs) = row.number("carbohydrates_100g") {
        let fiber = if regions == region::AU {
            p.get(Nutrient::Fiber).unwrap_or(0.0)
        } else {
            0.0
        };
        p.set(Nutrient::Carbs, carbs + fiber);
    }
    p
}

fn record(row: &Row<'_>, regions: u8) -> FoodRecord {
    let brand = row
        .text("brands")
        .split(',')
        .next()
        .map(str::trim)
        .filter(|b| !b.is_empty());
    let serving = row
        .number("serving_quantity")
        .filter(|g| *g > 0.0 && *g < 2_000.0)
        .map(|g| Serving::new("serving", g));
    FoodRecord {
        dataset: Dataset::OpenFoodFacts,
        source_id: row.text("code").to_string(),
        name: row.text("product_name").to_string(),
        brand: brand.map(str::to_string),
        barcode: Some(row.text("code").to_string()).filter(|c| !c.is_empty()),
        regions,
        branded: true,
        popularity: row.number("unique_scans_n").unwrap_or(0.0),
        per_100g: profile(row, regions),
        servings: serving.into_iter().collect(),
    }
}

/// Streams the export and imports Australian and US products.
pub fn import(path: &Path, options: Options, writer: &mut FoodDbWriter) -> Result<Stats> {
    let file = std::fs::File::open(path)?;
    let reader = BufReader::with_capacity(1 << 20, MultiGzDecoder::new(BufReader::new(file)));
    let mut csv = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .quoting(false)
        .flexible(true)
        .from_reader(reader);
    let columns: HashMap<String, usize> = csv
        .byte_headers()?
        .iter()
        .enumerate()
        .map(|(i, h)| (String::from_utf8_lossy(h).into_owned(), i))
        .collect();

    let mut stats = Stats::default();
    let mut seen_codes = HashSet::new();
    let mut record_buf = csv::ByteRecord::new();
    while csv.read_byte_record(&mut record_buf)? {
        stats.rows += 1;
        let row = Row {
            columns: &columns,
            record: &record_buf,
        };
        let regions = regions_of(row.text("countries_tags"));
        if regions == 0 {
            continue;
        }
        stats.in_region += 1;
        let scans = row.number("unique_scans_n").unwrap_or(0.0) as u64;
        if regions == region::US && scans < options.us_min_scans {
            continue;
        }
        let food = record(&row, regions);
        if food.barcode.is_none() || !plausible(&food) || !seen_codes.insert(food.source_id.clone())
        {
            continue;
        }
        writer.insert(&food)?;
        stats.imported += 1;
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row_from(pairs: &[(&str, &str)]) -> (HashMap<String, usize>, csv::ByteRecord) {
        let columns = pairs
            .iter()
            .enumerate()
            .map(|(i, (k, _))| (k.to_string(), i))
            .collect();
        let record = csv::ByteRecord::from(pairs.iter().map(|(_, v)| *v).collect::<Vec<_>>());
        (columns, record)
    }

    #[test]
    fn regions_are_parsed_from_tags() {
        assert_eq!(regions_of("en:australia,en:new-zealand"), region::AU);
        assert_eq!(regions_of("en:united-states, en:australia"), region::ALL);
        assert_eq!(regions_of("en:france"), 0);
    }

    #[test]
    fn australian_carbs_get_fibre_added_and_units_convert() {
        let (columns, rec) = row_from(&[
            ("code", "9300601000000"),
            ("product_name", "Wholemeal Bread"),
            ("brands", "Tip Top,Other"),
            ("energy_100g", "1004"),
            ("proteins_100g", "10"),
            ("fat_100g", "3"),
            ("carbohydrates_100g", "38"),
            ("fiber_100g", "7"),
            ("salt_100g", "1"),
            ("calcium_100g", "0.05"),
            ("serving_quantity", "38"),
            ("unique_scans_n", "12"),
        ]);
        let row = Row {
            columns: &columns,
            record: &rec,
        };
        let food = record(&row, region::AU);
        assert_eq!(food.brand.as_deref(), Some("Tip Top"));
        assert!((food.per_100g.get(Nutrient::Energy).unwrap() - 239.96).abs() < 0.01);
        assert_eq!(food.per_100g.get(Nutrient::Carbs), Some(45.0));
        assert_eq!(food.per_100g.get(Nutrient::Sodium), Some(400.0));
        assert_eq!(food.per_100g.get(Nutrient::Calcium), Some(50.0));
        assert_eq!(food.servings, vec![Serving::new("serving", 38.0)]);
        assert!(plausible(&food));

        let us = record(&row, region::US);
        assert_eq!(us.per_100g.get(Nutrient::Carbs), Some(38.0));
    }
}
