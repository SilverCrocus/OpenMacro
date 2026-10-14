//! Basal metabolic rate equations and the initial expenditure estimate.
//!
//! The initial estimate is only a starting point: once the user has logged
//! enough intake and weight data, [`crate::expenditure`] takes over.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sex {
    Male,
    Female,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyStats {
    pub sex: Sex,
    pub age_years: f64,
    pub height_cm: f64,
    pub weight_kg: f64,
    /// Body fat as a fraction (0.25 = 25 %), if known.
    pub body_fat_fraction: Option<f64>,
}

impl BodyStats {
    pub fn lean_mass_kg(&self) -> Option<f64> {
        self.body_fat_fraction
            .filter(|bf| (0.0..1.0).contains(bf))
            .map(|bf| self.weight_kg * (1.0 - bf))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BmrEquation {
    /// Mifflin-St Jeor (1990). Needs no body-fat estimate.
    #[default]
    MifflinStJeor,
    /// Katch-McArdle. Needs lean mass.
    KatchMcArdle,
    /// Cunningham (1980). Needs lean mass; tends to suit active people.
    Cunningham,
}

/// BMR in kcal/day, or `None` if the equation needs a body-fat estimate that is missing.
pub fn bmr(equation: BmrEquation, stats: &BodyStats) -> Option<f64> {
    match equation {
        BmrEquation::MifflinStJeor => {
            let sex_offset = match stats.sex {
                Sex::Male => 5.0,
                Sex::Female => -161.0,
            };
            Some(
                10.0 * stats.weight_kg + 6.25 * stats.height_cm - 5.0 * stats.age_years
                    + sex_offset,
            )
        }
        BmrEquation::KatchMcArdle => stats.lean_mass_kg().map(|lbm| 370.0 + 21.6 * lbm),
        BmrEquation::Cunningham => stats.lean_mass_kg().map(|lbm| 500.0 + 22.0 * lbm),
    }
}

/// Day-to-day activity outside planned exercise, combined with training volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActivityLevel {
    /// Desk job, little walking, no exercise.
    Sedentary,
    /// Light activity or 1-3 exercise sessions a week.
    Light,
    /// On your feet a fair amount, or 3-5 sessions a week.
    #[default]
    Moderate,
    /// Physical job or 6-7 sessions a week.
    Very,
    /// Physical job plus hard daily training.
    Extreme,
}

impl ActivityLevel {
    /// Physical activity level multiplier applied to BMR.
    pub fn factor(self) -> f64 {
        match self {
            ActivityLevel::Sedentary => 1.2,
            ActivityLevel::Light => 1.375,
            ActivityLevel::Moderate => 1.55,
            ActivityLevel::Very => 1.725,
            ActivityLevel::Extreme => 1.9,
        }
    }
}

/// Starting expenditure estimate in kcal/day. Falls back to Mifflin-St Jeor
/// when the chosen equation needs a body-fat estimate the user hasn't given.
pub fn initial_expenditure(
    stats: &BodyStats,
    equation: BmrEquation,
    activity: ActivityLevel,
) -> f64 {
    let base = bmr(equation, stats)
        .or_else(|| bmr(BmrEquation::MifflinStJeor, stats))
        .expect("Mifflin-St Jeor needs no optional inputs");
    base * activity.factor()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats() -> BodyStats {
        BodyStats {
            sex: Sex::Male,
            age_years: 30.0,
            height_cm: 180.0,
            weight_kg: 80.0,
            body_fat_fraction: Some(0.15),
        }
    }

    #[test]
    fn equations() {
        let s = stats();
        assert_eq!(bmr(BmrEquation::MifflinStJeor, &s), Some(1780.0));
        assert!((bmr(BmrEquation::KatchMcArdle, &s).unwrap() - 1838.8).abs() < 1e-9);
        assert!((bmr(BmrEquation::Cunningham, &s).unwrap() - 1996.0).abs() < 1e-9);

        let female = BodyStats {
            sex: Sex::Female,
            ..s
        };
        assert_eq!(bmr(BmrEquation::MifflinStJeor, &female), Some(1614.0));
    }

    #[test]
    fn lean_mass_equations_need_body_fat() {
        let s = BodyStats {
            body_fat_fraction: None,
            ..stats()
        };
        assert_eq!(bmr(BmrEquation::Cunningham, &s), None);
        // Falls back to Mifflin-St Jeor.
        assert!(
            (initial_expenditure(&s, BmrEquation::Cunningham, ActivityLevel::Sedentary) - 2136.0)
                .abs()
                < 1e-9
        );
    }
}
