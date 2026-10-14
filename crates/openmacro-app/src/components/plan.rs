//! Goal and coached program settings, and the resulting weekly targets.

use chrono::Weekday;
use dioxus::prelude::*;
use openmacro_core::KCAL_PER_KG_BODY_MASS;
use openmacro_core::expenditure::EstimateStatus;
use openmacro_core::log::{MAINTAIN_BAND, MAINTAIN_CORRECTION_PCT};
use openmacro_core::nutrients::{KCAL_PER_G_CARBS, KCAL_PER_G_FAT, KCAL_PER_G_PROTEIN};
use openmacro_core::targets::{
    CalorieFloor, DietType, HIGH_DAY_MULTIPLIER, MacroTargets, ProteinPreference,
    WeeklyDistribution,
};

use super::{fmt0, fmt1};
use crate::prefs;
use crate::state::{AppState, CoachSnapshot};
use openmacro_core::trend::TrendSmoothing;

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
    // Maintaining can still plan a gentle correction back towards the goal.
    let rate = goal.effective_rate(weight);
    let weekly_kg = weight * rate / 100.0;
    let daily_delta = weekly_kg * KCAL_PER_KG_BODY_MASS / 7.0;
    let to_go = goal.goal_weight_kg - weight;
    let eta = if weekly_kg.abs() < 1e-6 || to_go.signum() != weekly_kg.signum() {
        "—".to_string()
    } else {
        format!("{:.0} weeks", (to_go / weekly_kg).abs().ceil())
    };
    let unit = prefs::weight_symbol();
    let energy_unit = prefs::energy_symbol();
    let rate_text = format!(
        "{:+.2}% / week ({:+.2} {unit})",
        rate,
        prefs::weight_value(weekly_kg)
    );
    let high_days = match &program.distribution {
        WeeklyDistribution::Even => Vec::new(),
        WeeklyDistribution::Shift { high_days } => high_days.clone(),
    };
    let shifting = !high_days.is_empty();
    let week = coach.week;
    let manual = state.manual_targets.read().is_some();
    // Switching to manual starts from the current targets' daily average.
    let coach_week_average = {
        let avg = |f: fn(&MacroTargets) -> f64| (week.iter().map(f).sum::<f64>() / 7.0).round();
        MacroTargets {
            kcal: avg(|t| t.kcal),
            protein_g: avg(|t| t.protein_g),
            fat_g: avg(|t| t.fat_g),
            carbs_g: avg(|t| t.carbs_g),
        }
    };
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
    let fixed = state.coach_settings.read().fixed_expenditure_kcal;
    let basis_kcal =
        fixed.unwrap_or_else(|| locked.as_ref().map_or(expenditure, |(kcal, _)| *kcal));
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
                    span { class: "num", {prefs::weight(weight, 1)} }
                    span { class: "muted", "Trend {unit}" }
                }
                div { class: "stat",
                    span { class: "num", {prefs::weight(goal.goal_weight_kg, 1)} }
                    span { class: "muted", "Goal {unit}" }
                }
                div { class: "stat",
                    span { class: "num", "{eta}" }
                    span { class: "muted", "Estimated" }
                }
            }
            label { class: "field",
                span { "Goal weight ({unit})" }
                input {
                    r#type: "number",
                    step: "0.1",
                    value: "{prefs::weight(goal.goal_weight_kg, 1)}",
                    onchange: move |e| {
                        if let Ok(v) = e.value().parse::<f64>() {
                            state.goal.write().goal_weight_kg = prefs::kg_from_input(v);
                        }
                    },
                }
            }
            Choice { label: "Goal", even: true,
                button {
                    class: if goal.rate_pct_per_week < 0.0 { "active" },
                    onclick: move |_| {
                        if goal.rate_pct_per_week >= 0.0 {
                            state.goal.write().rate_pct_per_week = -0.5;
                        }
                    },
                    "Lose"
                }
                button {
                    class: if goal.is_maintain() { "active" },
                    onclick: move |_| state.goal.write().rate_pct_per_week = 0.0,
                    "Maintain"
                }
                button {
                    class: if goal.rate_pct_per_week > 0.0 { "active" },
                    onclick: move |_| {
                        if goal.rate_pct_per_week <= 0.0 {
                            state.goal.write().rate_pct_per_week = 0.25;
                        }
                    },
                    "Gain"
                }
            }
            if goal.is_maintain() {
                p { class: "muted small",
                    "Your targets hold your weight within {prefs::weight_text(goal.goal_weight_kg * MAINTAIN_BAND, 1)} of your goal. If your trend drifts further, they correct gently by {MAINTAIN_CORRECTION_PCT}% of body weight a week."
                }
            } else {
                label { class: "field",
                    span { "Rate: {rate_text}" }
                    input {
                        r#type: "range",
                        min: if goal.rate_pct_per_week < 0.0 { "-1" } else { "0.05" },
                        max: if goal.rate_pct_per_week < 0.0 { "-0.05" } else { "0.5" },
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
        }
        section { class: "card",
            h2 { "Program" }
            Choice { label: "Style",
                button {
                    class: if !manual { "active" },
                    onclick: move |_| state.manual_targets.set(None),
                    "Coached"
                }
                button {
                    class: if manual { "active" },
                    onclick: move |_| {
                        if !manual {
                            state.manual_targets.set(Some(coach_week_average));
                        }
                    },
                    "Manual"
                }
            }
            if manual {
                ManualTargets {}
            } else {
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
                        (CalorieFloor::Standard, prefs::energy_text(1200.0)),
                        (CalorieFloor::Low, prefs::energy_text(800.0)),
                        (CalorieFloor::None, "None".to_string()),
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
        }
        EstimateSettings {}
        section { class: "card",
            h2 { "This week's targets" }
            div { class: "plan-bars",
                for (i , (_ , letter)) in WEEKDAYS.iter().enumerate() {
                    div { key: "{i}", class: "plan-col",
                        span { class: "plan-kcal", {prefs::energy(week[i].kcal)} }
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
        if manual {
            section { class: "card",
                h2 { "How these targets were set" }
                p { class: "muted",
                    "You're on a manual program, so these are the targets you entered, every day. Your expenditure estimate is {prefs::energy(expenditure)} {energy_unit}/day ({status}), if you want to compare."
                }
            }
        } else {
            section { class: "card",
                h2 { "How these targets were set" }
                ol { class: "explain",
                    li {
                        strong { "Expenditure: {prefs::energy(basis_kcal)} {energy_unit}/day. " }
                        if fixed.is_some() {
                            "Fixed in Estimates below. The adaptive estimate is {prefs::energy(expenditure)} {energy_unit}/day, {status}."
                        } else if let Some((_, date)) = &locked {
                            "Set at your check-in on {date}. The current estimate is {prefs::energy(expenditure)} {energy_unit}/day, {status}."
                        } else {
                            "Estimated {status}."
                        }
                    }
                    li {
                        strong { "Average target: {prefs::energy(coach.average_target.kcal)} {energy_unit}/day. " }
                        "Your rate of {rate:+.2}% of body weight per week is about {prefs::weight_value(weekly_kg):+.2} {unit}, or {prefs::energy_value(daily_delta):+.0} {energy_unit}/day relative to expenditure.{floor_note}"
                    }
                    if shifting {
                        li {
                            strong { "Calorie shift. " }
                            "Selected days get {((HIGH_DAY_MULTIPLIER - 1.0) * 100.0):.0}% more than the others; the weekly total stays the same."
                        }
                    }
                    li {
                        strong { "Protein: {fmt0(week[0].protein_g)} g. " }
                        "{program.protein.grams_per_kg()} g per kg of trend weight (about {fmt1(program.protein.grams_per_kg() * openmacro_core::units::KG_PER_LB)} g per lb)."
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
                        "Checking in today would change your average target by {prefs::energy_value(live_change):+.0} {energy_unit}."
                    }
                }
            }
        }
    }
}

/// Fixed or adaptive expenditure, and how fast the weight trend reacts.
#[component]
fn EstimateSettings() -> Element {
    let mut state = use_context::<AppState>();
    let coach = use_context::<Memo<CoachSnapshot>>();
    let settings = (state.coach_settings)();
    let estimate = coach.read().expenditure_kcal();
    let fixed = settings.fixed_expenditure_kcal;

    rsx! {
        section { class: "card",
            h2 { "Estimates" }
            Choice { label: "Expenditure", even: true,
                button {
                    class: if fixed.is_none() { "active" },
                    onclick: move |_| state.coach_settings.write().fixed_expenditure_kcal = None,
                    "Adaptive"
                }
                button {
                    class: if fixed.is_some() { "active" },
                    onclick: move |_| {
                        if fixed.is_none() {
                            state.coach_settings.write().fixed_expenditure_kcal = Some(estimate.round());
                        }
                    },
                    "Fixed"
                }
            }
            if let Some(kcal) = fixed {
                label { class: "field",
                    span { "Fixed expenditure ({prefs::energy_symbol()}/day)" }
                    input {
                        r#type: "text",
                        "inputmode": "decimal",
                        value: "{prefs::energy(kcal)}",
                        onchange: move |e| {
                            if let Ok(v) = e.value().trim().replace(',', ".").parse::<f64>() {
                                if v > 0.0 {
                                    state.coach_settings.write().fixed_expenditure_kcal = Some(prefs::kcal_from_input(v));
                                }
                            }
                        },
                    }
                }
                p { class: "muted small",
                    "Targets use this instead of the estimate, which keeps updating ({prefs::energy_text(estimate)}/day now) for comparison."
                }
            } else {
                p { class: "muted small",
                    "Your expenditure adapts to what you eat and how your weight changes."
                }
            }
            Choice { label: "Weight trend", even: true,
                for smoothing in TrendSmoothing::ALL {
                    button {
                        key: "{smoothing.label()}",
                        class: if settings.smoothing == smoothing { "active" },
                        onclick: move |_| state.coach_settings.write().smoothing = smoothing,
                        "{smoothing.label()}"
                    }
                }
            }
            p { class: "muted small",
                "Faster follows the scale more closely; slower smooths out more day-to-day noise."
            }
        }
    }
}

/// Picks one value out of a set of targets, to edit it.
type TargetField = fn(&mut MacroTargets) -> &mut f64;

/// The manual program: calories and macros the user sets, used every day.
#[component]
fn ManualTargets() -> Element {
    let mut state = use_context::<AppState>();
    let Some(targets) = *state.manual_targets.read() else {
        return rsx! {};
    };
    let from_macros = targets.protein_g * KCAL_PER_G_PROTEIN
        + targets.fat_g * KCAL_PER_G_FAT
        + targets.carbs_g * KCAL_PER_G_CARBS;
    let mut set = move |field: TargetField, energy: bool, text: String| {
        if let Ok(v) = text.trim().replace(',', ".").parse::<f64>() {
            if v >= 0.0 {
                if let Some(t) = state.manual_targets.write().as_mut() {
                    *field(t) = if energy { prefs::kcal_from_input(v) } else { v };
                }
            }
        }
    };
    let fields: [(&str, &str, f64, TargetField); 4] = [
        (
            "Energy",
            prefs::energy_symbol(),
            prefs::energy_value(targets.kcal),
            |t| &mut t.kcal,
        ),
        ("Protein", "g", targets.protein_g, |t| &mut t.protein_g),
        ("Fat", "g", targets.fat_g, |t| &mut t.fat_g),
        ("Carbs", "g", targets.carbs_g, |t| &mut t.carbs_g),
    ];

    rsx! {
        p { class: "muted small",
            "Your own targets, the same every day. Check-ins still update your expenditure estimate but leave these alone."
        }
        div { class: "field-grid",
            for (label , unit , value , field) in fields {
                label { key: "{label}", class: "field",
                    span { "{label} ({unit})" }
                    input {
                        r#type: "text",
                        "inputmode": "decimal",
                        value: "{fmt0(value)}",
                        onchange: move |e| set(field, label == "Energy", e.value()),
                    }
                }
            }
        }
        if (from_macros - targets.kcal).abs() > 50.0 {
            p { class: "muted small",
                "Your macros add up to {prefs::energy_text(from_macros)}, {prefs::energy_text((from_macros - targets.kcal).abs())} "
                if from_macros > targets.kcal { "more" } else { "less" }
                " than your calorie target."
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
