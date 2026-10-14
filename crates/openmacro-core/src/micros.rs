//! Standard daily targets for fibre, vitamins, minerals and a few others.
//!
//! Values are the US Dietary Reference Intakes (National Academies) for
//! healthy adults: the RDA, or the AI where there is no RDA, as the target,
//! and the tolerable upper intake level as the ceiling where it applies to
//! food (several ULs cover supplements only and are left out). Fibre,
//! saturated fat and added sugars scale with the day's energy.

use std::collections::BTreeMap;

use crate::bmr::Sex;
use crate::nutrients::{KCAL_PER_G_CARBS, KCAL_PER_G_FAT, Nutrient, NutrientGoal};

fn goal(target: f64, ceiling: Option<f64>) -> NutrientGoal {
    NutrientGoal {
        floor: None,
        target: Some(target),
        ceiling,
    }
}

fn ceiling_only(ceiling: f64) -> NutrientGoal {
    NutrientGoal {
        floor: None,
        target: None,
        ceiling: Some(ceiling),
    }
}

/// Standard goals for an adult of this sex and age eating `kcal` a day.
pub fn standard_goals(sex: Sex, age_years: f64, kcal: f64) -> BTreeMap<Nutrient, NutrientGoal> {
    let male = sex == Sex::Male;
    let by_sex = |m: f64, f: f64| if male { m } else { f };
    let over = |age: f64| age_years > age;
    use Nutrient::*;
    let mut goals = BTreeMap::new();
    let mut set = |n: Nutrient, g: NutrientGoal| {
        goals.insert(n, g);
    };

    set(Fiber, goal(14.0 * kcal / 1000.0, None));
    set(Saturated, ceiling_only(0.10 * kcal / KCAL_PER_G_FAT));
    set(AddedSugars, ceiling_only(0.10 * kcal / KCAL_PER_G_CARBS));
    set(Omega3Ala, goal(by_sex(1.6, 1.1), None));
    set(Omega6, goal(by_sex(17.0, 12.0), None));

    set(VitaminA, goal(by_sex(900.0, 700.0), Some(3000.0)));
    set(VitaminC, goal(by_sex(90.0, 75.0), Some(2000.0)));
    set(
        VitaminD,
        goal(if over(70.0) { 20.0 } else { 15.0 }, Some(100.0)),
    );
    set(VitaminE, goal(15.0, None));
    set(VitaminK, goal(by_sex(120.0, 90.0), None));
    set(VitaminB1, goal(by_sex(1.2, 1.1), None));
    set(VitaminB2, goal(by_sex(1.3, 1.1), None));
    set(VitaminB3, goal(by_sex(16.0, 14.0), None));
    set(VitaminB5, goal(5.0, None));
    set(
        VitaminB6,
        goal(if over(50.0) { by_sex(1.7, 1.5) } else { 1.3 }, Some(100.0)),
    );
    set(VitaminB12, goal(2.4, None));
    set(Folate, goal(400.0, None));
    set(Choline, goal(by_sex(550.0, 425.0), Some(3500.0)));

    let calcium = if (!male && over(50.0)) || over(70.0) {
        1200.0
    } else {
        1000.0
    };
    let calcium_ul = if over(50.0) { 2000.0 } else { 2500.0 };
    set(Calcium, goal(calcium, Some(calcium_ul)));
    set(Copper, goal(0.9, Some(10.0)));
    set(
        Iron,
        goal(if male || over(50.0) { 8.0 } else { 18.0 }, Some(45.0)),
    );
    set(
        Magnesium,
        goal(
            if over(30.0) {
                by_sex(420.0, 320.0)
            } else {
                by_sex(400.0, 310.0)
            },
            None,
        ),
    );
    set(Manganese, goal(by_sex(2.3, 1.8), Some(11.0)));
    set(Phosphorus, goal(700.0, Some(4000.0)));
    set(Potassium, goal(by_sex(3400.0, 2600.0), None));
    set(Selenium, goal(55.0, Some(400.0)));
    // Sodium: the adequate intake, and the level to stay under for health.
    set(Sodium, goal(1500.0, Some(2300.0)));
    set(Zinc, goal(by_sex(11.0, 8.0), Some(40.0)));
    set(Caffeine, ceiling_only(400.0));
    goals
}

/// Where a day's (or an average day's) intake sits against a goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// Under the floor, or under two thirds of the target.
    Low,
    /// Under the target.
    Under,
    /// At or over the target and under any ceiling.
    Met,
    /// Over the ceiling.
    Over,
}

pub fn standing(goal: &NutrientGoal, amount: f64) -> Standing {
    if goal.ceiling.is_some_and(|c| amount > c) {
        return Standing::Over;
    }
    if goal.floor.is_some_and(|f| amount < f) {
        return Standing::Low;
    }
    match goal.target {
        Some(t) if amount < t * 2.0 / 3.0 => Standing::Low,
        Some(t) if amount < t => Standing::Under,
        _ => Standing::Met,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goals_depend_on_sex_age_and_energy() {
        let man = standard_goals(Sex::Male, 35.0, 2500.0);
        let woman = standard_goals(Sex::Female, 35.0, 2000.0);
        let older_woman = standard_goals(Sex::Female, 60.0, 2000.0);
        assert_eq!(man[&Nutrient::Iron].target, Some(8.0));
        assert_eq!(woman[&Nutrient::Iron].target, Some(18.0));
        assert_eq!(older_woman[&Nutrient::Iron].target, Some(8.0));
        assert_eq!(older_woman[&Nutrient::Calcium].target, Some(1200.0));
        assert_eq!(man[&Nutrient::Fiber].target, Some(35.0));
        assert_eq!(woman[&Nutrient::Fiber].target, Some(28.0));
        // 10% of 2000 kcal from saturated fat is about 22 g.
        assert!((woman[&Nutrient::Saturated].ceiling.unwrap() - 22.22).abs() < 0.01);
        assert!(man.values().all(NutrientGoal::is_valid));
    }

    #[test]
    fn standings() {
        let sodium = NutrientGoal {
            floor: None,
            target: Some(1500.0),
            ceiling: Some(2300.0),
        };
        assert_eq!(standing(&sodium, 900.0), Standing::Low);
        assert_eq!(standing(&sodium, 1200.0), Standing::Under);
        assert_eq!(standing(&sodium, 1800.0), Standing::Met);
        assert_eq!(standing(&sodium, 2600.0), Standing::Over);
        let saturated = ceiling_only(22.0);
        assert_eq!(standing(&saturated, 5.0), Standing::Met);
        assert_eq!(standing(&saturated, 30.0), Standing::Over);
    }
}
