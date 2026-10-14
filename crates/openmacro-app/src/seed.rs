//! Demo data: five weeks of plausible logging and weigh-ins, generated from a
//! simulated person with a known expenditure so the coach has something to show.

use std::collections::BTreeMap;

use chrono::{Duration, NaiveDate};
use openmacro_core::KCAL_PER_KG_BODY_MASS;
use openmacro_core::bmr::{ActivityLevel, BmrEquation, BodyStats, Sex};
use openmacro_core::expenditure::DayStatus;
use openmacro_core::food::Amount;
use openmacro_core::nutrients::Nutrient;
use openmacro_core::targets::{CoachedProgram, WeeklyDistribution};
use openmacro_core::trend::WeighIn;

use crate::foods::{food_by_id, plate_item};
use crate::state::{Data, Goal, LogEntry, Profile};

/// (hour, food id, servings)
type Meal = (u32, &'static str, f64);

const DAY_PLANS: [&[Meal]; 3] = [
    &[
        (8, "oats", 0.5),
        (8, "banana", 1.0),
        (8, "milk", 1.0),
        (10, "greek-yogurt", 1.0),
        (13, "chicken-breast", 1.0),
        (13, "white-rice", 1.0),
        (13, "broccoli", 1.0),
        (13, "olive-oil", 1.0),
        (16, "apple", 1.0),
        (16, "almonds", 1.0),
        (19, "salmon", 1.0),
        (19, "potato", 1.0),
        (19, "broccoli", 1.0),
    ],
    &[
        (8, "eggs", 2.0),
        (8, "whole-wheat-bread", 2.0),
        (8, "butter", 0.5),
        (12, "pasta", 1.5),
        (12, "ground-beef", 1.0),
        (12, "cheddar", 1.0),
        (15, "whey", 1.0),
        (15, "banana", 1.0),
        (19, "chicken-breast", 1.0),
        (19, "potato", 1.0),
        (19, "avocado", 1.0),
    ],
    &[
        (7, "greek-yogurt", 1.0),
        (7, "oats", 0.5),
        (7, "peanut-butter", 1.0),
        (12, "black-beans", 1.0),
        (12, "white-rice", 1.0),
        (12, "avocado", 1.0),
        (12, "cheddar", 1.0),
        (16, "apple", 1.0),
        (16, "whey", 1.0),
        (20, "salmon", 1.0),
        (20, "white-rice", 1.0),
        (20, "broccoli", 1.0),
    ],
];

const HISTORY_DAYS: i64 = 35;
const TRUE_EXPENDITURE: f64 = 2250.0;
/// What the person actually eats on days they don't (fully) log.
const UNLOGGED_INTAKE: f64 = 1850.0;

pub fn demo(today: NaiveDate) -> Data {
    let mut log = Vec::new();
    let mut weigh_ins = Vec::new();
    let mut day_status = BTreeMap::new();
    let mut mass = 72.0;
    let mut id = 1;

    for i in 0..=HISTORY_DAYS {
        let date = today - Duration::days(HISTORY_DAYS - i);
        let skipped = i == 9 || i == 22;
        let partial = i == 16;
        // Today only breakfast is logged so far; a partial day stops after lunch.
        let last_hour = if date == today {
            9
        } else if partial {
            13
        } else {
            23
        };

        let mut logged_kcal = 0.0;
        if !skipped {
            for &(hour, food_id, count) in DAY_PLANS[i as usize % DAY_PLANS.len()] {
                if hour > last_hour {
                    break;
                }
                let food = food_by_id(food_id).expect("demo food exists");
                let item = plate_item(food, Amount::Servings { index: 0, count })
                    .expect("demo serving exists");
                logged_kcal += item.nutrients.get_or_zero(Nutrient::Energy);
                log.push(LogEntry {
                    id,
                    date,
                    hour,
                    name: item.name,
                    amount: item.amount,
                    nutrients: item.nutrients,
                    food_id: item.food_id,
                });
                id += 1;
            }
        }
        if partial {
            day_status.insert(date, DayStatus::Partial);
        }

        if i % 5 != 4 {
            let water_noise = 0.45 * (i as f64 * 2.3).sin();
            weigh_ins.push(WeighIn {
                date,
                kg: ((mass + water_noise) * 10.0).round() / 10.0,
            });
        }
        let eaten = if skipped || partial {
            UNLOGGED_INTAKE
        } else {
            logged_kcal
        };
        mass += (eaten - TRUE_EXPENDITURE) / KCAL_PER_KG_BODY_MASS;
    }

    Data {
        log,
        weigh_ins,
        day_status,
        profile: Profile {
            stats: BodyStats {
                sex: Sex::Male,
                age_years: 32.0,
                height_cm: 178.0,
                weight_kg: 72.0,
                body_fat_fraction: None,
            },
            activity: ActivityLevel::Moderate,
            equation: BmrEquation::MifflinStJeor,
        },
        goal: Goal {
            goal_weight_kg: 68.0,
            rate_pct_per_week: -0.5,
        },
        program: CoachedProgram {
            diet: Default::default(),
            protein: Default::default(),
            floor: Default::default(),
            distribution: WeeklyDistribution::Even,
        },
        check_ins: Vec::new(),
    }
}
