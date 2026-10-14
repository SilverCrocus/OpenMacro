//! Goal-driven calorie targets, weekly calorie distribution and macro splits.
//!
//! Pipeline for a coached program:
//! 1. [`average_daily_target`]: expenditure plus the energy implied by the goal rate, clamped to a floor.
//! 2. [`weekly_calories`]: spread that average over the week, optionally shifting calories onto chosen days.
//! 3. [`macros_for_day`]: protein from body weight, then fat and carbs by diet type.

use chrono::Weekday;

use crate::KCAL_PER_KG_BODY_MASS;
use crate::nutrients::{KCAL_PER_G_CARBS, KCAL_PER_G_FAT, KCAL_PER_G_PROTEIN};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CalorieFloor {
    /// Never recommend below 1200 kcal.
    #[default]
    Standard,
    /// Never recommend below 800 kcal. For supervised or very small individuals.
    Low,
    /// No floor.
    None,
}

impl CalorieFloor {
    pub fn kcal(self) -> Option<f64> {
        match self {
            CalorieFloor::Standard => Some(1200.0),
            CalorieFloor::Low => Some(800.0),
            CalorieFloor::None => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CalorieTarget {
    pub kcal: f64,
    /// True when the goal rate would have required eating below the floor.
    pub limited_by_floor: bool,
}

/// The average daily calorie target for a goal.
///
/// `rate_pct_per_week` is the goal rate as a percentage of body weight per
/// week: negative to lose, positive to gain, zero to maintain.
pub fn average_daily_target(
    expenditure_kcal: f64,
    trend_weight_kg: f64,
    rate_pct_per_week: f64,
    floor: CalorieFloor,
) -> CalorieTarget {
    let weekly_change_kg = trend_weight_kg * rate_pct_per_week / 100.0;
    let daily_delta_kcal = weekly_change_kg * KCAL_PER_KG_BODY_MASS / 7.0;
    let target = expenditure_kcal + daily_delta_kcal;
    match floor.kcal() {
        Some(min) if target < min => CalorieTarget {
            kcal: min,
            limited_by_floor: true,
        },
        _ => CalorieTarget {
            kcal: target,
            limited_by_floor: false,
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum WeeklyDistribution {
    #[default]
    Even,
    /// Higher targets on the given days, lower on the rest, same weekly total.
    Shift { high_days: Vec<Weekday> },
}

/// High days get this multiple of the low-day target.
pub const HIGH_DAY_MULTIPLIER: f64 = 1.10;

/// Calorie targets for Monday through Sunday that average to `average_kcal`.
///
/// Falls back to an even distribution if shifting would push low days below the floor.
pub fn weekly_calories(
    average_kcal: f64,
    distribution: &WeeklyDistribution,
    floor: CalorieFloor,
) -> [f64; 7] {
    let even = [average_kcal; 7];
    let WeeklyDistribution::Shift { high_days } = distribution else {
        return even;
    };
    let is_high = |i: usize| {
        high_days
            .iter()
            .any(|d| d.num_days_from_monday() as usize == i)
    };
    let high_count = (0..7).filter(|i| is_high(*i)).count();
    if high_count == 0 || high_count == 7 {
        return even;
    }

    let low =
        average_kcal * 7.0 / ((7 - high_count) as f64 + high_count as f64 * HIGH_DAY_MULTIPLIER);
    if floor.kcal().is_some_and(|min| low < min) {
        return even;
    }
    std::array::from_fn(|i| {
        if is_high(i) {
            low * HIGH_DAY_MULTIPLIER
        } else {
            low
        }
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProteinPreference {
    Low,
    #[default]
    Moderate,
    High,
    ExtraHigh,
}

impl ProteinPreference {
    /// Grams of protein per kilogram of body weight.
    pub fn grams_per_kg(self) -> f64 {
        match self {
            ProteinPreference::Low => 1.4,
            ProteinPreference::Moderate => 1.7,
            ProteinPreference::High => 2.0,
            ProteinPreference::ExtraHigh => 2.4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DietType {
    #[default]
    Balanced,
    LowFat,
    LowCarb,
    Keto,
}

/// Keto caps carbs at a fixed gram amount regardless of calories.
pub const KETO_CARBS_G: f64 = 30.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MacroTargets {
    pub kcal: f64,
    pub protein_g: f64,
    pub fat_g: f64,
    pub carbs_g: f64,
}

/// Splits a day's calories into macros.
///
/// Protein is fixed first. The diet type then fixes either fat (as a share of
/// total calories) or carbs, and the other macro takes what remains. If protein
/// alone exceeds the budget, fat and carbs are zero.
pub fn macros_for_day(kcal: f64, protein_g: f64, diet: DietType) -> MacroTargets {
    let protein_kcal = protein_g * KCAL_PER_G_PROTEIN;
    let remaining = (kcal - protein_kcal).max(0.0);

    let (fat_kcal, carbs_kcal) = match diet {
        DietType::Balanced => fat_share(kcal, 0.30, remaining),
        DietType::LowFat => fat_share(kcal, 0.20, remaining),
        DietType::LowCarb => carbs_fixed(kcal * 0.20, remaining),
        DietType::Keto => carbs_fixed(KETO_CARBS_G * KCAL_PER_G_CARBS, remaining),
    };

    MacroTargets {
        kcal,
        protein_g,
        fat_g: fat_kcal / KCAL_PER_G_FAT,
        carbs_g: carbs_kcal / KCAL_PER_G_CARBS,
    }
}

fn fat_share(kcal: f64, share: f64, remaining: f64) -> (f64, f64) {
    let fat = (kcal * share).min(remaining);
    (fat, remaining - fat)
}

fn carbs_fixed(carbs_kcal: f64, remaining: f64) -> (f64, f64) {
    let carbs = carbs_kcal.min(remaining);
    (remaining - carbs, carbs)
}

#[derive(Debug, Clone, PartialEq)]
pub struct CoachedProgram {
    pub diet: DietType,
    pub protein: ProteinPreference,
    pub floor: CalorieFloor,
    pub distribution: WeeklyDistribution,
}

/// Macro targets for Monday through Sunday.
pub fn weekly_plan(
    program: &CoachedProgram,
    expenditure_kcal: f64,
    trend_weight_kg: f64,
    rate_pct_per_week: f64,
) -> [MacroTargets; 7] {
    let average = average_daily_target(
        expenditure_kcal,
        trend_weight_kg,
        rate_pct_per_week,
        program.floor,
    );
    let calories = weekly_calories(average.kcal, &program.distribution, program.floor);
    let protein_g = trend_weight_kg * program.protein.grams_per_kg();
    calories.map(|kcal| macros_for_day(kcal, protein_g, program.diet))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deficit_from_goal_rate() {
        // 68.4 kg losing 0.5 %/week = 0.342 kg/week ≈ 376 kcal/day below expenditure.
        let t = average_daily_target(2128.0, 68.4, -0.5, CalorieFloor::Standard);
        assert_eq!(t.kcal.round(), 1752.0);
        assert!(!t.limited_by_floor);

        let maintain = average_daily_target(2128.0, 68.4, 0.0, CalorieFloor::Standard);
        assert_eq!(maintain.kcal, 2128.0);
    }

    #[test]
    fn floor_clamps() {
        let t = average_daily_target(1500.0, 60.0, -1.0, CalorieFloor::Standard);
        assert_eq!(
            t,
            CalorieTarget {
                kcal: 1200.0,
                limited_by_floor: true
            }
        );
        let none = average_daily_target(1500.0, 60.0, -1.0, CalorieFloor::None);
        assert!(none.kcal < 1200.0 && !none.limited_by_floor);
    }

    #[test]
    fn shifted_week_keeps_average() {
        let shift = WeeklyDistribution::Shift {
            high_days: vec![Weekday::Fri, Weekday::Sat],
        };
        let week = weekly_calories(1752.0, &shift, CalorieFloor::Standard);
        assert_eq!(
            week.map(f64::round),
            [1703.0, 1703.0, 1703.0, 1703.0, 1874.0, 1874.0, 1703.0]
        );
        assert!((week.iter().sum::<f64>() / 7.0 - 1752.0).abs() < 1e-9);
    }

    #[test]
    fn shift_falls_back_to_even_below_floor() {
        let shift = WeeklyDistribution::Shift {
            high_days: vec![Weekday::Sat, Weekday::Sun],
        };
        assert_eq!(
            weekly_calories(1210.0, &shift, CalorieFloor::Standard),
            [1210.0; 7]
        );
    }

    #[test]
    fn balanced_split() {
        let protein = 68.4 * ProteinPreference::Moderate.grams_per_kg();
        let m = macros_for_day(1752.0, protein, DietType::Balanced);
        assert_eq!(m.protein_g.round(), 116.0);
        assert_eq!(m.fat_g.round(), 58.0);
        assert_eq!(m.carbs_g.round(), 190.0);
    }

    #[test]
    fn keto_and_low_carb_fix_carbs() {
        let keto = macros_for_day(2000.0, 120.0, DietType::Keto);
        assert_eq!(keto.carbs_g, KETO_CARBS_G);
        assert!((keto.fat_g - (2000.0 - 480.0 - 120.0) / 9.0).abs() < 1e-9);

        let low_carb = macros_for_day(2000.0, 120.0, DietType::LowCarb);
        assert_eq!(low_carb.carbs_g, 100.0);
    }

    #[test]
    fn protein_exceeding_budget_zeroes_the_rest() {
        let m = macros_for_day(400.0, 150.0, DietType::Balanced);
        assert_eq!((m.fat_g, m.carbs_g), (0.0, 0.0));
    }

    #[test]
    fn coached_plan_end_to_end() {
        let program = CoachedProgram {
            diet: DietType::Balanced,
            protein: ProteinPreference::Moderate,
            floor: CalorieFloor::Standard,
            distribution: WeeklyDistribution::Even,
        };
        let plan = weekly_plan(&program, 2128.0, 68.4, -0.5);
        assert!(
            plan.iter()
                .all(|d| d.kcal.round() == 1752.0 && d.protein_g.round() == 116.0)
        );
    }
}
