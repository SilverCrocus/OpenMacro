//! Adaptive daily energy expenditure (TDEE) from logged intake and weight change.
//!
//! Energy balance says `intake - expenditure = ρ · Δmass / Δt`, with ρ the
//! energy content of body-mass change ([`KCAL_PER_KG_BODY_MASS`]). Rearranged:
//!
//! ```text
//! expenditure ≈ mean(intake over window) - ρ · slope(trend weight over window)
//! ```
//!
//! Each day we compute that "raw" estimate over a trailing window, then move
//! the running estimate a fraction of the way toward it, with the step size
//! capped. The estimate depends only on what was eaten and how weight changed,
//! never on how closely the user followed their targets.
//!
//! When there isn't enough data (too few usable intake days in the window, or
//! no weigh-in recently) the estimate holds its last value.

use std::collections::BTreeMap;

use chrono::{Duration, NaiveDate};

use crate::KCAL_PER_KG_BODY_MASS;
use crate::trend::{WeighIn, ema_trend};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DayStatus {
    /// Use the logged intake if any; a day with nothing logged is ignored.
    #[default]
    Normal,
    /// The user confirmed they ate nothing: counts as 0 kcal.
    Fasted,
    /// The user flagged the log as incomplete: excluded.
    Partial,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DayRecord {
    pub date: NaiveDate,
    /// Total logged energy for the day, or `None` if nothing was logged.
    pub intake_kcal: Option<f64>,
    pub status: DayStatus,
    /// Average scale weight for the day, if the user weighed in.
    pub weight_kg: Option<f64>,
}

impl DayRecord {
    /// The intake that should count toward expenditure, if any.
    pub fn usable_intake(&self) -> Option<f64> {
        match self.status {
            DayStatus::Normal => self.intake_kcal,
            DayStatus::Fasted => Some(0.0),
            DayStatus::Partial => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExpenditureConfig {
    pub energy_density_kcal_per_kg: f64,
    /// Length of the trailing window used for each raw estimate.
    pub window_days: i64,
    /// Minimum usable intake days inside the window before updating.
    pub min_intake_days: usize,
    /// The user must have weighed in within this many days to update.
    pub weigh_in_lookback_days: i64,
    /// Fraction of the gap to the raw estimate closed each day (0..=1).
    pub response: f64,
    /// Cap on how far the estimate may move in a single day.
    pub max_daily_change_kcal: f64,
    /// EMA alpha for the trend weight used to measure mass change.
    pub trend_alpha: f64,
}

impl Default for ExpenditureConfig {
    fn default() -> Self {
        Self {
            energy_density_kcal_per_kg: KCAL_PER_KG_BODY_MASS,
            window_days: 14,
            min_intake_days: 3,
            weigh_in_lookback_days: 7,
            response: 0.1,
            max_daily_change_kcal: 100.0,
            trend_alpha: 0.1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstimateStatus {
    /// Not enough data has ever been available; still the initial estimate.
    Initial,
    /// Not enough recent data; holding the last estimate.
    Holding,
    /// Updated from today's data.
    Updated,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExpenditurePoint {
    pub date: NaiveDate,
    pub kcal: f64,
    pub status: EstimateStatus,
    /// The unsmoothed window estimate, when one could be computed.
    pub raw_kcal: Option<f64>,
}

/// Runs the estimator over every day from the first to the last record.
///
/// `initial_kcal` is the starting estimate, typically from
/// [`crate::bmr::initial_expenditure`].
pub fn estimate(
    days: &[DayRecord],
    initial_kcal: f64,
    config: &ExpenditureConfig,
) -> Vec<ExpenditurePoint> {
    let by_date: BTreeMap<NaiveDate, &DayRecord> = days.iter().map(|d| (d.date, d)).collect();
    let (Some(&start), Some(&end)) = (by_date.keys().next(), by_date.keys().next_back()) else {
        return Vec::new();
    };

    let weigh_ins: Vec<WeighIn> = days
        .iter()
        .filter_map(|d| d.weight_kg.map(|kg| WeighIn { date: d.date, kg }))
        .collect();
    let trend: BTreeMap<NaiveDate, f64> = ema_trend(&weigh_ins, config.trend_alpha, end)
        .into_iter()
        .map(|p| (p.date, p.trend_kg))
        .collect();

    let mut kcal = initial_kcal;
    let mut status = EstimateStatus::Initial;
    let mut points = Vec::new();

    for date in start.iter_days().take_while(|d| *d <= end) {
        let window_start = (date - Duration::days(config.window_days - 1)).max(start);
        let raw = raw_estimate(&by_date, &trend, window_start, date, config);
        match raw {
            Some(raw) => {
                let max = config.max_daily_change_kcal;
                kcal += (config.response * (raw - kcal)).clamp(-max, max);
                status = EstimateStatus::Updated;
            }
            None if status != EstimateStatus::Initial => status = EstimateStatus::Holding,
            None => {}
        }
        points.push(ExpenditurePoint {
            date,
            kcal,
            status,
            raw_kcal: raw,
        });
    }
    points
}

fn raw_estimate(
    by_date: &BTreeMap<NaiveDate, &DayRecord>,
    trend: &BTreeMap<NaiveDate, f64>,
    from: NaiveDate,
    to: NaiveDate,
    config: &ExpenditureConfig,
) -> Option<f64> {
    let intakes: Vec<f64> = by_date
        .range(from..=to)
        .filter_map(|(_, d)| d.usable_intake())
        .collect();
    if intakes.len() < config.min_intake_days {
        return None;
    }

    let lookback_start = to - Duration::days(config.weigh_in_lookback_days - 1);
    if !by_date
        .range(lookback_start..=to)
        .any(|(_, d)| d.weight_kg.is_some())
    {
        return None;
    }

    let slope_kg_per_day = trend_slope(trend, from, to)?;
    let mean_intake = intakes.iter().sum::<f64>() / intakes.len() as f64;
    Some(mean_intake - config.energy_density_kcal_per_kg * slope_kg_per_day)
}

/// Least-squares slope of the trend weight over `[from, to]`, in kg/day.
fn trend_slope(trend: &BTreeMap<NaiveDate, f64>, from: NaiveDate, to: NaiveDate) -> Option<f64> {
    let points: Vec<(f64, f64)> = trend
        .range(from..=to)
        .map(|(d, kg)| ((*d - from).num_days() as f64, *kg))
        .collect();
    if points.len() < 2 {
        return None;
    }
    let n = points.len() as f64;
    let mean_x = points.iter().map(|p| p.0).sum::<f64>() / n;
    let mean_y = points.iter().map(|p| p.1).sum::<f64>() / n;
    let sxx: f64 = points.iter().map(|p| (p.0 - mean_x).powi(2)).sum();
    let sxy: f64 = points.iter().map(|p| (p.0 - mean_x) * (p.1 - mean_y)).sum();
    (sxx > 0.0).then(|| sxy / sxx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(i: i64) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() + Duration::days(i)
    }

    /// A person with a fixed true expenditure, eating with day-to-day variation,
    /// whose scale weight carries water noise on top of real mass change.
    fn simulate(days: i64, true_tdee: f64) -> Vec<DayRecord> {
        let mut mass = 80.0;
        (0..days)
            .map(|i| {
                let intake = 2000.0 + 200.0 * (i as f64 * 0.9).sin();
                let scale = mass + 0.5 * (i as f64 * 2.3).sin();
                mass += (intake - true_tdee) / KCAL_PER_KG_BODY_MASS;
                DayRecord {
                    date: day(i),
                    intake_kcal: Some(intake),
                    status: DayStatus::Normal,
                    weight_kg: Some(scale),
                }
            })
            .collect()
    }

    #[test]
    fn converges_to_true_expenditure() {
        let points = estimate(&simulate(90, 2500.0), 2200.0, &ExpenditureConfig::default());
        let last = points.last().unwrap();
        assert_eq!(last.status, EstimateStatus::Updated);
        assert!((last.kcal - 2500.0).abs() < 60.0, "estimate {}", last.kcal);
    }

    #[test]
    fn holds_when_logging_stops() {
        let mut days = simulate(40, 2500.0);
        for d in &mut days[20..] {
            d.intake_kcal = None;
        }
        let points = estimate(&days, 2200.0, &ExpenditureConfig::default());
        // Day 31 (index 30) is the last whose 14-day window still holds 3 logged days.
        assert_eq!(points[30].status, EstimateStatus::Updated);
        assert_eq!(points[31].status, EstimateStatus::Holding);
        assert_eq!(points.last().unwrap().kcal, points[30].kcal);
    }

    #[test]
    fn needs_recent_weigh_in() {
        let mut days = simulate(30, 2500.0);
        for d in &mut days {
            d.weight_kg = None;
        }
        let points = estimate(&days, 2200.0, &ExpenditureConfig::default());
        assert!(
            points
                .iter()
                .all(|p| p.status == EstimateStatus::Initial && p.kcal == 2200.0)
        );
    }

    #[test]
    fn first_update_needs_three_logged_days() {
        let points = estimate(&simulate(5, 2500.0), 2200.0, &ExpenditureConfig::default());
        let statuses: Vec<_> = points.iter().map(|p| p.status).collect();
        assert_eq!(
            statuses,
            [
                EstimateStatus::Initial,
                EstimateStatus::Initial,
                EstimateStatus::Updated,
                EstimateStatus::Updated,
                EstimateStatus::Updated
            ]
        );
    }

    #[test]
    fn day_status_rules() {
        let base = DayRecord {
            date: day(0),
            intake_kcal: Some(1500.0),
            status: DayStatus::Normal,
            weight_kg: None,
        };
        assert_eq!(base.usable_intake(), Some(1500.0));
        assert_eq!(
            DayRecord {
                intake_kcal: None,
                ..base
            }
            .usable_intake(),
            None
        );
        assert_eq!(
            DayRecord {
                intake_kcal: None,
                status: DayStatus::Fasted,
                ..base
            }
            .usable_intake(),
            Some(0.0)
        );
        assert_eq!(
            DayRecord {
                status: DayStatus::Partial,
                ..base
            }
            .usable_intake(),
            None
        );
    }

    #[test]
    fn empty_input() {
        assert!(estimate(&[], 2000.0, &ExpenditureConfig::default()).is_empty());
    }
}
