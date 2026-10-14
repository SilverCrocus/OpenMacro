//! Trend weight: a smoothed line through noisy scale weigh-ins.
//!
//! Scale weight swings by a kilogram or more day to day from water, glycogen
//! and gut contents. The trend is an exponential moving average over daily
//! values; days without a weigh-in use a value linearly interpolated between
//! the surrounding weigh-ins, so gaps neither freeze nor jolt the trend.

use std::collections::BTreeMap;

use chrono::NaiveDate;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeighIn {
    pub date: NaiveDate,
    pub kg: f64,
}

/// User-facing smoothing presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrendSmoothing {
    Faster,
    #[default]
    Standard,
    Slower,
}

impl TrendSmoothing {
    /// EMA weight given to each new daily value.
    pub fn alpha(self) -> f64 {
        match self {
            TrendSmoothing::Faster => 0.2,
            TrendSmoothing::Standard => 0.1,
            TrendSmoothing::Slower => 0.05,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrendPoint {
    pub date: NaiveDate,
    pub trend_kg: f64,
    /// The day's average scale weight, if the user weighed in.
    pub scale_kg: Option<f64>,
}

/// Averages multiple weigh-ins on the same day.
pub fn daily_weights(weigh_ins: &[WeighIn]) -> BTreeMap<NaiveDate, f64> {
    let mut sums: BTreeMap<NaiveDate, (f64, u32)> = BTreeMap::new();
    for w in weigh_ins {
        let e = sums.entry(w.date).or_insert((0.0, 0));
        e.0 += w.kg;
        e.1 += 1;
    }
    sums.into_iter()
        .map(|(d, (sum, n))| (d, sum / f64::from(n)))
        .collect()
}

/// Computes the trend for every day from the first weigh-in through `through`.
///
/// After the last weigh-in the trend is held flat: with no new information
/// there is no reason to keep moving it.
pub fn ema_trend(weigh_ins: &[WeighIn], alpha: f64, through: NaiveDate) -> Vec<TrendPoint> {
    let daily = daily_weights(weigh_ins);
    let Some((&first_date, &first_kg)) = daily.first_key_value() else {
        return Vec::new();
    };

    let mut points = vec![TrendPoint {
        date: first_date,
        trend_kg: first_kg,
        scale_kg: Some(first_kg),
    }];
    let mut trend = first_kg;

    for date in first_date.iter_days().skip(1).take_while(|d| *d <= through) {
        let scale = daily.get(&date).copied();
        let value = scale.or_else(|| interpolate(&daily, date));
        if let Some(v) = value {
            trend += alpha * (v - trend);
        }
        points.push(TrendPoint {
            date,
            trend_kg: trend,
            scale_kg: scale,
        });
    }
    points
}

/// Linear interpolation between the weigh-ins either side of `date`, or `None`
/// if `date` is after the last weigh-in.
fn interpolate(daily: &BTreeMap<NaiveDate, f64>, date: NaiveDate) -> Option<f64> {
    let (&prev_date, &prev_kg) = daily.range(..date).next_back()?;
    let (&next_date, &next_kg) = daily.range(date..).next()?;
    let span = (next_date - prev_date).num_days() as f64;
    let offset = (date - prev_date).num_days() as f64;
    Some(prev_kg + (next_kg - prev_kg) * offset / span)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 1, day).unwrap()
    }

    #[test]
    fn empty_input() {
        assert!(ema_trend(&[], 0.1, d(10)).is_empty());
    }

    #[test]
    fn same_day_weigh_ins_are_averaged() {
        let w = [
            WeighIn {
                date: d(1),
                kg: 70.0,
            },
            WeighIn {
                date: d(1),
                kg: 71.0,
            },
        ];
        let t = ema_trend(&w, 0.1, d(1));
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].trend_kg, 70.5);
    }

    #[test]
    fn gaps_are_interpolated_and_tail_is_flat() {
        let w = [
            WeighIn {
                date: d(1),
                kg: 70.0,
            },
            WeighIn {
                date: d(3),
                kg: 72.0,
            },
        ];
        let t = ema_trend(&w, 0.5, d(5));
        assert_eq!(t.len(), 5);
        // Day 2 uses the interpolated 71.0: 70 + 0.5 * (71 - 70).
        assert_eq!(t[1].trend_kg, 70.5);
        assert_eq!(t[1].scale_kg, None);
        // Day 3 uses the real weigh-in: 70.5 + 0.5 * (72 - 70.5).
        assert_eq!(t[2].trend_kg, 71.25);
        // No weigh-ins after day 3: the trend holds.
        assert_eq!(t[4].trend_kg, 71.25);
    }

    #[test]
    fn trend_suppresses_noise() {
        // Alternating ±1 kg noise around a steady 70 kg.
        let w: Vec<WeighIn> = (1..=30)
            .map(|day| WeighIn {
                date: d(day),
                kg: if day % 2 == 0 { 71.0 } else { 69.0 },
            })
            .collect();
        let last = *ema_trend(&w, TrendSmoothing::Standard.alpha(), d(30))
            .last()
            .unwrap();
        assert!(
            (last.trend_kg - 70.0).abs() < 0.2,
            "trend {}",
            last.trend_kg
        );
    }
}
