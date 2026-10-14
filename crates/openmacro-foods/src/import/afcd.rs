//! Australian Food Composition Database (FSANZ), Release 3 "Nutrient
//! profiles" workbook. Licensed CC BY 4.0, attribution to Food Standards
//! Australia New Zealand.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use calamine::{Data, Reader, open_workbook_auto};
use openmacro_core::nutrients::{Nutrient, NutrientProfile};

use super::{Result, plausible, set_first, sum_present};
use crate::{Dataset, FoodDbWriter, FoodRecord, region};

const SHEET: &str = "All solids & liquids per 100 g";
const KJ_PER_KCAL: f64 = 4.184;

/// (column header, nutrient, scale to our unit)
const DIRECT: &[(&str, Nutrient, f64)] = &[
    (
        "Energy with dietary fibre, equated (kJ)",
        Nutrient::Energy,
        1.0 / KJ_PER_KCAL,
    ),
    ("Moisture (water) (g)", Nutrient::Water, 1.0),
    ("Protein (g)", Nutrient::Protein, 1.0),
    ("Fat, total (g)", Nutrient::Fat, 1.0),
    ("Total dietary fibre (g)", Nutrient::Fiber, 1.0),
    ("Alcohol (g)", Nutrient::Alcohol, 1.0),
    ("Total sugars (g)", Nutrient::Sugars, 1.0),
    ("Added sugars (g)", Nutrient::AddedSugars, 1.0),
    ("Starch (g)", Nutrient::Starch, 1.0),
    ("Calcium (Ca) (mg)", Nutrient::Calcium, 1.0),
    ("Copper (Cu) (mg)", Nutrient::Copper, 1.0),
    ("Iron (Fe) (mg)", Nutrient::Iron, 1.0),
    ("Magnesium (Mg) (mg)", Nutrient::Magnesium, 1.0),
    ("Manganese (Mn) (mg)", Nutrient::Manganese, 1.0),
    ("Phosphorus (P) (mg)", Nutrient::Phosphorus, 1.0),
    ("Potassium (K) (mg)", Nutrient::Potassium, 1.0),
    ("Selenium (Se) (ug)", Nutrient::Selenium, 1.0),
    ("Sodium (Na) (mg)", Nutrient::Sodium, 1.0),
    ("Zinc (Zn) (mg)", Nutrient::Zinc, 1.0),
    ("Thiamin (B1) (mg)", Nutrient::VitaminB1, 1.0),
    ("Riboflavin (B2) (mg)", Nutrient::VitaminB2, 1.0),
    ("Niacin (B3) (mg)", Nutrient::VitaminB3, 1.0),
    ("Pantothenic acid (B5) (mg)", Nutrient::VitaminB5, 1.0),
    ("Pyridoxine (B6) (mg)", Nutrient::VitaminB6, 1.0),
    ("Cobalamin (B12) (ug)", Nutrient::VitaminB12, 1.0),
    ("Total folates (ug)", Nutrient::Folate, 1.0),
    (
        "Vitamin A retinol equivalents (ug)",
        Nutrient::VitaminA,
        1.0,
    ),
    ("Vitamin C (mg)", Nutrient::VitaminC, 1.0),
    ("Vitamin D3 equivalents (ug)", Nutrient::VitaminD, 1.0),
    (
        "Total saturated fatty acids, equated (g)",
        Nutrient::Saturated,
        1.0,
    ),
    (
        "Total monounsaturated fatty acids, equated (g)",
        Nutrient::Monounsaturated,
        1.0,
    ),
    (
        "Total polyunsaturated fatty acids, equated (g)",
        Nutrient::Polyunsaturated,
        1.0,
    ),
    (
        "Total trans fatty acids, imputed (mg)",
        Nutrient::TransFat,
        0.001,
    ),
    ("C18:3w3 (g)", Nutrient::Omega3Ala, 1.0),
    ("C20:5w3 (mg)", Nutrient::Omega3Epa, 0.001),
    ("C22:6w3 (mg)", Nutrient::Omega3Dha, 0.001),
    ("Cystine plus cysteine (mg)", Nutrient::Cysteine, 0.001),
    ("Histidine (mg)", Nutrient::Histidine, 0.001),
    ("Isoleucine (mg)", Nutrient::Isoleucine, 0.001),
    ("Leucine (mg)", Nutrient::Leucine, 0.001),
    ("Lysine (mg)", Nutrient::Lysine, 0.001),
    ("Methionine (mg)", Nutrient::Methionine, 0.001),
    ("Phenylalanine (mg)", Nutrient::Phenylalanine, 0.001),
    ("Threonine (mg)", Nutrient::Threonine, 0.001),
    ("Tryptophan (mg)", Nutrient::Tryptophan, 0.001),
    ("Tyrosine (mg)", Nutrient::Tyrosine, 0.001),
    ("Valine (mg)", Nutrient::Valine, 0.001),
    ("Caffeine (mg)", Nutrient::Caffeine, 1.0),
    ("Cholesterol (mg)", Nutrient::Cholesterol, 1.0),
];

const AVAILABLE_CARBS: &str = "Available carbohydrate, with sugar alcohols (g)";
const ALPHA_TOCOPHEROL: &str = "Alpha tocopherol (mg)";
const VITAMIN_E: &str = "Vitamin E (mg)";
const DPA: &str = "C22:5w3 (mg)";
const LINOLEIC: &str = "C18:2w6 (g)";
const ARACHIDONIC: &str = "C20:4w6 (mg)";

/// Collapses the line breaks and repeated spaces in the workbook's headers.
fn normalize(header: &str) -> String {
    header.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn number(cell: Option<&Data>) -> Option<f64> {
    match cell? {
        Data::Float(v) => Some(*v),
        Data::Int(v) => Some(*v as f64),
        Data::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn text(cell: Option<&Data>) -> Option<String> {
    match cell? {
        Data::String(s) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
        Data::Float(v) => Some(v.to_string()),
        Data::Int(v) => Some(v.to_string()),
        _ => None,
    }
}

/// Builds a nutrient profile from one row, given header → column positions.
fn profile(columns: &HashMap<String, usize>, row: &[Data]) -> NutrientProfile {
    let get = |header: &str| columns.get(header).and_then(|&i| number(row.get(i)));
    let mut p = NutrientProfile::new();
    for (header, nutrient, scale) in DIRECT {
        if let Some(v) = get(header) {
            p.set(*nutrient, v * scale);
        }
    }
    set_first(
        &mut p,
        Nutrient::VitaminE,
        &[get(ALPHA_TOCOPHEROL), get(VITAMIN_E)],
        1.0,
    );
    // AFCD reports available carbohydrate; store total carbohydrate like USDA.
    if let Some(available) = get(AVAILABLE_CARBS) {
        p.set(
            Nutrient::Carbs,
            available + get("Total dietary fibre (g)").unwrap_or(0.0),
        );
    }
    let n3 = sum_present(&[
        p.get(Nutrient::Omega3Ala),
        p.get(Nutrient::Omega3Epa),
        p.get(Nutrient::Omega3Dha),
        get(DPA).map(|v| v * 0.001),
    ]);
    if let Some(v) = n3 {
        p.set(Nutrient::Omega3, v);
    }
    if let Some(v) = sum_present(&[get(LINOLEIC), get(ARACHIDONIC).map(|v| v * 0.001)]) {
        p.set(Nutrient::Omega6, v);
    }
    p
}

/// Imports every food from the AFCD "Nutrient profiles" workbook.
pub fn import(
    workbook: &Path,
    writer: &mut FoodDbWriter,
    seen: &mut HashSet<String>,
) -> Result<usize> {
    let mut wb = open_workbook_auto(workbook)?;
    let range = wb.worksheet_range(SHEET)?;
    let mut rows = range.rows();

    // The header row is the first one that starts with "Public Food Key".
    let header = rows
        .by_ref()
        .find(|r| text(r.first()).as_deref() == Some("Public Food Key"))
        .ok_or("AFCD header row not found")?;
    let columns: HashMap<String, usize> = header
        .iter()
        .enumerate()
        .filter_map(|(i, c)| text(Some(c)).map(|h| (normalize(&h), i)))
        .collect();
    for (h, _, _) in DIRECT {
        if !columns.contains_key(*h) {
            return Err(format!("AFCD column missing: {h}").into());
        }
    }
    let name_col = *columns
        .get("Food Name")
        .ok_or("AFCD column missing: Food Name")?;

    let mut imported = 0;
    for row in rows {
        let (Some(key), Some(name)) = (text(row.first()), text(row.get(name_col))) else {
            continue;
        };
        if !seen.insert(format!("afcd:{}", name.to_lowercase())) {
            continue;
        }
        let record = FoodRecord {
            dataset: Dataset::Afcd,
            source_id: key,
            name,
            brand: None,
            barcode: None,
            regions: region::AU,
            branded: false,
            popularity: 0.0,
            per_100g: profile(&columns, row),
            servings: Vec::new(),
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
    fn headers_are_normalized() {
        assert_eq!(
            normalize("Energy with dietary fibre, equated \n(kJ)"),
            "Energy with dietary fibre, equated (kJ)"
        );
        assert_eq!(normalize("Malic acid\n (g)"), "Malic acid (g)");
    }

    #[test]
    fn energy_carbs_and_fatty_acids_convert() {
        let headers = [
            "Energy with dietary fibre, equated (kJ)",
            "Total dietary fibre (g)",
            AVAILABLE_CARBS,
            "C18:3w3 (g)",
            "C20:5w3 (mg)",
            DPA,
            "Histidine (mg)",
        ];
        let columns: HashMap<String, usize> = headers
            .iter()
            .enumerate()
            .map(|(i, h)| (h.to_string(), i))
            .collect();
        let row = [
            Data::Float(418.4),
            Data::Float(3.0),
            Data::Float(20.0),
            Data::Float(0.2),
            Data::Int(100),
            Data::Empty,
            Data::Int(500),
        ];
        let p = profile(&columns, &row);
        assert!((p.get(Nutrient::Energy).unwrap() - 100.0).abs() < 1e-9);
        assert_eq!(p.get(Nutrient::Carbs), Some(23.0));
        assert!((p.get(Nutrient::Omega3).unwrap() - 0.3).abs() < 1e-9);
        assert_eq!(p.get(Nutrient::Histidine), Some(0.5));
    }
}
