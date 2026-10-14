//! Goal and coached program settings, and the resulting weekly targets.

use chrono::Weekday;
use dioxus::prelude::*;
use openmacro_core::KCAL_PER_KG_BODY_MASS;
use openmacro_core::expenditure::EstimateStatus;
use openmacro_core::targets::{
    CalorieFloor, DietType, HIGH_DAY_MULTIPLIER, ProteinPreference, WeeklyDistribution,
};

use super::{fmt0, fmt1};
use crate::state::{AppState, CoachSnapshot};

const WEEKDAYS: [(Weekday, &str); 7] = [
    (Weekday::Mon, "M"),
    (Weekday::Tue, "T"),
    (Weekday::Wed, "W"),
    (Weekday::Thu, "T"),
    (Weekday::Fri, "F"),
    (Weekday::Sat, "S"),
    (Weekday::Sun, "S"),
];

#[component]
pub fn Plan() -> Element {
    let mut state = use_context::<AppState>();
    let coach = use_context::<Memo<CoachSnapshot>>();
    let coach = coach.read();
    let goal = (state.goal)();
    let program = state.program.read().clone();

    let expenditure = coach.expenditure_kcal();
    let weight = coach
        .trend_kg()
        .unwrap_or(state.profile.read().stats.weight_kg);
    let weekly_kg = weight * goal.rate_pct_per_week / 100.0;
    let daily_delta = weekly_kg * KCAL_PER_KG_BODY_MASS / 7.0;
    let to_go = goal.goal_weight_kg - weight;
    let eta = if weekly_kg.abs() < 1e-6 || to_go.signum() != weekly_kg.signum() {
        "—".to_string()
    } else {
        format!("{:.0} weeks", (to_go / weekly_kg).abs().ceil())
    };
    let rate_text = format!(
        "{:+.2}% / week ({:+.2} kg)",
        goal.rate_pct_per_week, weekly_kg
    );
    let high_days = match &program.distribution {
        WeeklyDistribution::Even => Vec::new(),
        WeeklyDistribution::Shift { high_days } => high_days.clone(),
    };
    let shifting = !high_days.is_empty();
    let week = coach.week;
    let bar_max = week.iter().map(|d| d.kcal).fold(1.0, f64::max);
    let status = match coach.expenditure_status() {
        EstimateStatus::Initial => "from your profile; it will adapt once you log for a few days",
        EstimateStatus::Holding => "holding: log food and weigh in to resume updates",
        EstimateStatus::Updated => "from your logged intake and weight trend",
    };
    let check_in_day = (state.check_in_day)();
    // Targets come from the last check-in's expenditure; say so, and show
    // what a check-in today would change.
    let locked = coach
        .last_check_in
        .as_ref()
        .map(|c| (c.expenditure_kcal, c.date.format("%-d %B").to_string()));
    let basis_kcal = locked.as_ref().map_or(expenditure, |(kcal, _)| *kcal);
    let live_change = coach.live_target.kcal - coach.average_target.kcal;
    let floor_note = if coach.average_target.limited_by_floor {
        " Your goal rate would go below your calorie floor, so the target is held at the floor; consider a slower rate."
    } else {
        ""
    };

    rsx! {
        header { class: "page-header",
            h1 { "Plan" }
        }
        super::checkin::CheckInCard { only_when_due: false }
        section { class: "card",
            h2 { "Goal" }
            div { class: "stat-row",
                div { class: "stat",
                    span { class: "num", {fmt1(weight)} }
                    span { class: "muted", "Trend kg" }
                }
                div { class: "stat",
                    span { class: "num", {fmt1(goal.goal_weight_kg)} }
                    span { class: "muted", "Goal kg" }
                }
                div { class: "stat",
                    span { class: "num", "{eta}" }
                    span { class: "muted", "Estimated" }
                }
            }
            label { class: "field",
                span { "Goal weight (kg)" }
                input {
                    r#type: "number",
                    step: "0.1",
                    value: "{goal.goal_weight_kg}",
                    onchange: move |e| {
                        if let Ok(v) = e.value().parse::<f64>() {
                            state.goal.write().goal_weight_kg = v;
                        }
                    },
                }
            }
            label { class: "field",
                span { "Rate: {rate_text}" }
                input {
                    r#type: "range",
                    min: "-1",
                    max: "0.5",
                    step: "0.05",
                    value: "{goal.rate_pct_per_week}",
                    oninput: move |e| {
                        if let Ok(v) = e.value().parse::<f64>() {
                            state.goal.write().rate_pct_per_week = v;
                        }
                    },
                }
            }
        }
        section { class: "card",
            h2 { "Program" }
            Choice { label: "Diet",
                for (value , text) in [
                    (DietType::Balanced, "Balanced"),
                    (DietType::LowFat, "Low-fat"),
                    (DietType::LowCarb, "Low-carb"),
                    (DietType::Keto, "Keto"),
                ]
                {
                    button {
                        key: "{text}",
                        class: if program.diet == value { "active" },
                        onclick: move |_| state.program.write().diet = value,
                        "{text}"
                    }
                }
            }
            Choice { label: "Protein",
                for (value , text) in [
                    (ProteinPreference::Low, "Low"),
                    (ProteinPreference::Moderate, "Moderate"),
                    (ProteinPreference::High, "High"),
                    (ProteinPreference::ExtraHigh, "Extra high"),
                ]
                {
                    button {
                        key: "{text}",
                        class: if program.protein == value { "active" },
                        onclick: move |_| state.program.write().protein = value,
                        "{text}"
                    }
                }
            }
            Choice { label: "Calorie floor",
                for (value , text) in [
                    (CalorieFloor::Standard, "1200 kcal"),
                    (CalorieFloor::Low, "800 kcal"),
                    (CalorieFloor::None, "None"),
                ]
                {
                    button {
                        key: "{text}",
                        class: if program.floor == value { "active" },
                        onclick: move |_| state.program.write().floor = value,
                        "{text}"
                    }
                }
            }
            Choice { label: "Weekly calories",
                button {
                    class: if !shifting { "active" },
                    onclick: move |_| state.program.write().distribution = WeeklyDistribution::Even,
                    "Even"
                }
                button {
                    class: if shifting { "active" },
                    onclick: move |_| {
                        if !shifting {
                            state.program.write().distribution = WeeklyDistribution::Shift {
                                high_days: vec![Weekday::Sat, Weekday::Sun],
                            };
                        }
                    },
                    "Higher on some days"
                }
            }
            Choice { label: "Check-in day", even: true,
                for (day , letter) in WEEKDAYS {
                    button {
                        key: "{day}",
                        class: if check_in_day == day { "active" },
                        aria_label: "Check in on {day}",
                        onclick: move |_| state.check_in_day.set(day),
                        "{letter}"
                    }
                }
            }
            if shifting {
                div { class: "weekday-picker",
                    for (day , letter) in WEEKDAYS {
                        {
                            let on = high_days.contains(&day);
                            let days = high_days.clone();
                            rsx! {
                                button {
                                    key: "{day}",
                                    class: if on { "day-toggle active" } else { "day-toggle" },
                                    aria_label: "{day} higher calories",
                                    onclick: move |_| {
                                        let mut days = days.clone();
                                        if on {
                                            days.retain(|d| *d != day);
                                        } else {
                                            days.push(day);
                                        }
                                        state.program.write().distribution = if days.is_empty() {
                                            WeeklyDistribution::Even
                                        } else {
                                            WeeklyDistribution::Shift { high_days: days }
                                        };
                                    },
                                    "{letter}"
                                }
                            }
                        }
                    }
                }
            }
        }
        section { class: "card",
            h2 { "This week's targets" }
            div { class: "plan-bars",
                for (i , (_ , letter)) in WEEKDAYS.iter().enumerate() {
                    div { key: "{i}", class: "plan-col",
                        span { class: "plan-kcal", {fmt0(week[i].kcal)} }
                        div { class: "plan-stack", style: "height: {week[i].kcal / bar_max * 100.0}%",
                            div { class: "seg protein", style: "flex: {week[i].protein_g * 4.0}" }
                            div { class: "seg fat", style: "flex: {week[i].fat_g * 9.0}" }
                            div { class: "seg carbs", style: "flex: {week[i].carbs_g * 4.0}" }
                        }
                        span { class: "muted small", {format!("{}P {}F {}C", fmt0(week[i].protein_g), fmt0(week[i].fat_g), fmt0(week[i].carbs_g))} }
                        span { class: "muted", "{letter}" }
                    }
                }
            }
        }
        section { class: "card",
            h2 { "How these targets were set" }
            ol { class: "explain",
                li {
                    strong { "Expenditure: {fmt0(basis_kcal)} kcal/day. " }
                    if let Some((_, date)) = &locked {
                        "Set at your check-in on {date}. The current estimate is {fmt0(expenditure)} kcal/day, {status}."
                    } else {
                        "Estimated {status}."
                    }
                }
                li {
                    strong { "Average target: {fmt0(coach.average_target.kcal)} kcal/day. " }
                    "Your rate of {goal.rate_pct_per_week:+.2}% of body weight per week is about {weekly_kg:+.2} kg, or {daily_delta:+.0} kcal/day relative to expenditure.{floor_note}"
                }
                if shifting {
                    li {
                        strong { "Calorie shift. " }
                        "Selected days get {((HIGH_DAY_MULTIPLIER - 1.0) * 100.0):.0}% more than the others; the weekly total stays the same."
                    }
                }
                li {
                    strong { "Protein: {fmt0(week[0].protein_g)} g. " }
                    "{program.protein.grams_per_kg()} g per kg of trend weight."
                }
                li {
                    strong { "Fat and carbs. " }
                    "The remaining calories are split according to your diet type."
                }
            }
            if locked.is_none() {
                p { class: "muted small",
                    "Until your first check-in, targets follow the live estimate. A check-in fixes them for the week."
                }
            } else if live_change.abs() >= 1.0 {
                p { class: "muted small",
                    "Checking in today would change your average target by {live_change:+.0} kcal."
                }
            }
        }
    }
}

/// A labelled segmented control; `even` gives every option the same width.
#[component]
fn Choice(label: &'static str, #[props(default)] even: bool, children: Element) -> Element {
    rsx! {
        div { class: "choice",
            span { class: "choice-label", "{label}" }
            div { class: if even { "segmented even" } else { "segmented" }, {children} }
        }
    }
}
