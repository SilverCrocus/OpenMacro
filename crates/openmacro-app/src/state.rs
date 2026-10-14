//! App state: the user's data as plain values, a reactive wrapper around it,
//! and the coaching numbers derived from it.

use std::collections::BTreeMap;

use chrono::{Datelike, Duration, Local, NaiveDate, Timelike};
use dioxus::prelude::*;
use openmacro_core::bmr::{self, ActivityLevel, BmrEquation, BodyStats, Sex};
use openmacro_core::expenditure::{
    self, DayRecord, DayStatus, EstimateStatus, ExpenditureConfig, ExpenditurePoint,
};
use openmacro_core::nutrients::{Nutrient, NutrientProfile};
use openmacro_core::targets::{self, CalorieTarget, CoachedProgram, MacroTargets};
use openmacro_core::trend::{self, TrendPoint, TrendSmoothing, WeighIn};

pub use openmacro_core::log::{Goal, LogEntry, Profile, day_totals};

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
        }
    }
}

/// Expenditure, trend and targets derived from [`Data`].
#[derive(Debug, Clone, PartialEq)]
pub struct CoachSnapshot {
    pub prior_kcal: f64,
    pub expenditure: Vec<ExpenditurePoint>,
    pub trend: Vec<TrendPoint>,
    pub average_target: CalorieTarget,
    /// Monday through Sunday.
    pub week: [MacroTargets; 7],
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
        let trend = trend::ema_trend(&data.weigh_ins, TrendSmoothing::Standard.alpha(), today);

        let kcal = expenditure.last().map_or(prior_kcal, |e| e.kcal);
        let weight = trend.last().map_or(profile.stats.weight_kg, |t| t.trend_kg);
        let rate = data.goal.rate_pct_per_week;
        Self {
            prior_kcal,
            average_target: targets::average_daily_target(kcal, weight, rate, data.program.floor),
            week: targets::weekly_plan(&data.program, kcal, weight, rate),
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
            intake_kcal: if date == today {
                None
            } else {
                intake.get(&date).copied()
            },
            status: data.day_status.get(&date).copied().unwrap_or_default(),
            weight_kg: weights.get(&date).copied(),
        })
        .collect()
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
        let data = persist
            .load()
            .unwrap_or_else(|| Data::empty(default_profile()));
        Self {
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

    pub fn data(&self) -> Data {
        Data {
            log: self.log.read().clone(),
            weigh_ins: self.weigh_ins.read().clone(),
            day_status: self.day_status.read().clone(),
            profile: *self.profile.read(),
            goal: *self.goal.read(),
            program: self.program.read().clone(),
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
        self.plate.set(Plate::closed(today()));
        self.selected_date.set(today());
    }

    pub fn open_plate(mut self, date: NaiveDate, hour: u32) {
        let mut plate = self.plate.write();
        plate.open = true;
        plate.date = date;
        plate.hour = hour;
    }

    pub fn close_plate(mut self) {
        let mut plate = self.plate.write();
        plate.open = false;
        plate.items.clear();
    }

    pub fn add_to_plate(mut self, item: PlateItem) {
        self.plate.write().items.push(item);
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
        let entries: Vec<LogEntry> = items
            .into_iter()
            .map(|item| {
                let mut entry = LogEntry {
                    id: 0,
                    date,
                    hour,
                    name: item.name,
                    amount: item.amount,
                    nutrients: item.nutrients,
                };
                entry.id = match self.persist.read().insert_entry(&entry) {
                    Some(id) => id,
                    None => {
                        let id = *self.next_id.peek();
                        self.next_id.set(id + 1);
                        id
                    }
                };
                entry
            })
            .collect();
        self.log.write().extend(entries);
        self.selected_date.set(date);
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
