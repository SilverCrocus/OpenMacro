//! Retail barcodes: decoding camera frames, normalising codes and reading
//! Open Food Facts' online product JSON.

use std::collections::HashSet;

use openmacro_core::food::{Food, FoodSource, Serving};
use openmacro_core::nutrients::{Nutrient, NutrientProfile};
use rxing::{BarcodeFormat, DecodeHints, helpers};
use serde_json::Value;

/// Decodes an EAN/UPC barcode from an 8-bit grayscale frame.
pub fn decode_luma(luma: Vec<u8>, width: u32, height: u32) -> Option<String> {
    let mut hints = DecodeHints {
        TryHarder: Some(true),
        PossibleFormats: Some(HashSet::from([
            BarcodeFormat::EAN_13,
            BarcodeFormat::EAN_8,
            BarcodeFormat::UPC_A,
            BarcodeFormat::UPC_E,
        ])),
        ..Default::default()
    };
    let result = helpers::detect_in_luma_with_hints(luma, width, height, None, &mut hints).ok()?;
    let code = result.getText().trim().to_string();
    valid_checksum(&code).then_some(code)
}

/// GTIN (EAN-8/12/13/14) check digit.
pub fn valid_checksum(code: &str) -> bool {
    let digits: Vec<u32> = code.chars().filter_map(|c| c.to_digit(10)).collect();
    if digits.len() != code.len() || ![8, 12, 13, 14].contains(&digits.len()) {
        return false;
    }
    let (check, body) = digits.split_last().expect("non-empty");
    let sum: u32 = body
        .iter()
        .rev()
        .enumerate()
        .map(|(i, d)| if i % 2 == 0 { d * 3 } else { *d })
        .sum();
    (10 - sum % 10) % 10 == *check
}

/// The same product can be stored as UPC-A (12 digits) or EAN-13 with a
/// leading zero; try both forms.
pub fn lookup_variants(code: &str) -> Vec<String> {
    let code = code.trim();
    let mut variants = vec![code.to_string()];
    if code.len() == 12 {
        variants.push(format!("0{code}"));
    } else if code.len() == 13 && code.starts_with('0') {
        variants.push(code[1..].to_string());
    }
    variants
}

/// Builds a food from Open Food Facts' `/api/v2/product/<code>` JSON, or
/// `None` if the product is missing or lacks headline nutrition.
pub fn food_from_off(code: &str, json: &Value) -> Option<Food> {
    if json["status"].as_i64() != Some(1) {
        return None;
    }
    let product = &json["product"];
    let n = &product["nutriments"];
    let number = |key: &str| {
        n[key]
            .as_f64()
            .or_else(|| n[key].as_str().and_then(|s| s.parse().ok()))
    };

    let mut per_100g = NutrientProfile::new();
    let energy =
        number("energy-kcal_100g").or_else(|| number("energy_100g").map(|kj| kj / 4.184))?;
    per_100g.set(Nutrient::Energy, energy);
    for (key, nutrient, scale) in [
        ("proteins_100g", Nutrient::Protein, 1.0),
        ("fat_100g", Nutrient::Fat, 1.0),
        ("fiber_100g", Nutrient::Fiber, 1.0),
        ("sugars_100g", Nutrient::Sugars, 1.0),
        ("saturated-fat_100g", Nutrient::Saturated, 1.0),
        ("sodium_100g", Nutrient::Sodium, 1_000.0),
    ] {
        if let Some(v) = number(key) {
            per_100g.set(nutrient, v * scale);
        }
    }
    let carbs = number("carbohydrates_100g")?;
    // Australian labels give available carbohydrate; store the total.
    let australian_only = product["countries_tags"].as_array().is_some_and(|tags| {
        tags.iter().any(|t| t == "en:australia") && !tags.iter().any(|t| t == "en:united-states")
    });
    let fiber = if australian_only {
        per_100g.get_or_zero(Nutrient::Fiber)
    } else {
        0.0
    };
    per_100g.set(Nutrient::Carbs, carbs + fiber);
    per_100g.get(Nutrient::Protein)?;
    per_100g.get(Nutrient::Fat)?;

    let name = product["product_name"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    let brand = product["brands"]
        .as_str()
        .and_then(|b| b.split(',').next())
        .map(str::trim)
        .filter(|b| !b.is_empty());
    let serving = product["serving_quantity"]
        .as_f64()
        .or_else(|| {
            product["serving_quantity"]
                .as_str()
                .and_then(|s| s.parse().ok())
        })
        .filter(|g| *g > 0.0 && *g < 2_000.0);
    Some(Food {
        id: format!("off:{code}"),
        name: name.to_string(),
        brand: brand.map(str::to_string),
        source: FoodSource::Branded,
        barcode: Some(code.to_string()),
        per_100g,
        servings: serving
            .map(|g| Serving::new("serving", g))
            .into_iter()
            .collect(),
    })
}

/// Looks a barcode up on Open Food Facts through the web view.
pub async fn fetch_off(code: &str) -> Result<Option<Food>, String> {
    if !code.chars().all(|c| c.is_ascii_digit()) {
        return Ok(None);
    }
    let script = format!(
        r#"try {{
            const r = await fetch("https://world.openfoodfacts.org/api/v2/product/{code}.json?fields=product_name,brands,nutriments,serving_quantity,countries_tags");
            return {{ status: r.status, body: r.ok ? await r.json() : null }};
        }} catch (e) {{
            return {{ status: 0, body: null }};
        }}"#
    );
    let reply: Value = dioxus::prelude::document::eval(&script)
        .join()
        .await
        .map_err(|e| format!("Lookup failed: {e}"))?;
    match reply["status"].as_u64() {
        Some(200) => Ok(food_from_off(code, &reply["body"])),
        Some(404) => Ok(None),
        _ => Err("Couldn't reach Open Food Facts. Check your connection.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rxing::{MultiFormatWriter, Writer};
    use serde_json::json;

    /// Renders `code` as a grayscale image with a quiet zone and some blur-like noise.
    fn render(code: &str, format: BarcodeFormat) -> (Vec<u8>, u32, u32) {
        let matrix = MultiFormatWriter.encode(code, &format, 300, 120).unwrap();
        let (w, h) = (matrix.getWidth(), matrix.getHeight());
        let mut luma = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                let base: i32 = if matrix.get(x, y) { 30 } else { 225 };
                let noise = ((x * 7 + y * 13) % 17) as i32 - 8;
                luma.push((base + noise).clamp(0, 255) as u8);
            }
        }
        (luma, w, h)
    }

    #[test]
    fn decodes_rendered_ean13_and_upca() {
        let (luma, w, h) = render("9300652010053", BarcodeFormat::EAN_13);
        assert_eq!(decode_luma(luma, w, h).as_deref(), Some("9300652010053"));
        let (luma, w, h) = render("041196910759", BarcodeFormat::UPC_A);
        let decoded = decode_luma(luma, w, h).unwrap();
        assert!(
            lookup_variants(&decoded).contains(&"041196910759".to_string()),
            "{decoded}"
        );
    }

    #[test]
    fn blank_frames_decode_to_nothing() {
        assert_eq!(decode_luma(vec![200; 320 * 240], 320, 240), None);
    }

    #[test]
    fn checksums() {
        assert!(valid_checksum("9300652010053"));
        assert!(!valid_checksum("9300652010054"));
        assert!(valid_checksum("041196910759"));
        assert!(valid_checksum("96385074"));
        assert!(!valid_checksum("12345"));
        assert!(!valid_checksum("93006520100a7"));
    }

    #[test]
    fn variants_cover_upc_and_ean_forms() {
        assert_eq!(
            lookup_variants("041196910759"),
            ["041196910759", "0041196910759"]
        );
        assert_eq!(
            lookup_variants("0041196910759"),
            ["0041196910759", "041196910759"]
        );
        assert_eq!(lookup_variants("9300652010053"), ["9300652010053"]);
    }

    #[test]
    fn open_food_facts_json_becomes_a_food() {
        let body = json!({
            "status": 1,
            "product": {
                "product_name": "Weet-Bix",
                "brands": "Sanitarium,Weet-Bix",
                "countries_tags": ["en:australia"],
                "serving_quantity": "30",
                "nutriments": {
                    "energy_100g": 1490, "proteins_100g": 12.4, "fat_100g": 1.4,
                    "carbohydrates_100g": 61.0, "fiber_100g": 11.0, "sodium_100g": 0.27
                }
            }
        });
        let food = food_from_off("9300652010053", &body).unwrap();
        assert_eq!(food.brand.as_deref(), Some("Sanitarium"));
        assert!((food.per_100g.get(Nutrient::Energy).unwrap() - 356.1).abs() < 0.1);
        assert_eq!(food.per_100g.get(Nutrient::Carbs), Some(72.0));
        assert_eq!(food.per_100g.get(Nutrient::Sodium), Some(270.0));
        assert_eq!(food.servings, vec![Serving::new("serving", 30.0)]);
        assert!(food_from_off("1", &json!({ "status": 0 })).is_none());
    }
}
