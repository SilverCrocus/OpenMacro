//! App state: the user's data as plain values, a reactive wrapper around it,
//! and the coaching numbers derived from it.

use std::collections::BTreeMap;

use chrono::{Datelike, Duration, Local, NaiveDate, Timelike, Weekday};
use dioxus::prelude::*;
use openmacro_core::bmr::{self, ActivityLevel, BmrEquation, BodyStats, Sex};
use openmacro_core::body::Measurement;
use openmacro_core::checkin::CheckIn;
use openmacro_core::expenditure::{
    self, DayRecord, DayStatus, EstimateStatus, ExpenditureConfig, ExpenditurePoint,
};
use openmacro_core::food::Food;
use openmacro_core::nutrients::{Nutrient, NutrientGoal, NutrientProfile};
use openmacro_core::recipe::Recipe;
use openmacro_core::targets::{self, CalorieTarget, CoachedProgram, MacroTargets};
use openmacro_core::trend::{self, TrendPoint, TrendSmoothing, WeighIn};

pub use openmacro_core::log::{Goal, LogEntry, Profile, day_totals};

use crate::food_search::FoodRegion;
use crate::persist::Persistence;

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

pub fn current_hour() -> u32 {
    Local::now().hour()
}

/// 0 = Monday.
pub fn weekday_index(date: NaiveDate) -> usize {
    date.weekday().num_days_from_monday() as usize
}

pub fn week_start(date: NaiveDate) -> NaiveDate {
    date - Duration::days(weekday_index(date) as i64)
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlateItem {
    pub name: String,
    pub amount: String,
    pub nutrients: NutrientProfile,
    /// Stable id of the food, so the log can offer it again later.
    pub food_id: Option<String>,
    /// Weight, when known (not for quick adds); used to weigh recipes.
    pub grams: Option<f64>,
}

/// Foods staged for logging together at one date and hour.
#[derive(Debug, Clone, PartialEq)]
pub struct Plate {
    pub open: bool,
    pub date: NaiveDate,
    pub hour: u32,
    pub items: Vec<PlateItem>,
}

impl Plate {
    pub fn closed(date: NaiveDate) -> Self {
        Self {
            open: false,
            date,
            hour: 8,
            items: Vec::new(),
        }
    }

    pub fn totals(&self) -> NutrientProfile {
        self.items.iter().map(|i| &i.nutrients).sum()
    }
}

/// Starting profile for a new account until the user edits it under More.
pub fn default_profile() -> Profile {
    Profile {
        stats: BodyStats {
            sex: Sex::Male,
            age_years: 30.0,
            height_cm: 175.0,
            weight_kg: 75.0,
            body_fat_fraction: None,
        },
        activity: ActivityLevel::Moderate,
        equation: BmrEquation::MifflinStJeor,
    }
}

/// Everything the user owns, as plain values.
#[derive(Debug, Clone, PartialEq)]
pub struct Data {
    pub log: Vec<LogEntry>,
    pub weigh_ins: Vec<WeighIn>,
    pub day_status: BTreeMap<NaiveDate, DayStatus>,
    pub profile: Profile,
    pub goal: Goal,
    pub program: CoachedProgram,
    /// Completed check-ins, oldest first.
    pub check_ins: Vec<CheckIn>,
    /// Targets the user set themselves (the manual program), used every
    /// day instead of the coached ones.
    pub manual_targets: Option<MacroTargets>,
    pub coach_settings: CoachSettings,
}

/// How the estimates behave.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(
    not(target_arch = "wasm32"),
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct CoachSettings {
    /// How quickly the weight trend follows the scale.
    pub smoothing: TrendSmoothing,
    /// Plan with this expenditure instead of the adaptive estimate.
    pub fixed_expenditure_kcal: Option<f64>,
    /// Daily water goal in millilitres; `None` means the default.
    pub water_goal_ml: Option<f64>,
    /// The user's own nutrient goals, by nutrient key, replacing the
    /// standard ones.
    pub nutrient_goals: BTreeMap<String, NutrientGoal>,
}

impl Data {
    /// A fresh account: no history, default goal and program.
    pub fn empty(profile: Profile) -> Self {
        Self {
            log: Vec::new(),
            weigh_ins: Vec::new(),
            day_status: BTreeMap::new(),
            goal: Goal {
                goal_weight_kg: profile.stats.weight_kg,
                rate_pct_per_week: 0.0,
            },
            profile,
            program: CoachedProgram {
                diet: Default::default(),
                protein: Default::default(),
                floor: Default::default(),
                distribution: Default::default(),
            },
            check_ins: Vec::new(),
            manual_targets: None,
            coach_settings: CoachSettings::default(),
        }
    }
}

/// Expenditure, trend and targets derived from [`Data`].
///
/// A check-in locks the expenditure estimate and trend weight that targets
/// are based on, so targets don't drift day to day. Goal and program changes
/// still apply straight away, using those locked values.
#[derive(Debug, Clone, PartialEq)]
pub struct CoachSnapshot {
    pub prior_kcal: f64,
    pub expenditure: Vec<ExpenditurePoint>,
    pub trend: Vec<TrendPoint>,
    /// Active targets: from the last check-in's basis, or live before the first.
    pub average_target: CalorieTarget,
    /// Monday through Sunday.
    pub week: [MacroTargets; 7],
    /// What a check-in today would set.
    pub live_target: CalorieTarget,
    pub live_week: [MacroTargets; 7],
    pub last_check_in: Option<CheckIn>,
}

impl CoachSnapshot {
    pub fn compute(data: &Data, today: NaiveDate) -> Self {
        let profile = &data.profile;
        let prior_kcal =
            bmr::initial_expenditure(&profile.stats, profile.equation, profile.activity);
        let expenditure = expenditure::estimate(
            &day_records(data, today),
            prior_kcal,
            &ExpenditureConfig::default(),
        );
        let settings = &data.coach_settings;
        let trend = trend::ema_trend(&data.weigh_ins, settings.smoothing.alpha(), today);

        // A fixed expenditure replaces the estimate (and any check-in's) for targets.
        let fixed = settings.fixed_expenditure_kcal;
        let live_kcal = fixed.unwrap_or_else(|| expenditure.last().map_or(prior_kcal, |e| e.kcal));
        let live_weight = trend.last().map_or(profile.stats.weight_kg, |t| t.trend_kg);
        let last_check_in = data
            .check_ins
            .iter()
            .filter(|c| c.date <= today)
            .max_by_key(|c| c.date)
            .cloned();
        let (kcal, weight) = match &last_check_in {
            Some(c) => (
                fixed.unwrap_or(c.expenditure_kcal),
                c.trend_kg.unwrap_or(live_weight),
            ),
            None => (live_kcal, live_weight),
        };
        // Maintaining corrects towards the goal weight from outside its band.
        let rate = data.goal.effective_rate(weight);
        let live_rate = data.goal.effective_rate(live_weight);
        let floor = data.program.floor;
        let (average_target, week, live_target, live_week) = match data.manual_targets {
            // A manual program sets the same targets every day; check-ins
            // don't change them.
            Some(manual) => {
                let target = CalorieTarget {
                    kcal: manual.kcal,
                    limited_by_floor: false,
                };
                (target, [manual; 7], target, [manual; 7])
            }
            None => (
                targets::average_daily_target(kcal, weight, rate, floor),
                targets::weekly_plan(&data.program, kcal, weight, rate),
                targets::average_daily_target(live_kcal, live_weight, live_rate, floor),
                targets::weekly_plan(&data.program, live_kcal, live_weight, live_rate),
            ),
        };
        Self {
            prior_kcal,
            average_target,
            week,
            live_target,
            live_week,
            last_check_in,
            expenditure,
            trend,
        }
    }

    pub fn expenditure_kcal(&self) -> f64 {
        self.expenditure.last().map_or(self.prior_kcal, |e| e.kcal)
    }

    pub fn expenditure_status(&self) -> EstimateStatus {
        self.expenditure
            .last()
            .map_or(EstimateStatus::Initial, |e| e.status)
    }

    pub fn trend_kg(&self) -> Option<f64> {
        self.trend.last().map(|t| t.trend_kg)
    }

    pub fn target_for(&self, date: NaiveDate) -> MacroTargets {
        self.week[weekday_index(date)]
    }
}

/// The targets in force on `date`: from the check-in active then, or the
/// current targets from the latest check-in onwards. None before the first.
pub fn target_on(
    date: NaiveDate,
    check_ins: &[CheckIn],
    coach: &CoachSnapshot,
) -> Option<MacroTargets> {
    let active = check_ins
        .iter()
        .filter(|c| c.date <= date)
        .max_by_key(|c| c.date)?;
    if coach.last_check_in.as_ref() == Some(active) {
        Some(coach.target_for(date))
    } else {
        active.week.get(weekday_index(date)).copied()
    }
}

/// One record per day through today. Today's intake is left out because the
/// day is still in progress, but today's weigh-in counts.
fn day_records(data: &Data, today: NaiveDate) -> Vec<DayRecord> {
    let mut intake: BTreeMap<NaiveDate, f64> = BTreeMap::new();
    for e in &data.log {
        *intake.entry(e.date).or_default() += e.nutrients.get_or_zero(Nutrient::Energy);
    }
    let weights = trend::daily_weights(&data.weigh_ins);
    let Some(first) = intake.keys().chain(weights.keys()).min().copied() else {
        return Vec::new();
    };
    first
        .iter_days()
        .take_while(|d| *d <= today)
        .map(|date| DayRecord {
            date,
            // Days with no energy logged (nothing, or only water) count as
            // unlogged; a real fast is marked with the Fasted status.
            intake_kcal: if date == today {
                None
            } else {
                intake.get(&date).copied().filter(|kcal| *kcal > 0.0)
            },
            status: data.day_status.get(&date).copied().unwrap_or_default(),
            weight_kg: weights.get(&date).copied(),
        })
        .collect()
}

/// The food id of water logged from the water tile.
pub const WATER_ID: &str = "water";
/// The daily water goal when the user hasn't set one.
pub const DEFAULT_WATER_GOAL_ML: f64 = 2000.0;

/// Entries picked for copying to another day.
#[derive(Debug, Clone, PartialEq)]
pub struct Copying {
    pub entries: Vec<LogEntry>,
    /// True when copying a whole day, which keeps each entry's time.
    pub whole_day: bool,
}

/// Everything saved on the device, read in one go.
struct Saved {
    data: Data,
    region: FoodRegion,
    custom_foods: Vec<Food>,
    favorites: Vec<Food>,
    check_in_day: Weekday,
    recipes: BTreeMap<String, Recipe>,
    measurements: Vec<Measurement>,
}

impl Saved {
    fn read(persist: &Persistence) -> Self {
        Self {
            data: persist
                .load()
                .unwrap_or_else(|| Data::empty(default_profile())),
            region: persist
                .load_region()
                .and_then(|code| FoodRegion::from_code(&code))
                .unwrap_or_default(),
            custom_foods: persist.custom_foods(),
            favorites: persist.favorites(),
            check_in_day: persist
                .load_check_in_day()
                .and_then(|day| day.parse().ok())
                .unwrap_or(Weekday::Mon),
            recipes: persist.recipes().into_iter().collect(),
            measurements: persist.measurements(),
        }
    }
}

/// Reactive handles to the user's data, shared through context. Changes to
/// the log, weigh-ins and day status are written through to the device
/// immediately; profile, goal and program are saved by an effect in `App`.
#[derive(Clone, Copy)]
pub struct AppState {
    pub log: Signal<Vec<LogEntry>>,
    pub weigh_ins: Signal<Vec<WeighIn>>,
    pub day_status: Signal<BTreeMap<NaiveDate, DayStatus>>,
    pub profile: Signal<Profile>,
    pub goal: Signal<Goal>,
    pub program: Signal<CoachedProgram>,
    pub plate: Signal<Plate>,
    pub selected_date: Signal<NaiveDate>,
    pub persist: Signal<Persistence>,
    /// Which country's products and spellings search prefers.
    pub region: Signal<FoodRegion>,
    /// Foods the user created, newest first.
    pub custom_foods: Signal<Vec<Food>>,
    /// Starred foods, most recent first.
    pub favorites: Signal<Vec<Food>>,
    pub check_ins: Signal<Vec<CheckIn>>,
    /// Set when the user runs a manual program.
    pub manual_targets: Signal<Option<MacroTargets>>,
    pub coach_settings: Signal<CoachSettings>,
    /// Weekday the weekly check-in falls on.
    pub check_in_day: Signal<Weekday>,
    /// Whether the check-in flow is showing.
    pub check_in_open: Signal<bool>,
    /// The logged entry being edited, if any.
    pub editing: Signal<Option<LogEntry>>,
    /// Body measurements, oldest first.
    pub measurements: Signal<Vec<Measurement>>,
    /// Entries chosen to copy to another day, if the copy sheet is open.
    pub copying: Signal<Option<Copying>>,
    /// Recipes' ingredients, by food id.
    pub recipes: Signal<BTreeMap<String, Recipe>>,
    /// The recipe whose ingredients are on the plate for editing, if any.
    pub recipe_edit: Signal<Option<String>>,
    /// Ids for entries when nothing is saved (web preview).
    next_id: Signal<i64>,
}

fn next_id_after(log: &[LogEntry]) -> i64 {
    log.iter().map(|e| e.id).max().unwrap_or(0) + 1
}

impl AppState {
    /// Opens the device database and loads saved data; a first launch starts empty.
    pub fn load() -> Self {
        let persist = Persistence::open();
        let Saved {
            data,
            region,
            custom_foods,
            favorites,
            check_in_day,
            recipes,
            measurements,
        } = Saved::read(&persist);
        Self {
            region: Signal::new(region),
            custom_foods: Signal::new(custom_foods),
            favorites: Signal::new(favorites),
            check_ins: Signal::new(data.check_ins.clone()),
            manual_targets: Signal::new(data.manual_targets),
            coach_settings: Signal::new(data.coach_settings),
            check_in_day: Signal::new(check_in_day),
            check_in_open: Signal::new(false),
            editing: Signal::new(None),
            copying: Signal::new(None),
            measurements: Signal::new(measurements),
            recipes: Signal::new(recipes),
            recipe_edit: Signal::new(None),
            next_id: Signal::new(next_id_after(&data.log)),
            log: Signal::new(data.log),
            weigh_ins: Signal::new(data.weigh_ins),
            day_status: Signal::new(data.day_status),
            profile: Signal::new(data.profile),
            goal: Signal::new(data.goal),
            program: Signal::new(data.program),
            plate: Signal::new(Plate::closed(today())),
            selected_date: Signal::new(today()),
            persist: Signal::new(persist),
        }
    }

    /// Re-reads everything from the device, e.g. after restoring a backup.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn reload(mut self) {
        let saved = Saved::read(&self.persist.peek());
        self.next_id.set(next_id_after(&saved.data.log));
        self.log.set(saved.data.log);
        self.weigh_ins.set(saved.data.weigh_ins);
        self.day_status.set(saved.data.day_status);
        self.profile.set(saved.data.profile);
        self.goal.set(saved.data.goal);
        self.program.set(saved.data.program);
        self.check_ins.set(saved.data.check_ins);
        self.manual_targets.set(saved.data.manual_targets);
        self.coach_settings.set(saved.data.coach_settings);
        self.region.set(saved.region);
        self.custom_foods.set(saved.custom_foods);
        self.favorites.set(saved.favorites);
        self.check_in_day.set(saved.check_in_day);
        self.recipes.set(saved.recipes);
        self.measurements.set(saved.measurements);
        if let Some(prefs) = self.persist.peek().load_prefs() {
            *crate::prefs::PREFS.write() = prefs;
        }
        self.recipe_edit.set(None);
        self.check_in_open.set(false);
        self.editing.set(None);
        self.copying.set(None);
        self.plate.set(Plate::closed(today()));
        self.selected_date.set(today());
    }

    pub fn data(&self) -> Data {
        Data {
            log: self.log.read().clone(),
            weigh_ins: self.weigh_ins.read().clone(),
            day_status: self.day_status.read().clone(),
            profile: *self.profile.read(),
            goal: *self.goal.read(),
            program: self.program.read().clone(),
            check_ins: self.check_ins.read().clone(),
            manual_targets: *self.manual_targets.read(),
            coach_settings: self.coach_settings.read().clone(),
        }
    }

    /// True when the account has no history at all.
    pub fn is_empty(&self) -> bool {
        self.log.read().is_empty() && self.weigh_ins.read().is_empty()
    }

    /// Replaces everything, e.g. to load demo data or clear the account.
    pub fn replace_data(mut self, mut data: Data) {
        if let Some(saved) = self.persist.write().replace_history(&data) {
            data.log = saved;
        }
        self.next_id.set(next_id_after(&data.log));
        self.log.set(data.log);
        self.weigh_ins.set(data.weigh_ins);
        self.day_status.set(data.day_status);
        self.profile.set(data.profile);
        self.goal.set(data.goal);
        self.program.set(data.program);
        self.check_ins.set(data.check_ins);
        self.plate.set(Plate::closed(today()));
        self.selected_date.set(today());
    }

    pub fn open_plate(mut self, date: NaiveDate, hour: u32) {
        let mut plate = self.plate.write();
        plate.open = true;
        plate.date = date;
        plate.hour = hour;
    }

    /// Hides the logging sheet but keeps what's on the plate for later.
    pub fn hide_plate(mut self) {
        self.plate.write().open = false;
    }

    pub fn close_plate(mut self) {
        let mut plate = self.plate.write();
        plate.open = false;
        plate.items.clear();
        self.recipe_edit.set(None);
    }

    pub fn add_to_plate(mut self, item: PlateItem) {
        self.plate.write().items.push(item);
    }

    pub fn replace_plate_item(mut self, index: usize, item: PlateItem) {
        if let Some(slot) = self.plate.write().items.get_mut(index) {
            *slot = item;
        }
    }

    pub fn remove_from_plate(mut self, index: usize) {
        let mut plate = self.plate.write();
        if index < plate.items.len() {
            plate.items.remove(index);
        }
    }

    /// Moves everything on the plate into the log at the plate's date and hour.
    pub fn log_plate(mut self) {
        let (date, hour, items) = {
            let mut plate = self.plate.write();
            plate.open = false;
            (plate.date, plate.hour, std::mem::take(&mut plate.items))
        };
        let entries = items
            .into_iter()
            .map(|item| LogEntry {
                id: 0,
                date,
                hour,
                name: item.name,
                amount: item.amount,
                nutrients: item.nutrients,
                food_id: item.food_id,
            })
            .collect();
        self.insert_entries(entries);
        self.selected_date.set(date);
    }

    /// Logs copies of `entries` on `date`, at `hour` or at their own times,
    /// and shows that day.
    pub fn copy_entries(mut self, entries: &[LogEntry], date: NaiveDate, hour: Option<u32>) {
        let copies = entries
            .iter()
            .map(|e| LogEntry {
                id: 0,
                date,
                hour: hour.unwrap_or(e.hour),
                ..e.clone()
            })
            .collect();
        self.insert_entries(copies);
        self.copying.set(None);
        self.selected_date.set(date);
    }

    /// Saves new entries, giving each its id, and adds them to the log.
    fn insert_entries(mut self, mut entries: Vec<LogEntry>) {
        for entry in &mut entries {
            entry.id = match self.persist.read().insert_entry(entry) {
                Some(id) => id,
                None => {
                    let id = *self.next_id.peek();
                    self.next_id.set(id + 1);
                    id
                }
            };
        }
        self.log.write().extend(entries);
    }

    /// Saves a food the user created and returns it with its id.
    pub fn add_custom_food(mut self, mut food: Food) -> Food {
        food.id = self
            .persist
            .read()
            .insert_custom_food(&food)
            .unwrap_or_else(|| {
                let n = *self.next_id.peek();
                self.next_id.set(n + 1);
                format!("custom:{n}")
            });
        self.custom_foods.write().insert(0, food.clone());
        food
    }

    /// Logs a drink of water now, as a zero-energy entry with the Water nutrient.
    pub fn log_water(self, date: NaiveDate, ml: f64) {
        let entry = LogEntry {
            id: 0,
            date,
            hour: current_hour(),
            name: "Water".into(),
            amount: format!("{} ml", ml.round()),
            nutrients: NutrientProfile::new()
                .with(Nutrient::Energy, 0.0)
                .with(Nutrient::Water, ml),
            food_id: Some(WATER_ID.into()),
        };
        self.insert_entries(vec![entry]);
    }

    /// Saves a recipe; it becomes a food in the Library and search.
    pub fn add_recipe(mut self, recipe: &Recipe) -> Food {
        let id = self
            .persist
            .read()
            .insert_recipe(recipe)
            .unwrap_or_else(|| {
                let n = *self.next_id.peek();
                self.next_id.set(n + 1);
                format!("custom:{n}")
            });
        let food = recipe.to_food(id.clone());
        self.recipes.write().insert(id, recipe.clone());
        self.custom_foods.write().insert(0, food.clone());
        food
    }

    /// Saves changes to a custom food (its id says which).
    pub fn update_custom_food(self, food: Food) {
        self.persist.read().update_custom_food(&food);
        self.replace_food(food);
    }

    /// Saves changes to a recipe and returns the food made from it.
    pub fn update_recipe(mut self, id: &str, recipe: &Recipe) -> Food {
        self.persist.read().update_recipe(id, recipe);
        self.recipes.write().insert(id.to_string(), recipe.clone());
        let food = recipe.to_food(id);
        self.replace_food(food.clone());
        food
    }

    /// Swaps an edited food into the Library and favorites.
    fn replace_food(mut self, food: Food) {
        if let Some(slot) = self
            .custom_foods
            .write()
            .iter_mut()
            .find(|f| f.id == food.id)
        {
            *slot = food.clone();
        }
        if self.is_favorite(&food.id) {
            self.persist.read().set_favorite(&food, true);
            if let Some(slot) = self.favorites.write().iter_mut().find(|f| f.id == food.id) {
                *slot = food;
            }
        }
    }

    /// Puts a recipe's ingredients on the plate to change them.
    pub fn edit_recipe(mut self, id: &str) {
        let Some(recipe) = self.recipes.read().get(id).cloned() else {
            return;
        };
        let items = recipe
            .ingredients
            .into_iter()
            .map(|i| PlateItem {
                name: i.name,
                amount: i.amount,
                nutrients: i.nutrients,
                food_id: i.food_id,
                grams: i.grams,
            })
            .collect();
        self.set_plate_items(items);
        self.recipe_edit.set(Some(id.to_string()));
    }

    /// Replaces everything on the plate with one item.
    pub fn set_plate_items(mut self, items: Vec<PlateItem>) {
        self.plate.write().items = items;
    }

    /// Records a completed check-in; its targets become the active ones.
    pub fn add_check_in(mut self, check_in: CheckIn) {
        self.persist.read().insert_check_in(&check_in);
        self.check_ins.write().push(check_in);
        self.check_in_open.set(false);
    }

    pub fn is_favorite(&self, id: &str) -> bool {
        self.favorites.read().iter().any(|f| f.id == id)
    }

    /// Stars or unstars a food.
    pub fn toggle_favorite(mut self, food: &Food) {
        let starred = !self.is_favorite(&food.id);
        self.persist.read().set_favorite(food, starred);
        let mut favorites = self.favorites.write();
        if starred {
            favorites.insert(0, food.clone());
        } else {
            favorites.retain(|f| f.id != food.id);
        }
    }

    pub fn delete_custom_food(mut self, id: &str) {
        self.persist.read().delete_custom_food(id);
        self.recipes.write().remove(id);
        self.custom_foods.write().retain(|f| f.id != id);
    }

    /// Replaces a logged entry's amount, nutrients and hour.
    pub fn update_entry(mut self, id: i64, hour: u32, item: PlateItem) {
        let mut log = self.log.write();
        let Some(entry) = log.iter_mut().find(|e| e.id == id) else {
            return;
        };
        entry.hour = hour;
        entry.name = item.name;
        entry.amount = item.amount;
        entry.nutrients = item.nutrients;
        entry.food_id = item.food_id;
        self.persist.read().update_entry(entry);
        drop(log);
        self.editing.set(None);
    }

    pub fn delete_entry(mut self, id: i64) {
        self.persist.read().delete_entry(id);
        self.log.write().retain(|e| e.id != id);
    }

    pub fn add_weigh_in(mut self, date: NaiveDate, kg: f64) {
        let weigh_in = WeighIn { date, kg };
        self.persist.read().insert_weigh_in(&weigh_in);
        self.weigh_ins.write().push(weigh_in);
    }

    pub fn add_measurement(mut self, m: Measurement) {
        self.persist.read().insert_measurement(&m);
        let mut list = self.measurements.write();
        list.push(m);
        list.sort_by_key(|m| m.date);
    }

    pub fn delete_measurement(mut self, m: Measurement) {
        self.persist.read().delete_measurement(&m);
        let mut list = self.measurements.write();
        if let Some(i) = list.iter().rposition(|x| *x == m) {
            list.remove(i);
        }
    }

    pub fn delete_weigh_in(mut self, weigh_in: WeighIn) {
        self.persist.read().delete_weigh_in(&weigh_in);
        let mut list = self.weigh_ins.write();
        if let Some(i) = list.iter().rposition(|w| *w == weigh_in) {
            list.remove(i);
        }
    }

    pub fn set_day_status(mut self, date: NaiveDate, status: DayStatus) {
        self.persist.read().set_day_status(date, status);
        let mut map = self.day_status.write();
        if status == DayStatus::Normal {
            map.remove(&date);
        } else {
            map.insert(date, status);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 15).unwrap()
    }

    #[test]
    fn demo_expenditure_adapts_from_prior() {
        let data = crate::seed::demo(date());
        let snapshot = CoachSnapshot::compute(&data, date());
        // The demo person's true expenditure is 2250 kcal; the equation guesses higher.
        assert!(snapshot.prior_kcal > 2500.0);
        assert_eq!(snapshot.expenditure_status(), EstimateStatus::Updated);
        let kcal = snapshot.expenditure_kcal();
        assert!((2150.0..2400.0).contains(&kcal), "expenditure {kcal}");
        assert!(snapshot.average_target.kcal < kcal);
    }

    #[test]
    fn empty_account_uses_prior() {
        let profile = crate::seed::demo(date()).profile;
        let snapshot = CoachSnapshot::compute(&Data::empty(profile), date());
        assert_eq!(snapshot.expenditure_kcal(), snapshot.prior_kcal);
        assert_eq!(snapshot.expenditure_status(), EstimateStatus::Initial);
        assert_eq!(snapshot.average_target.kcal, snapshot.prior_kcal);
    }

    #[test]
    fn check_in_locks_the_basis() {
        let mut data = crate::seed::demo(date());
        let live = CoachSnapshot::compute(&data, date());
        // Before any check-in, targets follow the live estimate.
        assert_eq!(live.week, live.live_week);

        data.check_ins.push(CheckIn {
            date: date() - chrono::Duration::days(3),
            expenditure_kcal: 2000.0,
            trend_kg: Some(72.0),
            week: Vec::new(),
            goal: None,
        });
        let locked = CoachSnapshot::compute(&data, date());
        let expected =
            targets::weekly_plan(&data.program, 2000.0, 72.0, data.goal.rate_pct_per_week);
        assert_eq!(locked.week, expected);
        assert_eq!(locked.live_week, live.live_week);

        // A goal change applies straight away, still from the locked basis.
        // Maintaining at the locked trend weight eats at expenditure...
        data.goal = Goal {
            goal_weight_kg: 72.0,
            rate_pct_per_week: 0.0,
        };
        let maintain = CoachSnapshot::compute(&data, date());
        assert_eq!(maintain.average_target.kcal, 2000.0);
        // ...and well above the goal weight, corrects gently downwards.
        data.goal.goal_weight_kg = 68.0;
        let correcting = CoachSnapshot::compute(&data, date());
        assert!(correcting.average_target.kcal < 2000.0);
    }

    #[test]
    fn manual_targets_replace_coached_ones() {
        let mut data = crate::seed::demo(date());
        let manual = MacroTargets {
            kcal: 2000.0,
            protein_g: 150.0,
            fat_g: 70.0,
            carbs_g: 195.0,
        };
        data.manual_targets = Some(manual);
        let snapshot = CoachSnapshot::compute(&data, date());
        assert_eq!(snapshot.week, [manual; 7]);
        assert_eq!(snapshot.live_week, [manual; 7]);
        assert_eq!(snapshot.average_target.kcal, 2000.0);
        // Expenditure is still estimated from the data.
        assert_eq!(snapshot.expenditure_status(), EstimateStatus::Updated);
    }

    #[test]
    fn fixed_expenditure_and_smoothing() {
        let mut data = crate::seed::demo(date());
        let adaptive = CoachSnapshot::compute(&data, date());

        data.coach_settings.fixed_expenditure_kcal = Some(2500.0);
        let fixed = CoachSnapshot::compute(&data, date());
        // Targets follow the fixed value; the estimate itself is unchanged.
        assert!(fixed.average_target.kcal > adaptive.average_target.kcal);
        assert_eq!(fixed.expenditure_kcal(), adaptive.expenditure_kcal());

        data.coach_settings = CoachSettings {
            smoothing: TrendSmoothing::Faster,
            ..Default::default()
        };
        let faster = CoachSnapshot::compute(&data, date());
        // The demo person is losing, so a faster trend sits lower.
        assert!(faster.trend_kg().unwrap() < adaptive.trend_kg().unwrap());
    }

    #[test]
    fn water_only_days_count_as_unlogged() {
        let mut data = crate::seed::demo(date());
        let yesterday = date() - Duration::days(1);
        data.log.retain(|e| e.date != yesterday);
        data.log.push(LogEntry {
            id: 9999,
            date: yesterday,
            hour: 9,
            name: "Water".into(),
            amount: "500 ml".into(),
            nutrients: NutrientProfile::new()
                .with(Nutrient::Energy, 0.0)
                .with(Nutrient::Water, 500.0),
            food_id: Some(WATER_ID.into()),
        });
        let records = day_records(&data, date());
        let day = records.iter().find(|r| r.date == yesterday).unwrap();
        assert_eq!(day.intake_kcal, None);
    }

    #[test]
    fn todays_intake_is_excluded() {
        let data = crate::seed::demo(date());
        let records = day_records(&data, date());
        let last = records.last().unwrap();
        assert_eq!(last.date, date());
        assert_eq!(last.intake_kcal, None);
        assert!(last.weight_kg.is_some());
    }

    #[test]
    fn week_start_is_monday() {
        assert_eq!(week_start(date()).weekday(), chrono::Weekday::Mon);
    }
}
