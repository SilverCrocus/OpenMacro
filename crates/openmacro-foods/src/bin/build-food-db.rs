//! Builds foods.db from the raw public datasets.
//!
//! ```text
//! build-food-db <raw-dir> <out.db> [--skip-off] [--us-min-scans N]
//! ```
//!
//! `<raw-dir>` layout (see docs/FOOD_DATA.md):
//! - `usda/<extracted FoodData Central CSV folders>`
//! - `afcd-r3/AFCD Release 3 - Nutrient profiles.xlsx`
//! - `off/en.openfoodfacts.org.products.csv.gz`

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use openmacro_foods::import::{self, afcd, off, usda};
use openmacro_foods::{Dataset, FoodDb, FoodDbWriter};

/// Finds the extracted folder whose name contains `marker` and holds food.csv.
fn usda_dir(raw: &Path, marker: &str) -> import::Result<PathBuf> {
    let mut stack = vec![raw.join("usda")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if name.contains(marker) && path.join("food.csv").exists() {
                    return Ok(path);
                }
                stack.push(path);
            }
        }
    }
    Err(format!("no USDA folder matching {marker} under {}", raw.display()).into())
}

fn main() -> import::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let flag_value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
    };
    let [raw, out] = positional[..2] else {
        eprintln!("usage: build-food-db <raw-dir> <out.db> [--skip-off] [--us-min-scans N]");
        std::process::exit(2);
    };
    let (raw, out) = (Path::new(raw), Path::new(out));
    let skip_off = args.iter().any(|a| a == "--skip-off");
    let us_min_scans = flag_value("--us-min-scans")
        .map(|v| v.parse())
        .transpose()?
        .unwrap_or(1);

    let started = Instant::now();
    let mut writer = FoodDbWriter::create(out)?;
    let mut seen = HashSet::new();

    // Earlier datasets win on duplicate names: FNDDS names read best.
    for (dataset, marker, data_type) in [
        (Dataset::UsdaFndds, "survey_food", "survey_fndds_food"),
        (
            Dataset::UsdaFoundation,
            "foundation_food",
            "foundation_food",
        ),
        (Dataset::UsdaSrLegacy, "sr_legacy_food", "sr_legacy_food"),
    ] {
        let dir = usda_dir(raw, marker)?;
        let n = usda::import(&dir, dataset, data_type, &mut writer, &mut seen)?;
        println!("{:<16} {n:>7} foods", dataset.code());
    }

    let afcd_path = raw.join("afcd-r3/AFCD Release 3 - Nutrient profiles.xlsx");
    let n = afcd::import(&afcd_path, &mut writer, &mut seen)?;
    println!("{:<16} {n:>7} foods", Dataset::Afcd.code());

    if !skip_off {
        let stats = off::import(
            &raw.join("off/en.openfoodfacts.org.products.csv.gz"),
            off::Options { us_min_scans },
            &mut writer,
        )?;
        println!(
            "{:<16} {:>7} products ({} rows read, {} AU/US)",
            Dataset::OpenFoodFacts.code(),
            stats.imported,
            stats.rows,
            stats.in_region
        );
    }

    let conn = writer.finish(&[
        ("schema_version", "1"),
        ("built_at", &chrono::Utc::now().date_naive().to_string()),
        (
            "attribution",
            "USDA FoodData Central (public domain, CC0). \
             Australian Food Composition Database Release 3, Food Standards Australia New Zealand (CC BY 4.0). \
             Open Food Facts contributors, openfoodfacts.org (ODbL; contents DbCL).",
        ),
    ])?;
    conn.execute_batch("VACUUM")?;
    drop(conn);

    let db = FoodDb::open(out)?;
    let size = std::fs::metadata(out)?.len() as f64 / 1_048_576.0;
    println!(
        "wrote {} foods to {} ({size:.1} MB) in {:.0?}",
        db.count()?,
        out.display(),
        started.elapsed()
    );
    Ok(())
}
