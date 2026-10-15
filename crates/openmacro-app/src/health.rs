//! Health Connect, Android's shared store for health data.
//!
//! OpenMacro reads weight and body fat that other apps saved (a smart
//! scale's app, say) and writes its food log, water, weigh-ins and body fat
//! for other apps to show. Each sync covers the last [`WINDOW_DAYS`] days:
//! - readings not seen before become weigh-ins and measurements. One the
//!   user deleted still counts as seen, so it stays deleted.
//! - days whose records changed since the last sync are written again,
//!   replacing everything OpenMacro saved for those days. A weigh-in that
//!   matches another app's reading came from it, so it isn't written back.
//!
//! The Android calls are in `bridge`; the rest is plain logic.

use std::collections::BTreeMap;

use chrono::{Duration, NaiveDate, NaiveDateTime};
use dioxus::core::spawn_forever;
use dioxus::prelude::*;
use openmacro_core::body::{Measurement, MeasurementKind};
use openmacro_core::log::LogEntry;
use openmacro_core::nutrients::{Nutrient, NutrientUnit};
use openmacro_core::trend::WeighIn;
use serde_json::{Value, json};

use crate::state::{AppState, WATER_ID, today};

/// How many days each sync reads and writes, today included.
pub const WINDOW_DAYS: i64 = 30;

/// Weigh-ins and body fat have no time of day; they're written at this hour.
const MORNING_HOUR: u32 = 8;

/// The permissions OpenMacro asks for, as Health Connect names them.
pub const PERMISSIONS: [(&str, &str); 6] = [
    ("READ_WEIGHT", "weight"),
    ("READ_BODY_FAT", "body fat"),
    ("WRITE_NUTRITION", "food"),
    ("WRITE_HYDRATION", "water"),
    ("WRITE_WEIGHT", "weight"),
    ("WRITE_BODY_FAT", "body fat"),
];

/// Saved with the other settings.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct HealthSettings {
    /// The user turned syncing on.
    pub enabled: bool,
    /// A fingerprint of what was last written for each day.
    pub written: BTreeMap<NaiveDate, u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Availability {
    /// Not checked yet.
    #[default]
    Unknown,
    /// Not on this phone (or not Android).
    Unsupported,
    /// Health Connect must be installed or updated from the Play Store.
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    NeedsInstall,
    Ready,
}

/// When the last sync ended, and why it failed if it did.
#[derive(Debug, Clone, PartialEq)]
pub struct Synced {
    pub at: NaiveDateTime,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HealthStatus {
    pub availability: Availability,
    /// Permissions granted, like "READ_WEIGHT".
    pub granted: Vec<String>,
    pub syncing: bool,
    /// Something changed during a sync, so another one follows it.
    again: bool,
    pub last: Option<Synced>,
    /// When a sync last added or sent anything, and what.
    pub last_change: Option<(NaiveDateTime, String)>,
}

pub static SETTINGS: GlobalSignal<HealthSettings> = Signal::global(HealthSettings::default);
pub static STATUS: GlobalSignal<HealthStatus> = Signal::global(HealthStatus::default);

/// A reading another app saved: kilograms for weight, percent for body fat.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    pub date: NaiveDate,
    pub value: f64,
}

/// What other apps saved in the window.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Others {
    pub weights: Vec<Reading>,
    pub body_fat: Vec<Reading>,
}

/// Readings that are the same to within rounding.
fn same(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.005
}

/// The readings to add: those not `seen` before, each once.
pub fn new_readings(readings: &[Reading], seen: impl Fn(&Reading) -> bool) -> Vec<Reading> {
    let mut out: Vec<Reading> = Vec::new();
    for r in readings {
        if !seen(r)
            && !out
                .iter()
                .any(|o| o.date == r.date && same(o.value, r.value))
        {
            out.push(*r);
        }
    }
    out
}

/// Everything OpenMacro writes for some days, in the bridge's JSON shape.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Records {
    pub food: Vec<Value>,
    pub water: Vec<Value>,
    pub weight: Vec<Value>,
    pub body_fat: Vec<Value>,
}

impl Records {
    fn is_empty(&self) -> bool {
        self.food.is_empty()
            && self.water.is_empty()
            && self.weight.is_empty()
            && self.body_fat.is_empty()
    }

    fn extend(&mut self, other: &Records) {
        self.food.extend_from_slice(&other.food);
        self.water.extend_from_slice(&other.water);
        self.weight.extend_from_slice(&other.weight);
        self.body_fat.extend_from_slice(&other.body_fat);
    }

    pub fn to_json(&self) -> Value {
        json!({
            "food": self.food,
            "water": self.water,
            "weight": self.weight,
            "bodyFat": self.body_fat,
        })
    }
}

/// Health Connect's field for each nutrient it has one for.
fn field(n: Nutrient) -> Option<&'static str> {
    use Nutrient::*;
    Some(match n {
        Energy => "energy",
        Protein => "protein",
        Carbs => "totalCarbohydrate",
        Fat => "totalFat",
        Saturated => "saturatedFat",
        TransFat => "transFat",
        Monounsaturated => "monounsaturatedFat",
        Polyunsaturated => "polyunsaturatedFat",
        Cholesterol => "cholesterol",
        Fiber => "dietaryFiber",
        Sugars => "sugar",
        Sodium => "sodium",
        Potassium => "potassium",
        Calcium => "calcium",
        Iron => "iron",
        Magnesium => "magnesium",
        Manganese => "manganese",
        Phosphorus => "phosphorus",
        Selenium => "selenium",
        Copper => "copper",
        Zinc => "zinc",
        VitaminA => "vitaminA",
        VitaminC => "vitaminC",
        VitaminD => "vitaminD",
        VitaminE => "vitaminE",
        VitaminK => "vitaminK",
        VitaminB1 => "thiamin",
        VitaminB2 => "riboflavin",
        VitaminB3 => "niacin",
        VitaminB5 => "pantothenicAcid",
        VitaminB6 => "vitaminB6",
        VitaminB12 => "vitaminB12",
        Folate => "folate",
        Caffeine => "caffeine",
        _ => return None,
    })
}

/// `value` to three decimals. Amounts read back from the database can
/// differ from the ones logged in the last bit, which would otherwise
/// change a day's fingerprint and write it again.
fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

/// An entry's nutrients in Health Connect's units: kilocalories for
/// energy, grams for the rest.
fn nutrient_fields(entry: &LogEntry) -> serde_json::Map<String, Value> {
    entry
        .nutrients
        .iter()
        .filter_map(|(n, amount)| {
            let key = field(n)?;
            let amount = round3(amount);
            let value = match n.unit() {
                NutrientUnit::Kcal | NutrientUnit::Gram => amount,
                NutrientUnit::Milligram => amount / 1e3,
                NutrientUnit::Microgram => amount / 1e6,
            };
            (value.is_finite() && value >= 0.0).then(|| (key.to_string(), json!(value)))
        })
        .collect()
}

/// What OpenMacro writes for each day from `from` to `to` that has anything.
pub fn day_records(
    log: &[LogEntry],
    weigh_ins: &[WeighIn],
    measurements: &[Measurement],
    others: &Others,
    from: NaiveDate,
    to: NaiveDate,
) -> BTreeMap<NaiveDate, Records> {
    let within = |d: NaiveDate| (from..=to).contains(&d);
    let mut days: BTreeMap<NaiveDate, Records> = BTreeMap::new();

    let mut entries: Vec<&LogEntry> = log.iter().filter(|e| within(e.date)).collect();
    entries.sort_by_key(|e| (e.date, e.hour, e.id));
    for e in entries {
        let day = days.entry(e.date).or_default();
        let at = json!({ "date": e.date, "hour": e.hour, "minute": 0 });
        if e.food_id.as_deref() == Some(WATER_ID) {
            let ml = e.nutrients.get_or_zero(Nutrient::Water);
            if ml > 0.0 {
                day.water
                    .push(with(at, json!({ "liters": round3(ml) / 1000.0 })));
            }
        } else {
            day.food.push(with(
                at,
                json!({ "name": e.name, "nutrients": nutrient_fields(e) }),
            ));
        }
    }

    let mut weights: Vec<(NaiveDate, f64)> = weigh_ins
        .iter()
        .filter(|w| within(w.date))
        .filter(|w| {
            !others
                .weights
                .iter()
                .any(|r| r.date == w.date && same(r.value, w.kg))
        })
        .map(|w| (w.date, round3(w.kg)))
        .collect();
    weights.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    for (date, kg) in weights {
        let day = days.entry(date).or_default();
        let at = morning(date, day.weight.len());
        day.weight.push(with(at, json!({ "kg": kg })));
    }

    let mut fat: Vec<(NaiveDate, f64)> = measurements
        .iter()
        .filter(|m| m.kind == MeasurementKind::BodyFat && within(m.date))
        .filter(|m| {
            !others
                .body_fat
                .iter()
                .any(|r| r.date == m.date && same(r.value, m.value))
        })
        .map(|m| (m.date, round3(m.value)))
        .collect();
    fat.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    for (date, percent) in fat {
        let day = days.entry(date).or_default();
        let at = morning(date, day.body_fat.len());
        day.body_fat.push(with(at, json!({ "percent": percent })));
    }
    days
}

/// The `n`th reading of a day, a minute apart from 8 AM.
fn morning(date: NaiveDate, n: usize) -> Value {
    json!({ "date": date, "hour": MORNING_HOUR, "minute": n.min(59) })
}

/// `base` with `extra`'s fields added.
fn with(mut base: Value, extra: Value) -> Value {
    if let (Some(base), Value::Object(extra)) = (base.as_object_mut(), extra) {
        base.extend(extra);
    }
    base
}

/// FNV-1a, which unlike std's hasher stays the same across Rust versions.
fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The days from `from` to `to` to write again, each with its new
/// fingerprint (`None` once OpenMacro has nothing for the day). The
/// permissions count too, so granting another one writes everything again.
pub fn changes(
    days: &BTreeMap<NaiveDate, Records>,
    written: &BTreeMap<NaiveDate, u64>,
    granted: &[String],
    from: NaiveDate,
    to: NaiveDate,
) -> Vec<(NaiveDate, Option<u64>)> {
    let grants = granted.join(",");
    from.iter_days()
        .take_while(|d| *d <= to)
        .filter_map(|d| {
            let now = days
                .get(&d)
                .filter(|r| !r.is_empty())
                .map(|r| fingerprint(&format!("{grants}|{}", r.to_json())));
            (now != written.get(&d).copied()).then_some((d, now))
        })
        .collect()
}

/// Sorted days as runs of consecutive days, each written in one go.
pub fn runs(days: &[NaiveDate]) -> Vec<(NaiveDate, NaiveDate)> {
    let mut out: Vec<(NaiveDate, NaiveDate)> = Vec::new();
    for &d in days {
        match out.last_mut() {
            Some((_, end)) if *end + Duration::days(1) == d => *end = d,
            _ => out.push((d, d)),
        }
    }
    out
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// "added 1 weigh-in, sent 3 days", or `None` if nothing changed.
fn summary(weights: usize, body_fat: usize, days: usize) -> Option<String> {
    let mut parts = Vec::new();
    if weights > 0 {
        parts.push(format!("added {}", count(weights, "weigh-in", "weigh-ins")));
    }
    if body_fat > 0 {
        parts.push(format!(
            "added {}",
            count(body_fat, "body fat reading", "body fat readings")
        ));
    }
    if days > 0 {
        parts.push(format!("sent {}", count(days, "day", "days")));
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// Checks whether Health Connect is there and what OpenMacro may do.
pub async fn refresh() {
    let availability = bridge::availability().await;
    let granted = if availability == Availability::Ready {
        bridge::granted().await.unwrap_or_default()
    } else {
        Vec::new()
    };
    let mut status = STATUS.write();
    status.availability = availability;
    status.granted = granted;
}

/// Turns syncing on: asks for permissions, then syncs.
pub fn connect(state: AppState) {
    SETTINGS.write().enabled = true;
    ask_permissions(state);
}

/// Turns syncing off. What was written stays in Health Connect, but is
/// forgotten here, so turning syncing on again writes every day again.
pub fn turn_off() {
    let mut settings = SETTINGS.write();
    settings.enabled = false;
    settings.written.clear();
}

/// Shows Health Connect's permission screen, then syncs with whatever
/// the user allowed.
pub fn ask_permissions(state: AppState) {
    spawn_forever(async move {
        bridge::ask().await;
        refresh().await;
        sync(state);
    });
}

/// Opens Health Connect (or its Play Store page, if it needs installing).
pub fn open() {
    bridge::open();
}

/// Starts a sync if syncing is on and OpenMacro is on screen (Health
/// Connect only allows reading then; coming back on screen syncs). One
/// already running runs again after.
pub fn sync(state: AppState) {
    if !SETTINGS.peek().enabled || !bridge::on_screen() {
        return;
    }
    {
        let mut status = STATUS.write();
        if status.syncing {
            status.again = true;
            return;
        }
        status.syncing = true;
    }
    spawn_forever(async move {
        loop {
            let outcome = run(state).await;
            let at = chrono::Local::now().naive_local();
            let mut status = STATUS.write();
            if let Ok(Some(change)) = &outcome {
                status.last_change = Some((at, change.clone()));
            }
            status.last = Some(Synced {
                at,
                error: outcome.err(),
            });
            if !std::mem::take(&mut status.again) {
                status.syncing = false;
                break;
            }
        }
    });
}

/// One sync; returns what changed, if anything.
async fn run(state: AppState) -> Result<Option<String>, String> {
    refresh().await;
    let (availability, mut granted) = {
        let status = STATUS.peek();
        (status.availability, status.granted.clone())
    };
    if availability != Availability::Ready {
        return Err("Health Connect isn't available.".into());
    }
    if granted.is_empty() {
        return Err("OpenMacro isn't allowed to use Health Connect yet.".into());
    }
    granted.sort();
    let has = |p: &str| granted.iter().any(|g| g == p);
    let to = today();
    let from = to - Duration::days(WINDOW_DAYS - 1);

    let mut others = Others::default();
    if has("READ_WEIGHT") {
        others.weights = bridge::read("readWeights", from, to).await?;
    }
    if has("READ_BODY_FAT") {
        others.body_fat = bridge::read("readBodyFat", from, to).await?;
    }
    let (weights, body_fat) = import(state, &others);

    let days = day_records(
        &state.log.peek(),
        &state.weigh_ins.peek(),
        &state.measurements.peek(),
        &others,
        from,
        to,
    );
    let mut written = SETTINGS.peek().written.clone();
    written.retain(|d, _| *d >= from);
    let changed = changes(&days, &written, &granted, from, to);
    let changed_days: Vec<NaiveDate> = changed.iter().map(|(d, _)| *d).collect();
    for (start, end) in runs(&changed_days) {
        let mut records = Records::default();
        for r in days.range(start..=end).map(|(_, r)| r) {
            records.extend(r);
        }
        bridge::write(start, end, &records.to_json()).await?;
        for (d, now) in changed.iter().filter(|(d, _)| (start..=end).contains(d)) {
            match now {
                Some(f) => written.insert(*d, *f),
                None => written.remove(d),
            };
        }
        // Keep each run's progress in case a later one fails.
        SETTINGS.write().written = written.clone();
    }
    SETTINGS.write().written = written;
    Ok(summary(weights, body_fat, changed_days.len()))
}

/// Adds other apps' readings not seen before; returns how many of each.
fn import(state: AppState, others: &Others) -> (usize, usize) {
    let persist = state.persist;
    let weigh_ins = state.weigh_ins.peek().clone();
    let weights: Vec<WeighIn> = new_readings(&others.weights, |r| {
        let w = WeighIn {
            date: r.date,
            kg: r.value,
        };
        weigh_ins
            .iter()
            .any(|x| x.date == w.date && same(x.kg, w.kg))
            || persist.peek().weigh_in_seen(&w)
    })
    .into_iter()
    .map(|r| WeighIn {
        date: r.date,
        kg: r.value,
    })
    .collect();
    let measurements = state.measurements.peek().clone();
    let body_fat: Vec<Measurement> = new_readings(&others.body_fat, |r| {
        let m = Measurement {
            date: r.date,
            kind: MeasurementKind::BodyFat,
            value: r.value,
        };
        measurements
            .iter()
            .any(|x| x.kind == m.kind && x.date == m.date && same(x.value, m.value))
            || persist.peek().measurement_seen(&m)
    })
    .into_iter()
    .map(|r| Measurement {
        date: r.date,
        kind: MeasurementKind::BodyFat,
        value: r.value,
    })
    .collect();
    let counts = (weights.len(), body_fat.len());
    state.import_readings(weights, body_fat);
    counts
}

/// Loads the settings, then syncs at launch, a moment after the log,
/// weigh-ins or measurements change, and when the app comes back on screen.
pub fn use_health_sync(state: AppState) {
    use_hook(move || {
        if let Some(saved) = state.persist.peek().load_health() {
            *SETTINGS.write() = saved;
        }
    });
    use_effect(move || {
        let settings = SETTINGS.read().clone();
        state.persist.peek().save_health(&settings);
    });
    #[cfg(target_os = "android")]
    {
        use std::sync::atomic::{AtomicU64, Ordering};

        static CHANGES: AtomicU64 = AtomicU64::new(0);
        use_effect(move || {
            let _ = (
                state.log.read().len(),
                state.weigh_ins.read().len(),
                state.measurements.read().len(),
            );
            let change = CHANGES.fetch_add(1, Ordering::Relaxed) + 1;
            spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                if CHANGES.load(Ordering::Relaxed) == change {
                    sync(state);
                }
            });
        });
        use_hook(move || {
            spawn(async move {
                // MainActivity sends this when the app resumes.
                let mut shown = document::eval(
                    "window.addEventListener('openmacro-resume', () => dioxus.send(true));
                    await new Promise(() => {});",
                );
                while shown.recv::<bool>().await.is_ok() {
                    sync(state);
                }
            });
        });
    }
}

/// Health Connect's client library, through `HealthBridge` in
/// android/MainActivity.kt.
#[cfg(target_os = "android")]
mod bridge {
    use std::time::Duration;

    use chrono::NaiveDate;
    use jni::JNIEnv;
    use jni::objects::{JClass, JObject, JString, JValue};
    use serde_json::Value;

    use super::{Availability, Reading};
    use crate::android::{app_class, off_thread, with_activity};

    /// The bridge class.
    fn class<'a>(env: &mut JNIEnv<'a>, activity: &JObject) -> jni::errors::Result<JClass<'a>> {
        app_class(env, activity, "dev.dioxus.main.HealthBridge")
    }

    /// Calls a bridge method taking the context and `args`, and unwraps
    /// its {"ok": value} or {"error": message} reply.
    fn call(method: &str, args: &[String]) -> Result<Value, String> {
        let text = with_activity(|env, activity| {
            let class = class(env, activity)?;
            let strings = args
                .iter()
                .map(|a| env.new_string(a))
                .collect::<Result<Vec<_>, _>>()?;
            let mut values = vec![JValue::Object(activity)];
            values.extend(strings.iter().map(|s| JValue::Object(s)));
            let signature = format!(
                "(Landroid/content/Context;{})Ljava/lang/String;",
                "Ljava/lang/String;".repeat(args.len())
            );
            let reply = env
                .call_static_method(&class, method, signature, &values)?
                .l()?;
            let text: String = env.get_string(&JString::from(reply))?.into();
            Ok(text)
        })
        .ok_or("Health Connect didn't respond.")?;
        let mut reply: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        if let Some(error) = reply.get("error") {
            return Err(error
                .as_str()
                .unwrap_or("Health Connect: something went wrong.")
                .to_string());
        }
        Ok(reply["ok"].take())
    }

    async fn call_off_thread(method: &'static str, args: Vec<String>) -> Result<Value, String> {
        off_thread(move || call(method, &args))
            .await
            .unwrap_or_else(|| Err("Health Connect stopped responding.".into()))
    }

    /// Calls a bridge method that takes the activity and returns nothing.
    fn with_bridge(method: &str) {
        with_activity(|env, activity| {
            let class = class(env, activity)?;
            env.call_static_method(
                &class,
                method,
                "(Landroid/app/Activity;)V",
                &[JValue::Object(activity)],
            )?;
            Ok(())
        });
    }

    pub async fn availability() -> Availability {
        let status = off_thread(|| {
            with_activity(|env, activity| {
                let class = class(env, activity)?;
                env.call_static_method(
                    &class,
                    "status",
                    "(Landroid/content/Context;)I",
                    &[JValue::Object(activity)],
                )?
                .i()
            })
        })
        .await
        .flatten();
        match status {
            Some(2) => Availability::Ready,
            Some(1) => Availability::NeedsInstall,
            _ => Availability::Unsupported,
        }
    }

    pub async fn granted() -> Result<Vec<String>, String> {
        let list = call_off_thread("granted", Vec::new()).await?;
        Ok(list
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|p| p.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Readings from `readWeights` or `readBodyFat`.
    pub async fn read(
        method: &'static str,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<Reading>, String> {
        let list = call_off_thread(method, vec![from.to_string(), to.to_string()]).await?;
        Ok(list
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| {
                Some(Reading {
                    date: r["date"].as_str()?.parse().ok()?,
                    value: r["value"].as_f64()?,
                })
            })
            .collect())
    }

    /// Replaces what OpenMacro saved from `from` to `to` with `records`.
    pub async fn write(from: NaiveDate, to: NaiveDate, records: &Value) -> Result<(), String> {
        call_off_thread(
            "write",
            vec![from.to_string(), to.to_string(), records.to_string()],
        )
        .await
        .map(|_| ())
    }

    /// Shows the permission screen and waits until it closes.
    pub async fn ask() {
        with_bridge("ask");
        let asking = || {
            with_activity(|env, activity| {
                let class = class(env, activity)?;
                env.call_static_method(&class, "asking", "()Z", &[])?.z()
            })
            .unwrap_or(false)
        };
        // Give up after ten minutes, in case the screen never reports back.
        for _ in 0..2000 {
            tokio::time::sleep(Duration::from_millis(300)).await;
            if !asking() {
                break;
            }
        }
    }

    pub fn open() {
        with_bridge("open");
    }

    pub fn on_screen() -> bool {
        with_activity(|env, activity| {
            let class = class(env, activity)?;
            env.call_static_method(&class, "getOnScreen", "()Z", &[])?
                .z()
        })
        .unwrap_or(false)
    }
}

/// Health Connect is Android-only.
#[cfg(not(target_os = "android"))]
mod bridge {
    use chrono::NaiveDate;
    use serde_json::Value;

    use super::{Availability, Reading};

    const UNSUPPORTED: &str = "Health Connect is only on Android.";

    pub async fn availability() -> Availability {
        Availability::Unsupported
    }

    pub async fn granted() -> Result<Vec<String>, String> {
        Err(UNSUPPORTED.into())
    }

    pub async fn read(_: &'static str, _: NaiveDate, _: NaiveDate) -> Result<Vec<Reading>, String> {
        Err(UNSUPPORTED.into())
    }

    pub async fn write(_: NaiveDate, _: NaiveDate, _: &Value) -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }

    pub async fn ask() {}

    pub fn open() {}

    pub fn on_screen() -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use openmacro_core::nutrients::NutrientProfile;

    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    fn entry(id: i64, day: u32, hour: u32, kcal: f64) -> LogEntry {
        LogEntry {
            id,
            date: d(day),
            hour,
            name: format!("Food {id}"),
            amount: "1 serving".into(),
            nutrients: NutrientProfile::from_macros(kcal, 10.0, 5.0, 20.0)
                .with(Nutrient::Sodium, 400.0)
                .with(Nutrient::Leucine, 1.0),
            food_id: None,
        }
    }

    #[test]
    fn readings_seen_before_are_skipped_and_others_added_once() {
        let readings = [
            Reading {
                date: d(1),
                value: 80.0,
            },
            Reading {
                date: d(1),
                value: 80.001,
            },
            Reading {
                date: d(2),
                value: 79.5,
            },
        ];
        let added = new_readings(&readings, |r| r.date == d(2));
        assert_eq!(added, vec![readings[0]]);
    }

    #[test]
    fn days_hold_food_water_and_own_weigh_ins() {
        let mut water = entry(3, 10, 9, 0.0);
        water.food_id = Some(WATER_ID.into());
        water.nutrients = NutrientProfile::new().with(Nutrient::Water, 500.0);
        let log = [
            entry(2, 10, 12, 300.0),
            entry(1, 10, 8, 200.0),
            water,
            entry(4, 1, 8, 100.0),
        ];
        let weigh_ins = [
            WeighIn {
                date: d(10),
                kg: 80.0,
            },
            WeighIn {
                date: d(10),
                kg: 81.2,
            },
        ];
        let measurements = [Measurement {
            date: d(10),
            kind: MeasurementKind::BodyFat,
            value: 20.0,
        }];
        // 81.2 kg came from a scale app, so it isn't written back.
        let others = Others {
            weights: vec![Reading {
                date: d(10),
                value: 81.2,
            }],
            body_fat: vec![],
        };
        let days = day_records(&log, &weigh_ins, &measurements, &others, d(5), d(11));
        assert_eq!(days.len(), 1);
        let day = &days[&d(10)];
        assert_eq!(day.food.len(), 2);
        assert_eq!(day.food[0]["hour"], 8);
        assert_eq!(day.food[0]["nutrients"]["energy"], 200.0);
        assert_eq!(day.food[0]["nutrients"]["sodium"], 0.4);
        assert!(day.food[0]["nutrients"].get("leucine").is_none());
        assert_eq!(day.water[0]["liters"], 0.5);
        assert_eq!(day.weight.len(), 1);
        assert_eq!(day.weight[0]["kg"], 80.0);
        assert_eq!(day.weight[0]["hour"], MORNING_HOUR);
        assert_eq!(day.body_fat[0]["percent"], 20.0);
    }

    #[test]
    fn only_changed_days_are_written_again() {
        let granted = vec!["WRITE_NUTRITION".to_string()];
        let log = vec![entry(1, 10, 8, 200.0), entry(2, 11, 8, 300.0)];
        let records =
            |log: &[LogEntry]| day_records(log, &[], &[], &Others::default(), d(1), d(12));
        let days = records(&log);
        let first = changes(&days, &BTreeMap::new(), &granted, d(1), d(12));
        assert_eq!(
            first.iter().map(|c| c.0).collect::<Vec<_>>(),
            vec![d(10), d(11)]
        );
        let written: BTreeMap<_, _> = first.iter().map(|(d, f)| (*d, f.unwrap())).collect();
        assert!(changes(&days, &written, &granted, d(1), d(12)).is_empty());

        // Amounts off in the last bit, as read back from the database, aren't a change.
        let mut reread = log.clone();
        for e in &mut reread {
            let kcal = e.nutrients.get_or_zero(Nutrient::Energy);
            e.nutrients
                .set(Nutrient::Energy, f64::from_bits(kcal.to_bits() + 1));
        }
        assert!(changes(&records(&reread), &written, &granted, d(1), d(12)).is_empty());

        // Day 11's entry deleted: day 11 is written as empty.
        let days = records(&log[..1]);
        assert_eq!(
            changes(&days, &written, &granted, d(1), d(12)),
            vec![(d(11), None)]
        );

        // A new permission writes every day again.
        let more = vec!["WRITE_NUTRITION".to_string(), "WRITE_WEIGHT".to_string()];
        assert_eq!(
            changes(&records(&log), &written, &more, d(1), d(12)).len(),
            2
        );
    }

    #[test]
    fn consecutive_days_form_runs() {
        assert_eq!(
            runs(&[d(1), d(2), d(3), d(5), d(7), d(8)]),
            vec![(d(1), d(3)), (d(5), d(5)), (d(7), d(8))]
        );
        assert!(runs(&[]).is_empty());
    }

    #[test]
    fn summaries() {
        assert_eq!(summary(0, 0, 0), None);
        assert_eq!(summary(1, 0, 3).unwrap(), "added 1 weigh-in, sent 3 days");
        assert_eq!(
            summary(0, 2, 1).unwrap(),
            "added 2 body fat readings, sent 1 day"
        );
    }
}
