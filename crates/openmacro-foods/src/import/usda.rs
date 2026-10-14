//! USDA FoodData Central CSV downloads (Foundation, SR Legacy, FNDDS).
//! Public domain (CC0).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use openmacro_core::food::Serving;
use openmacro_core::nutrients::{Nutrient, NutrientProfile};

use super::{Result, plausible, set_first, split_count, sum_present};
use crate::{Dataset, FoodDbWriter, FoodRecord, region};

/// FDC nutrient ids that map straight onto one of ours (all units already match).
const DIRECT: &[(u32, Nutrient)] = &[
    (1003, Nutrient::Protein),
    (1004, Nutrient::Fat),
    (1018, Nutrient::Alcohol),
    (1079, Nutrient::Fiber),
    (1009, Nutrient::Starch),
    (1235, Nutrient::AddedSugars),
    (1292, Nutrient::Monounsaturated),
    (1293, Nutrient::Polyunsaturated),
    (1258, Nutrient::Saturated),
    (1257, Nutrient::TransFat),
    (1272, Nutrient::Omega3Dha),
    (1278, Nutrient::Omega3Epa),
    (1216, Nutrient::Cysteine),
    (1221, Nutrient::Histidine),
    (1212, Nutrient::Isoleucine),
    (1213, Nutrient::Leucine),
    (1214, Nutrient::Lysine),
    (1215, Nutrient::Methionine),
    (1217, Nutrient::Phenylalanine),
    (1211, Nutrient::Threonine),
    (1210, Nutrient::Tryptophan),
    (1218, Nutrient::Tyrosine),
    (1219, Nutrient::Valine),
    (1165, Nutrient::VitaminB1),
    (1166, Nutrient::VitaminB2),
    (1167, Nutrient::VitaminB3),
    (1170, Nutrient::VitaminB5),
    (1175, Nutrient::VitaminB6),
    (1178, Nutrient::VitaminB12),
    (1177, Nutrient::Folate),
    (1106, Nutrient::VitaminA),
    (1162, Nutrient::VitaminC),
    (1114, Nutrient::VitaminD),
    (1109, Nutrient::VitaminE),
    (1185, Nutrient::VitaminK),
    (1087, Nutrient::Calcium),
    (1098, Nutrient::Copper),
    (1089, Nutrient::Iron),
    (1090, Nutrient::Magnesium),
    (1101, Nutrient::Manganese),
    (1091, Nutrient::Phosphorus),
    (1092, Nutrient::Potassium),
    (1103, Nutrient::Selenium),
    (1093, Nutrient::Sodium),
    (1095, Nutrient::Zinc),
    (1057, Nutrient::Caffeine),
    (1253, Nutrient::Cholesterol),
    (1180, Nutrient::Choline),
    (1051, Nutrient::Water),
];

/// Ids used in fallbacks and derived values.
const ENERGY: [u32; 3] = [1008, 2048, 2047];
const CARBS: [u32; 2] = [1005, 1050];
const SUGARS: [u32; 2] = [2000, 1063];
const ALA: [u32; 2] = [1404, 1270];
const DPA: u32 = 1280;
const LINOLEIC: [u32; 2] = [1316, 1269];
const ARACHIDONIC: u32 = 1271;

fn wanted_ids() -> HashSet<u32> {
    DIRECT
        .iter()
        .map(|(id, _)| *id)
        .chain(ENERGY)
        .chain(CARBS)
        .chain(SUGARS)
        .chain(ALA)
        .chain(LINOLEIC)
        .chain([DPA, ARACHIDONIC, 1272, 1278])
        .collect()
}

fn profile(values: &HashMap<u32, f64>) -> NutrientProfile {
    let get = |id: u32| values.get(&id).copied();
    let first = |ids: &[u32]| ids.iter().map(|id| get(*id)).collect::<Vec<_>>();
    let mut p = NutrientProfile::new();
    for (id, nutrient) in DIRECT {
        if let Some(v) = get(*id) {
            p.set(*nutrient, v);
        }
    }
    set_first(&mut p, Nutrient::Energy, &first(&ENERGY), 1.0);
    set_first(&mut p, Nutrient::Carbs, &first(&CARBS), 1.0);
    set_first(&mut p, Nutrient::Sugars, &first(&SUGARS), 1.0);
    set_first(&mut p, Nutrient::Omega3Ala, &first(&ALA), 1.0);
    let ala = first(&ALA).into_iter().flatten().next();
    if let Some(v) = sum_present(&[ala, get(1278), get(1272), get(DPA)]) {
        p.set(Nutrient::Omega3, v);
    }
    let linoleic = first(&LINOLEIC).into_iter().flatten().next();
    if let Some(v) = sum_present(&[linoleic, get(ARACHIDONIC)]) {
        p.set(Nutrient::Omega6, v);
    }
    p
}

fn csv_reader(path: &Path) -> Result<csv::Reader<std::fs::File>> {
    Ok(csv::ReaderBuilder::new().flexible(true).from_path(path)?)
}

/// Column index by header name.
fn column(headers: &csv::StringRecord, name: &str) -> Result<usize> {
    headers
        .iter()
        .position(|h| h == name)
        .ok_or_else(|| format!("missing column {name}").into())
}

fn servings(rows: &mut Vec<(i64, Serving)>) -> Vec<Serving> {
    rows.sort_by_key(|(seq, _)| *seq);
    let mut seen = HashSet::new();
    rows.drain(..)
        .map(|(_, s)| s)
        .filter(|s| seen.insert(s.label.to_lowercase()))
        .collect()
}

/// Imports every food of `data_type` from an extracted FDC CSV folder.
/// Foods whose lowercase name is already in `seen` are skipped, so earlier
/// (preferred) datasets win on duplicates.
pub fn import(
    dir: &Path,
    dataset: Dataset,
    data_type: &str,
    writer: &mut FoodDbWriter,
    seen: &mut HashSet<String>,
) -> Result<usize> {
    // Foods of the wanted type.
    let mut foods: HashMap<u32, String> = HashMap::new();
    let mut rdr = csv_reader(&dir.join("food.csv"))?;
    let h = rdr.headers()?.clone();
    let (c_id, c_type, c_desc) = (
        column(&h, "fdc_id")?,
        column(&h, "data_type")?,
        column(&h, "description")?,
    );
    for row in rdr.records() {
        let row = row?;
        if &row[c_type] == data_type {
            if let Ok(id) = row[c_id].parse() {
                foods.insert(id, row[c_desc].trim().to_string());
            }
        }
    }

    // Some releases (FNDDS 2024) key food_nutrient by the legacy 3-digit
    // nutrient number instead of the nutrient id; translate those.
    let mut by_number: HashMap<u32, u32> = HashMap::new();
    if let Ok(mut rdr) = csv_reader(&dir.join("nutrient.csv")) {
        let h = rdr.headers()?.clone();
        let (c_id, c_nbr) = (column(&h, "id")?, column(&h, "nutrient_nbr")?);
        for row in rdr.records().flatten() {
            let nbr = row[c_nbr].parse::<f64>().ok().map(|n| n as u32);
            if let (Ok(id), Some(nbr)) = (row[c_id].parse::<u32>(), nbr) {
                by_number.insert(nbr, id);
            }
        }
    }

    // Nutrient amounts per 100 g.
    let wanted = wanted_ids();
    let mut amounts: HashMap<u32, HashMap<u32, f64>> = HashMap::new();
    let mut rdr = csv_reader(&dir.join("food_nutrient.csv"))?;
    let h = rdr.headers()?.clone();
    let (c_food, c_nut, c_amt) = (
        column(&h, "fdc_id")?,
        column(&h, "nutrient_id")?,
        column(&h, "amount")?,
    );
    for row in rdr.records() {
        let row = row?;
        let (Ok(food), Ok(nutrient), Ok(amount)) = (
            row[c_food].parse::<u32>(),
            row[c_nut].parse::<u32>(),
            row[c_amt].parse::<f64>(),
        ) else {
            continue;
        };
        // Ids start at 1000; anything lower is a legacy nutrient number.
        let nutrient = if nutrient < 1000 {
            by_number.get(&nutrient).copied().unwrap_or(nutrient)
        } else {
            nutrient
        };
        if foods.contains_key(&food) && wanted.contains(&nutrient) {
            amounts.entry(food).or_default().insert(nutrient, amount);
        }
    }

    // Portions: "1 cup" descriptions (FNDDS), modifiers (SR Legacy) or measure units (Foundation).
    let mut units: HashMap<String, String> = HashMap::new();
    if let Ok(mut rdr) = csv_reader(&dir.join("measure_unit.csv")) {
        for row in rdr.records().flatten() {
            units.insert(row[0].to_string(), row[1].to_string());
        }
    }
    let mut portions: HashMap<u32, Vec<(i64, Serving)>> = HashMap::new();
    if let Ok(mut rdr) = csv_reader(&dir.join("food_portion.csv")) {
        let h = rdr.headers()?.clone();
        let c_food = column(&h, "fdc_id")?;
        let c_seq = column(&h, "seq_num")?;
        let c_amount = column(&h, "amount")?;
        let c_unit = column(&h, "measure_unit_id")?;
        let c_desc = column(&h, "portion_description")?;
        let c_mod = column(&h, "modifier")?;
        let c_grams = column(&h, "gram_weight")?;
        for row in rdr.records().flatten() {
            let Ok(food) = row[c_food].parse::<u32>() else {
                continue;
            };
            let Ok(grams) = row[c_grams].parse::<f64>() else {
                continue;
            };
            if !foods.contains_key(&food) || grams <= 0.0 {
                continue;
            }
            let desc = row[c_desc].trim();
            let modifier = row[c_mod].trim();
            let amount = row[c_amount]
                .parse::<f64>()
                .ok()
                .filter(|a| *a > 0.0)
                .unwrap_or(1.0);
            let (count, label) =
                if !desc.is_empty() && !desc.eq_ignore_ascii_case("Quantity not specified") {
                    split_count(desc)
                } else if !modifier.is_empty() && modifier.parse::<f64>().is_err() {
                    (amount, modifier.to_string())
                } else {
                    match units.get(row[c_unit].trim()).map(String::as_str) {
                        Some(unit) if unit != "undetermined" => (amount, unit.to_string()),
                        _ => continue,
                    }
                };
            if label.is_empty() || count <= 0.0 {
                continue;
            }
            let seq = row[c_seq].parse().unwrap_or(i64::MAX);
            portions
                .entry(food)
                .or_default()
                .push((seq, Serving::new(label, grams / count)));
        }
    }

    let mut ids: Vec<u32> = foods.keys().copied().collect();
    ids.sort_unstable();
    let mut imported = 0;
    for id in ids {
        let name = &foods[&id];
        if !seen.insert(name.to_lowercase()) {
            continue;
        }
        let record = FoodRecord {
            dataset,
            source_id: id.to_string(),
            name: name.clone(),
            brand: None,
            barcode: None,
            regions: region::US,
            branded: false,
            popularity: 0.0,
            per_100g: amounts.get(&id).map(profile).unwrap_or_default(),
            servings: portions.get_mut(&id).map(servings).unwrap_or_default(),
        };
        if plausible(&record) {
            writer.insert(&record)?;
            imported += 1;
        }
    }
    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_values_and_fallbacks() {
        let values = HashMap::from([
            (2048, 120.0),
            (1003, 10.0),
            (1004, 5.0),
            (1050, 8.0),
            (1270, 0.5),
            (1278, 0.1),
            (1272, 0.2),
            (1269, 1.0),
            (1271, 0.05),
        ]);
        let p = profile(&values);
        assert_eq!(p.get(Nutrient::Energy), Some(120.0));
        assert_eq!(p.get(Nutrient::Carbs), Some(8.0));
        assert_eq!(p.get(Nutrient::Omega3Ala), Some(0.5));
        assert!((p.get(Nutrient::Omega3).unwrap() - 0.8).abs() < 1e-9);
        assert!((p.get(Nutrient::Omega6).unwrap() - 1.05).abs() < 1e-9);
        assert_eq!(p.get(Nutrient::Sugars), None);
    }
}
