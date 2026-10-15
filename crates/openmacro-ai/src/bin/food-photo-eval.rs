//! Measures how well vision models log food from photos, using dishes from
//! Google's Nutrition5k dataset (CC BY 4.0), whose weights and nutrients
//! were measured.
//!
//! ```text
//! food-photo-eval fetch <dir> [--count 100]
//! food-photo-eval models [--keys <file>] [--filter <text>]
//! food-photo-eval run <dir> --model provider:model[@effort]... [--keys <file>] [--jobs 4] [--limit n]
//! food-photo-eval report <dir> --foods <foods.db> [--region au|us] [--price spec=in,out]...
//! ```
//!
//! API keys come from the environment or a file of `KEY=value` lines
//! (GEMINI_API_KEY, ANTHROPIC_API_KEY, OPENAI_API_KEY, OPENROUTER_API_KEY). Answers are saved
//! in `<dir>/runs`, so a rerun only asks about photos not yet answered,
//! and reports (say, after improving the matching) cost nothing.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use openmacro_ai::matching::match_items;
use openmacro_ai::{Estimate, ModelChoice, Provider, error_message};
use openmacro_core::nutrients::{Nutrient, NutrientProfile};
use openmacro_foods::{FoodDb, region};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const BUCKET: &str = "https://storage.googleapis.com/nutrition5k_dataset/nutrition5k_dataset";
const LISTING: &str = "https://storage.googleapis.com/storage/v1/b/nutrition5k_dataset/o";

type Result<T> = std::result::Result<T, String>;

/// A dish with its measured totals.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Dish {
    id: String,
    kcal: f64,
    grams: f64,
    protein: f64,
    fat: f64,
    carbs: f64,
    /// Ingredient names and grams.
    ingredients: Vec<(String, f64)>,
}

/// One model's answer about one dish.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Answer {
    dish: String,
    seconds: f64,
    result: std::result::Result<Estimate, String>,
    /// What the model wrote, to check answers that didn't parse as expected.
    #[serde(default)]
    text: Option<String>,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let outcome = match args.first().map(String::as_str) {
        Some("fetch") => fetch(&args[1..]),
        Some("models") => models(&args[1..]),
        Some("run") => run(&args[1..]),
        Some("report") => report(&args[1..]),
        _ => Err(
            "usage: food-photo-eval fetch|models|run|report (see the source for options)".into(),
        ),
    };
    if let Err(message) = outcome {
        eprintln!("error: {message}");
        std::process::exit(1);
    }
}

/// The values given for `--name`, and the arguments that aren't options.
fn options(args: &[String], name: &str) -> Vec<String> {
    args.windows(2)
        .filter(|w| w[0] == name)
        .map(|w| w[1].clone())
        .collect()
}

fn option(args: &[String], name: &str) -> Option<String> {
    options(args, name).pop()
}

/// The first argument that isn't an option or an option's value.
fn positional(args: &[String]) -> Result<PathBuf> {
    let mut skip = false;
    for a in args {
        if skip {
            skip = false;
        } else if a.starts_with("--") {
            skip = true;
        } else {
            return Ok(PathBuf::from(a));
        }
    }
    Err("missing the data directory".into())
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(240)))
        .http_status_as_error(false)
        .build()
        .into()
}

fn get_bytes(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>> {
    let mut response = agent.get(url).call().map_err(|e| format!("{url}: {e}"))?;
    if response.status() != 200 {
        return Err(format!("{url}: HTTP {}", response.status()));
    }
    response
        .body_mut()
        .with_config()
        .limit(64 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| format!("{url}: {e}"))
}

/// Dishes in a Nutrition5k metadata file: totals, then groups of seven
/// columns per ingredient (id, name, grams, kcal, fat, carbs, protein).
fn parse_metadata(text: &str) -> Result<Vec<Dish>> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());
    let mut dishes = Vec::new();
    for record in reader.records() {
        let r = record.map_err(|e| e.to_string())?;
        let num = |i: usize| r.get(i).and_then(|v| v.trim().parse::<f64>().ok());
        let (Some(kcal), Some(grams), Some(fat), Some(carbs), Some(protein)) =
            (num(1), num(2), num(3), num(4), num(5))
        else {
            continue;
        };
        let ingredients = (6..r.len())
            .step_by(7)
            .filter_map(|i| Some((r.get(i + 1)?.to_string(), num(i + 2)?)))
            .collect();
        dishes.push(Dish {
            id: r[0].to_string(),
            kcal,
            grams,
            protein,
            fat,
            carbs,
            ingredients,
        });
    }
    Ok(dishes)
}

/// Downloads an evenly spread sample of test-split dishes that have an
/// overhead photo, with their measured nutrition.
fn fetch(args: &[String]) -> Result<()> {
    let dir = positional(args)?;
    let count: usize = option(args, "--count")
        .map(|c| c.parse().map_err(|_| "--count must be a number"))
        .transpose()?
        .unwrap_or(100);
    let agent = agent();

    let mut dishes: HashMap<String, Dish> = HashMap::new();
    for cafe in ["cafe1", "cafe2"] {
        let url = format!("{BUCKET}/metadata/dish_metadata_{cafe}.csv");
        let text = String::from_utf8_lossy(&get_bytes(&agent, &url)?).into_owned();
        for dish in parse_metadata(&text)? {
            dishes.insert(dish.id.clone(), dish);
        }
    }
    let test_ids = String::from_utf8_lossy(&get_bytes(
        &agent,
        &format!("{BUCKET}/dish_ids/splits/rgb_test_ids.txt"),
    )?)
    .into_owned();
    let test: HashSet<&str> = test_ids.lines().map(str::trim).collect();

    // Which dishes have an overhead photo.
    let mut overhead: HashSet<String> = HashSet::new();
    let mut token: Option<String> = None;
    loop {
        let mut request = agent
            .get(LISTING)
            .query("prefix", "nutrition5k_dataset/imagery/realsense_overhead/")
            .query("fields", "items(name),nextPageToken");
        if let Some(t) = &token {
            request = request.query("pageToken", t);
        }
        let page: Value = request
            .call()
            .map_err(|e| e.to_string())?
            .body_mut()
            .read_json()
            .map_err(|e| e.to_string())?;
        for item in page["items"].as_array().into_iter().flatten() {
            let id = item["name"]
                .as_str()
                .and_then(|name| name.strip_suffix("/rgb.png"))
                .and_then(|rest| rest.rsplit('/').next());
            if let Some(id) = id {
                overhead.insert(id.to_string());
            }
        }
        match page["nextPageToken"].as_str() {
            Some(t) => token = Some(t.to_string()),
            None => break,
        }
    }

    let mut candidates: Vec<&Dish> = dishes
        .values()
        .filter(|d| test.contains(d.id.as_str()) && overhead.contains(&d.id))
        // Nearly empty plates make percentage errors meaningless.
        .filter(|d| d.kcal >= 30.0)
        .collect();
    candidates.sort_by(|a, b| a.id.cmp(&b.id));
    if candidates.is_empty() {
        return Err("no dishes found".into());
    }
    let count = count.min(candidates.len());
    let sample: Vec<Dish> = (0..count)
        .map(|i| candidates[i * candidates.len() / count].clone())
        .collect();
    println!(
        "{} test dishes with overhead photos; sampling {count}",
        candidates.len()
    );

    let images = dir.join("images");
    fs::create_dir_all(&images).map_err(|e| e.to_string())?;
    for (i, dish) in sample.iter().enumerate() {
        let path = images.join(format!("{}.png", dish.id));
        if !path.exists() {
            let url = format!("{BUCKET}/imagery/realsense_overhead/{}/rgb.png", dish.id);
            fs::write(&path, get_bytes(&agent, &url)?).map_err(|e| e.to_string())?;
        }
        print!("\r{}/{count} photos", i + 1);
        let _ = std::io::stdout().flush();
    }
    println!();
    let json = serde_json::to_string_pretty(&sample).map_err(|e| e.to_string())?;
    fs::write(dir.join("dishes.json"), json).map_err(|e| e.to_string())
}

/// API keys from a `KEY=value` file (if given) and the environment.
fn keys(args: &[String]) -> Result<HashMap<Provider, String>> {
    let mut found = HashMap::new();
    let file = match option(args, "--keys") {
        Some(path) => fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?,
        None => String::new(),
    };
    for provider in Provider::ALL {
        let variable = provider.key_variable();
        let from_file = file.lines().find_map(|line| {
            let line = line.trim().trim_start_matches("export ").trim();
            let value = line
                .strip_prefix(variable)?
                .trim_start()
                .strip_prefix('=')?;
            Some(value.trim().trim_matches(['"', '\'']).to_string())
        });
        let key = from_file
            .or_else(|| std::env::var(variable).ok())
            .filter(|k| !k.is_empty());
        if let Some(key) = key {
            found.insert(provider, key);
        }
    }
    Ok(found)
}

/// Lists the models each key can use (for OpenRouter, the ones that take
/// images, with their prices), optionally only names containing `--filter`.
fn models(args: &[String]) -> Result<()> {
    let keys = keys(args)?;
    let filter = option(args, "--filter").unwrap_or_default().to_lowercase();
    let agent = agent();
    for provider in Provider::ALL {
        let Some(key) = keys.get(&provider) else {
            println!("{}: no key", provider.label());
            continue;
        };
        let request = match provider {
            Provider::Gemini => agent
                .get("https://generativelanguage.googleapis.com/v1beta/models")
                .query("pageSize", "1000")
                .header("x-goog-api-key", key),
            Provider::Anthropic => agent
                .get("https://api.anthropic.com/v1/models")
                .query("limit", "100")
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01"),
            Provider::OpenAi => agent
                .get("https://api.openai.com/v1/models")
                .header("authorization", format!("Bearer {key}")),
            Provider::OpenRouter => agent
                .get("https://openrouter.ai/api/v1/models")
                .header("authorization", format!("Bearer {key}")),
        };
        let reply: Value = request
            .call()
            .map_err(|e| e.to_string())?
            .body_mut()
            .read_json()
            .map_err(|e| e.to_string())?;
        if let Some(message) = error_message(&reply) {
            println!("{}: {message}", provider.label());
            continue;
        }
        let mut names: Vec<String> = match provider {
            Provider::Gemini => reply["models"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|m| {
                    m["supportedGenerationMethods"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(|x| x == "generateContent"))
                })
                .filter_map(|m| m["name"].as_str())
                .map(|n| n.trim_start_matches("models/").to_string())
                .collect(),
            Provider::OpenRouter => reply["data"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|m| {
                    m["architecture"]["input_modalities"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(|x| x == "image"))
                })
                .filter_map(|m| {
                    // Prices are in dollars per token, as text.
                    let per_million = |v: &Value| {
                        v.as_str()
                            .and_then(|p| p.parse::<f64>().ok())
                            .unwrap_or(0.0)
                            * 1e6
                    };
                    Some(format!(
                        "{}  (${:.2} in / ${:.2} out per 1M tokens)",
                        m["id"].as_str()?,
                        per_million(&m["pricing"]["prompt"]),
                        per_million(&m["pricing"]["completion"])
                    ))
                })
                .collect(),
            _ => reply["data"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|m| m["id"].as_str().map(str::to_string))
                .collect(),
        };
        names.retain(|n| n.to_lowercase().contains(&filter));
        names.sort();
        println!("{} ({}):", provider.label(), provider.code());
        for name in names {
            println!("  {name}");
        }
    }
    Ok(())
}

fn load_dishes(dir: &Path) -> Result<Vec<Dish>> {
    let text = fs::read_to_string(dir.join("dishes.json"))
        .map_err(|e| format!("{}: {e} (run fetch first)", dir.display()))?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

fn run_file(dir: &Path, choice: &ModelChoice) -> PathBuf {
    let name: String = choice
        .spec()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    dir.join("runs").join(format!("{name}.jsonl"))
}

fn load_answers(path: &Path) -> Vec<Answer> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// Asks one model about one photo, retrying when rate limited or the
/// service is busy. Returns the provider's successful reply.
fn ask(agent: &ureq::Agent, choice: &ModelChoice, key: &str, image: &[u8]) -> Result<Value> {
    let request = openmacro_ai::request(choice, key, image, "image/png", None);
    let mut wait = Duration::from_secs(10);
    for attempt in 1.. {
        let mut builder = agent.post(&request.url);
        for (name, value) in &request.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        let mut response = builder
            .send_json(&request.body)
            .map_err(|e| e.to_string())?;
        let status = response.status().as_u16();
        let reply: Value = response
            .body_mut()
            .with_config()
            .limit(16 * 1024 * 1024)
            .read_json()
            .map_err(|e| format!("HTTP {status}: {e}"))?;
        if matches!(status, 429 | 500 | 502 | 503 | 504 | 529) && attempt < 6 {
            std::thread::sleep(wait);
            wait *= 2;
            continue;
        }
        if status != 200 {
            return Err(format!(
                "HTTP {status}: {}",
                error_message(&reply).unwrap_or_else(|| reply.to_string())
            ));
        }
        return Ok(reply);
    }
    unreachable!()
}

/// Asks each model about every photo not yet answered.
fn run(args: &[String]) -> Result<()> {
    let dir = positional(args)?;
    let dishes = load_dishes(&dir)?;
    let keys = keys(args)?;
    let jobs: usize = option(args, "--jobs")
        .and_then(|j| j.parse().ok())
        .unwrap_or(4)
        .max(1);
    // Only the first n dishes, e.g. to try a model on a few photos first.
    let limit: usize = option(args, "--limit")
        .and_then(|n| n.parse().ok())
        .unwrap_or(usize::MAX);
    let choices = options(args, "--model")
        .iter()
        .map(|s| ModelChoice::parse(s).ok_or(format!("bad model {s:?}")))
        .collect::<Result<Vec<_>>>()?;
    if choices.is_empty() {
        return Err("give at least one --model provider:model".into());
    }
    fs::create_dir_all(dir.join("runs")).map_err(|e| e.to_string())?;
    let agent = agent();

    for choice in &choices {
        let key = keys
            .get(&choice.provider)
            .ok_or(format!("no {} key", choice.provider.key_variable()))?;
        let path = run_file(&dir, choice);
        let done: HashSet<String> = load_answers(&path)
            .into_iter()
            .filter(|a| a.result.is_ok())
            .map(|a| a.dish)
            .collect();
        let todo: Vec<&Dish> = dishes
            .iter()
            .take(limit)
            .filter(|d| !done.contains(&d.id))
            .collect();
        println!("{}: {} to ask", choice.spec(), todo.len());
        let queue = Mutex::new(todo.into_iter());
        let file = Mutex::new(
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|e| e.to_string())?,
        );
        let failures_in_a_row = Mutex::new(0usize);
        std::thread::scope(|scope| {
            for _ in 0..jobs {
                scope.spawn(|| {
                    loop {
                        if *failures_in_a_row.lock().unwrap() >= 5 {
                            return;
                        }
                        let Some(dish) = queue.lock().unwrap().next() else {
                            return;
                        };
                        let image =
                            match fs::read(dir.join("images").join(format!("{}.png", dish.id))) {
                                Ok(image) => image,
                                Err(e) => {
                                    eprintln!("{}: {e}", dish.id);
                                    continue;
                                }
                            };
                        let started = Instant::now();
                        let reply = ask(&agent, choice, key, &image);
                        let answer = Answer {
                            dish: dish.id.clone(),
                            seconds: started.elapsed().as_secs_f64(),
                            text: reply
                                .as_ref()
                                .ok()
                                .and_then(|r| openmacro_ai::answer_text(choice.provider, r)),
                            result: reply.and_then(|r| openmacro_ai::parse(choice.provider, &r)),
                        };
                        match &answer.result {
                            Ok(estimate) => {
                                *failures_in_a_row.lock().unwrap() = 0;
                                println!(
                                    "  {} {:.1} s, {} items",
                                    dish.id,
                                    answer.seconds,
                                    estimate.items.len()
                                );
                            }
                            Err(e) => {
                                *failures_in_a_row.lock().unwrap() += 1;
                                println!("  {} failed: {e}", dish.id);
                            }
                        }
                        let line = serde_json::to_string(&answer).unwrap();
                        let _ = writeln!(file.lock().unwrap(), "{line}");
                    }
                });
            }
        });
        if *failures_in_a_row.lock().unwrap() >= 5 {
            println!("  stopped after 5 failures in a row");
        }
    }
    Ok(())
}

fn median(mut values: Vec<f64>) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    if values.len() % 2 == 1 {
        values[mid]
    } else {
        (values[mid - 1] + values[mid]) / 2.0
    }
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

/// Lin's concordance correlation coefficient: 1 when predictions equal
/// the truth, lower for scatter and for bias.
fn concordance(pred: &[f64], truth: &[f64]) -> f64 {
    let (mp, mt) = (mean(pred), mean(truth));
    let n = pred.len() as f64;
    let cov = pred
        .iter()
        .zip(truth)
        .map(|(p, t)| (p - mp) * (t - mt))
        .sum::<f64>()
        / n;
    let vp = pred.iter().map(|p| (p - mp).powi(2)).sum::<f64>() / n;
    let vt = truth.iter().map(|t| (t - mt).powi(2)).sum::<f64>() / n;
    2.0 * cov / (vp + vt + (mp - mt).powi(2))
}

/// Error measures for one quantity over the dishes.
struct Errors {
    mae: f64,
    median_pct: f64,
    /// Total predicted over total measured, minus one.
    bias_pct: f64,
    ccc: f64,
}

fn errors(pred: &[f64], truth: &[f64]) -> Errors {
    let abs: Vec<f64> = pred.iter().zip(truth).map(|(p, t)| (p - t).abs()).collect();
    let pct: Vec<f64> = abs.iter().zip(truth).map(|(a, t)| a / t * 100.0).collect();
    Errors {
        mae: mean(&abs),
        median_pct: median(pct),
        bias_pct: (pred.iter().sum::<f64>() / truth.iter().sum::<f64>() - 1.0) * 100.0,
        ccc: concordance(pred, truth),
    }
}

/// Scores every saved run against the measured values.
fn report(args: &[String]) -> Result<()> {
    let dir = positional(args)?;
    let dishes: HashMap<String, Dish> = load_dishes(&dir)?
        .into_iter()
        .map(|d| (d.id.clone(), d))
        .collect();
    let foods = option(args, "--foods").ok_or("give --foods <foods.db>")?;
    let db = FoodDb::open(Path::new(&foods)).map_err(|e| e.to_string())?;
    let regions = match option(args, "--region").as_deref() {
        Some("us") => region::US,
        _ => region::AU,
    };
    let prices: HashMap<String, (f64, f64)> = options(args, "--price")
        .iter()
        .filter_map(|p| {
            let (spec, rates) = p.split_once('=')?;
            let (input, output) = rates.split_once(',')?;
            Some((
                spec.to_string(),
                (input.parse().ok()?, output.parse().ok()?),
            ))
        })
        .collect();

    let mut runs: Vec<PathBuf> = fs::read_dir(dir.join("runs"))
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .collect();
    runs.sort();

    let mut out = String::new();
    out.push_str(&format!(
        "Nutrition5k overhead photos, {} dishes; foods matched with the {} region.\n\n",
        dishes.len(),
        if regions == region::US { "US" } else { "AU" }
    ));
    out.push_str("| Model | Answered | Calories: model | Calories: database | Weight | Protein / fat / carbs (model) | Protein / fat / carbs (database) | Matched | Time | Tokens in / out | Per 1,000 photos |\n");
    out.push_str("|---|---|---|---|---|---|---|---|---|---|---|\n");

    for path in &runs {
        let spec = path.file_stem().unwrap().to_string_lossy().to_string();
        // The latest answer per dish wins.
        let mut latest: BTreeMap<String, Answer> = BTreeMap::new();
        for answer in load_answers(path) {
            if dishes.contains_key(&answer.dish) {
                latest.insert(answer.dish.clone(), answer);
            }
        }
        let ok: Vec<(&Dish, &Answer, &Estimate)> = latest
            .values()
            .filter_map(|a| Some((&dishes[&a.dish], a, a.result.as_ref().ok()?)))
            .collect();
        if ok.is_empty() {
            out.push_str(&format!(
                "| {spec} | 0/{} | | | | | | | | | |\n",
                latest.len()
            ));
            continue;
        }

        let mut csv = String::from("dish,kcal,model kcal,database kcal,grams,model grams,items\n");
        let mut truth = [vec![], vec![], vec![], vec![], vec![]];
        let mut model = [vec![], vec![], vec![], vec![], vec![]];
        let mut database = [vec![], vec![], vec![], vec![]];
        let (mut items, mut matched) = (0usize, 0usize);
        for (dish, _, estimate) in &ok {
            let sum = |f: fn(&openmacro_ai::Item) -> f64| estimate.items.iter().map(f).sum::<f64>();
            let found = match_items(&db, &estimate.items, regions);
            items += found.len();
            matched += found.iter().filter(|m| m.food.is_some()).count();
            let mut total = NutrientProfile::new();
            for m in &found {
                total += &m.nutrients();
            }
            for (i, (t, m)) in [
                (dish.kcal, sum(|i| i.kcal)),
                (dish.grams, sum(|i| i.grams)),
                (dish.protein, sum(|i| i.protein_g)),
                (dish.fat, sum(|i| i.fat_g)),
                (dish.carbs, sum(|i| i.carbs_g)),
            ]
            .into_iter()
            .enumerate()
            {
                truth[i].push(t);
                model[i].push(m);
            }
            for (i, n) in [
                Nutrient::Energy,
                Nutrient::Protein,
                Nutrient::Fat,
                Nutrient::Carbs,
            ]
            .into_iter()
            .enumerate()
            {
                database[i].push(total.get_or_zero(n));
            }
            let described: Vec<String> = found
                .iter()
                .map(|m| {
                    let food = m
                        .food
                        .as_ref()
                        .map_or("no match".to_string(), |f| f.name.clone());
                    format!("{} {:.0} g -> {food}", m.item.name, m.item.grams)
                })
                .collect();
            csv.push_str(&format!(
                "{},{:.0},{:.0},{:.0},{:.0},{:.0},\"{}\"\n",
                dish.id,
                dish.kcal,
                model[0].last().unwrap(),
                database[0].last().unwrap(),
                dish.grams,
                model[1].last().unwrap(),
                described.join("; ").replace('"', "'")
            ));
        }
        let _ = fs::write(path.with_extension("csv"), csv);

        let kcal_model = errors(&model[0], &truth[0]);
        let kcal_db = errors(&database[0], &truth[0]);
        let weight = errors(&model[1], &truth[1]);
        let macro_mae = |pred: &[Vec<f64>]| {
            format!(
                "{:.0} / {:.0} / {:.0} g",
                errors(&pred[0], &truth[2]).mae,
                errors(&pred[1], &truth[3]).mae,
                errors(&pred[2], &truth[4]).mae
            )
        };
        let kcal_text = |e: &Errors| {
            format!(
                "{:.0} kcal off; median {:.0}%; bias {:+.0}%; CCC {:.2}",
                e.mae, e.median_pct, e.bias_pct, e.ccc
            )
        };
        let seconds = median(ok.iter().map(|(_, a, _)| a.seconds).collect());
        let tokens_in = mean(
            &ok.iter()
                .map(|(_, _, e)| e.input_tokens as f64)
                .collect::<Vec<_>>(),
        );
        let tokens_out = mean(
            &ok.iter()
                .map(|(_, _, e)| e.output_tokens as f64)
                .collect::<Vec<_>>(),
        );
        let costs: Vec<f64> = ok.iter().filter_map(|(_, _, e)| e.cost_usd).collect();
        let cost = if costs.len() == ok.len() {
            // What the provider billed.
            Some(format!("${:.2}", mean(&costs) * 1000.0))
        } else {
            None
        };
        let cost = cost
            .or_else(|| {
                prices
                    .iter()
                    .filter_map(|(spec, rates)| {
                        Some((run_file(&dir, &ModelChoice::parse(spec)?), rates))
                    })
                    .find(|(file, _)| file == path)
                    .map(|(_, (input, output))| {
                        format!(
                            "${:.2}",
                            (tokens_in * input + tokens_out * output) / 1e6 * 1000.0
                        )
                    })
            })
            .unwrap_or_default();
        out.push_str(&format!(
            "| {spec} | {}/{} | {} | {} | {:.0} g off; median {:.0}%; bias {:+.0}% | {} | {} | {:.0}% | {seconds:.1} s | {tokens_in:.0} / {tokens_out:.0} | {cost} |\n",
            ok.len(),
            latest.len(),
            kcal_text(&kcal_model),
            kcal_text(&kcal_db),
            weight.mae,
            weight.median_pct,
            weight.bias_pct,
            macro_mae(&model[2..]),
            macro_mae(&database[1..]),
            matched as f64 / items.max(1) as f64 * 100.0,
        ));
    }
    print!("{out}");
    fs::write(dir.join("report.md"), out).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_rows_parse() {
        let dishes = parse_metadata(
            "dish_1,300.5,193,12.3,28.2,18.6,ingr_1,soy sauce,3.4,1.8,0.02,0.17,0.28,ingr_2,white rice,8.5,11.0,0.03,2.38,0.23\n",
        )
        .unwrap();
        assert_eq!(dishes[0].id, "dish_1");
        assert_eq!(dishes[0].kcal, 300.5);
        assert_eq!(dishes[0].fat, 12.3);
        assert_eq!(dishes[0].protein, 18.6);
        assert_eq!(dishes[0].ingredients[1], ("white rice".to_string(), 8.5));
    }

    #[test]
    fn concordance_is_one_for_a_perfect_match_and_lower_with_bias() {
        let truth = [100.0, 200.0, 300.0];
        assert!((concordance(&truth, &truth) - 1.0).abs() < 1e-12);
        let high = [150.0, 250.0, 350.0];
        assert!(concordance(&high, &truth) < 0.9);
    }
}
