//! Adaptive daily energy expenditure (TDEE) from logged intake and weight change.
//!
//! Energy balance says `intake - expenditure = ρ · Δmass / Δt`, with ρ the
//! energy content of body-mass change ([`KCAL_PER_KG_BODY_MASS`]). Rearranged,
//! over a trailing window of days:
//!
//! ```text
//! raw ≈ mean(usable intake in window) - ρ · slope(scale weight in window)
//! ```
//!
//! The slope is a least-squares fit to the raw weigh-ins, so it has no lag.
//! Scale weight is noisy, though, so each day's `raw` is treated as a noisy
//! measurement of the true expenditure and blended in with a one-dimensional
//! Kalman update: the estimate's variance grows a little every day (expenditure
//! drifts), and shrinks when a measurement arrives, by an amount that depends on
//! how informative that window is. Two weigh-ins three days apart barely move
//! the estimate; two weeks of daily weigh-ins move it a lot.
//!
//! The estimate depends only on what was eaten and how weight changed, never on
//! how closely the user followed their targets. When there isn't enough data
//! (too few usable intake days in the window, too few weigh-ins, or none
//! recently) the estimate holds its last value.

use std::collections::BTreeMap;

use chrono::{Duration, NaiveDate};

use crate::KCAL_PER_KG_BODY_MASS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
    /// Length of the trailing window used for each measurement.
    pub window_days: i64,
    /// Minimum usable intake days inside the window before updating.
    pub min_intake_days: usize,
    /// The user must have weighed in within this many days to update.
    pub weigh_in_lookback_days: i64,
    /// Assumed day-to-day scale noise (water, gut contents), 1σ.
    pub weight_noise_kg: f64,
    /// Assumed day-to-day variation in logged intake, 1σ.
    pub intake_noise_kcal: f64,
    /// How much true expenditure may drift per day, 1σ.
    pub process_noise_kcal: f64,
    /// Uncertainty of the initial estimate, 1σ.
    pub initial_uncertainty_kcal: f64,
    /// Consecutive windows share most of their days, so their measurements are
    /// correlated. Each measurement's variance is multiplied by
    /// `window_days * overlap_factor` to avoid counting the same data many times.
    pub overlap_factor: f64,
    /// Cap on how far the estimate may move in a single day.
    pub max_daily_change_kcal: f64,
}

impl Default for ExpenditureConfig {
    fn default() -> Self {
        Self {
            energy_density_kcal_per_kg: KCAL_PER_KG_BODY_MASS,
            window_days: 14,
            min_intake_days: 3,
            weigh_in_lookback_days: 7,
            weight_noise_kg: 0.6,
            intake_noise_kcal: 350.0,
            process_noise_kcal: 40.0,
            initial_uncertainty_kcal: 350.0,
            overlap_factor: 0.5,
            max_daily_change_kcal: 150.0,
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
    /// One standard deviation of the estimate, for drawing an uncertainty band.
    pub uncertainty_kcal: f64,
    pub status: EstimateStatus,
    /// The window measurement used today, when there was one.
    pub raw_kcal: Option<f64>,
}

/// A window's expenditure measurement and its variance.
struct Measurement {
    kcal: f64,
    variance: f64,
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

    let mut kcal = initial_kcal;
    let mut variance = config.initial_uncertainty_kcal.powi(2);
    let mut status = EstimateStatus::Initial;
    let mut points = Vec::new();

    for date in start.iter_days().take_while(|d| *d <= end) {
        variance += config.process_noise_kcal.powi(2);
        let window_start = (date - Duration::days(config.window_days - 1)).max(start);
        let measurement = measure(&by_date, window_start, date, config);
        match &measurement {
            Some(m) => {
                let gain = variance / (variance + m.variance);
                let max = config.max_daily_change_kcal;
                kcal += (gain * (m.kcal - kcal)).clamp(-max, max);
                variance *= 1.0 - gain;
                status = EstimateStatus::Updated;
            }
            None if status != EstimateStatus::Initial => status = EstimateStatus::Holding,
            None => {}
        }
        points.push(ExpenditurePoint {
            date,
            kcal,
            uncertainty_kcal: variance.sqrt(),
            status,
            raw_kcal: measurement.map(|m| m.kcal),
        });
    }
    points
}

fn measure(
    by_date: &BTreeMap<NaiveDate, &DayRecord>,
    from: NaiveDate,
    to: NaiveDate,
    config: &ExpenditureConfig,
) -> Option<Measurement> {
    let window = || by_date.range(from..=to).map(|(_, d)| *d);

    let intakes: Vec<f64> = window().filter_map(DayRecord::usable_intake).collect();
    if intakes.len() < config.min_intake_days {
        return None;
    }

    let lookback_start = to - Duration::days(config.weigh_in_lookback_days - 1);
    if !window().any(|d| d.date >= lookback_start && d.weight_kg.is_some()) {
        return None;
    }

    let weights: Vec<(f64, f64)> = window()
        .filter_map(|d| {
            d.weight_kg
                .map(|kg| ((d.date - from).num_days() as f64, kg))
        })
        .collect();
    let (slope, sxx) = least_squares_slope(&weights)?;

    let n = intakes.len() as f64;
    let mean_intake = intakes.iter().sum::<f64>() / n;
    let rho = config.energy_density_kcal_per_kg;
    let variance = ((rho * config.weight_noise_kg).powi(2) / sxx
        + config.intake_noise_kcal.powi(2) / n)
        * config.window_days as f64
        * config.overlap_factor;
    Some(Measurement {
        kcal: mean_intake - rho * slope,
        variance,
    })
}

/// Least-squares slope of `(x, y)` points and the spread of x (Σ(x - x̄)²).
fn least_squares_slope(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    if points.len() < 2 {
        return None;
    }
    let n = points.len() as f64;
    let mean_x = points.iter().map(|p| p.0).sum::<f64>() / n;
    let mean_y = points.iter().map(|p| p.1).sum::<f64>() / n;
    let sxx: f64 = points.iter().map(|p| (p.0 - mean_x).powi(2)).sum();
    let sxy: f64 = points.iter().map(|p| (p.0 - mean_x) * (p.1 - mean_y)).sum();
    (sxx > 0.0).then(|| (sxy / sxx, sxx))
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
    fn correct_prior_is_not_dragged_down_early() {
        // A lagging trend would read early weight loss as too slow and pull the
        // estimate below the truth; the raw-weight slope should not.
        let points = estimate(&simulate(28, 2500.0), 2500.0, &ExpenditureConfig::default());
        for p in &points {
            assert!((p.kcal - 2500.0).abs() < 120.0, "{}: {}", p.date, p.kcal);
        }
    }

    #[test]
    fn uncertainty_shrinks_with_data() {
        let points = estimate(&simulate(60, 2500.0), 2200.0, &ExpenditureConfig::default());
        let first = points[0].uncertainty_kcal;
        let last = points.last().unwrap().uncertainty_kcal;
        assert!(last < first * 0.6, "first {first}, last {last}");
    }

    #[test]
    fn empty_input() {
        assert!(estimate(&[], 2000.0, &ExpenditureConfig::default()).is_empty());
    }
}
