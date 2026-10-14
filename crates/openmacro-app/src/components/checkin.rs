//! The weekly check-in: when it's due, and the three-step flow that reviews
//! the week and locks in new targets.

use std::collections::BTreeSet;

use chrono::{Duration, NaiveDate};
use dioxus::prelude::*;
use openmacro_core::checkin::{self, CheckIn, Suggestion};
use openmacro_core::expenditure::DayStatus;
use openmacro_core::nutrients::Nutrient;
use openmacro_core::targets::MacroTargets;

use super::{Icon, fmt0, fmt1};
use crate::state::{AppState, CoachSnapshot, day_totals, today};

fn due(state: &AppState, coach: &CoachSnapshot) -> NaiveDate {
    let last = coach.last_check_in.as_ref().map(|c| c.date);
    checkin::due_date(today(), (state.check_in_day)(), last)
}

/// Shows when the next check-in is; `only_when_due` hides it until then.
#[component]
pub fn CheckInCard(only_when_due: bool) -> Element {
    let mut state = use_context::<AppState>();
    let coach = use_context::<Memo<CoachSnapshot>>();
    let coach = coach.read();
    let due_date = due(&state, &coach);
    let days = (due_date - today()).num_days();
    if only_when_due && days > 0 {
        return rsx! {};
    }
    let (title, detail) = if days <= 0 {
        (
            "Your weekly check-in is ready".to_string(),
            "Review your week and update your targets.".to_string(),
        )
    } else {
        let when = if days == 1 {
            "tomorrow".to_string()
        } else {
            format!("in {days} days")
        };
        (
            format!("Next check-in {when}"),
            due_date.format("%A %-d %B").to_string(),
        )
    };
    let last = coach
        .last_check_in
        .as_ref()
        .map(|c| format!("Last check-in {}", c.date.format("%-d %b")));

    rsx! {
        section { class: if days <= 0 { "card checkin-card due" } else { "card checkin-card" },
            div {
                h2 { "{title}" }
                p { class: "muted small", "{detail}" }
                if let Some(last) = last {
                    p { class: "muted small", "{last}" }
                }
            }
            button {
                class: if days <= 0 { "primary" },
                onclick: move |_| state.check_in_open.set(true),
                if days <= 0 { "Check in" } else { "Check in early" }
            }
        }
    }
}

#[component]
pub fn CheckInSheet() -> Element {
    let mut state = use_context::<AppState>();
    let mut step = use_signal(|| 0usize);
    let titles = ["Weigh in", "Review your week", "Your new targets"];
    let current = step();

    rsx! {
        div { class: "sheet-backdrop",
            div { class: "sheet", role: "dialog", aria_label: "Weekly check-in",
                header { class: "sheet-header",
                    button {
                        class: "icon-btn",
                        aria_label: "Close check-in",
                        onclick: move |_| state.check_in_open.set(false),
                        Icon { name: "close" }
                    }
                    div {
                        div { class: "checkin-title", "{titles[current]}" }
                        div { class: "muted small", "Weekly check-in · step {current + 1} of 3" }
                    }
                }
                div { class: "checkin-progress",
                    div { class: "checkin-progress-fill", style: "width: {(current + 1) * 100 / 3}%" }
                }
                div { class: "sheet-body",
                    match current {
                        0 => rsx! {
                            WeighInStep { on_next: move |_| step.set(1) }
                        },
                        1 => rsx! {
                            ReviewStep { on_next: move |_| step.set(2) }
                        },
                        _ => rsx! {
                            UpdateStep {}
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn WeighInStep(on_next: EventHandler<()>) -> Element {
    let state = use_context::<AppState>();
    let mut weight = use_signal(String::new);
    let last = state
        .weigh_ins
        .read()
        .iter()
        .max_by_key(|w| w.date)
        .copied();
    let value = weight()
        .trim()
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|v| (20.0..400.0).contains(v));
    let weighed_today = last.is_some_and(|w| w.date == today());

    rsx! {
        p { class: "muted",
            "Your expenditure estimate needs at least one weigh-in a week. Weighing in most days, at the same time, gives the smoothest trend."
        }
        if let Some(last) = last {
            p { class: "muted small", "Last weigh-in: {fmt1(last.kg)} kg on {last.date.format(\"%a %-d %b\")}" }
        }
        if weighed_today {
            p { "You've already weighed in today." }
            button { class: "primary wide", onclick: move |_| on_next.call(()), "Continue" }
        } else {
            label { class: "field",
                span { "Today's weight (kg)" }
                input {
                    r#type: "text",
                    "inputmode": "decimal",
                    placeholder: "kg",
                    oninput: move |e| weight.set(e.value()),
                }
            }
            div { class: "button-row form-actions",
                button { onclick: move |_| on_next.call(()), "Skip" }
                button {
                    class: "primary",
                    disabled: value.is_none(),
                    onclick: move |_| {
                        if let Some(kg) = value {
                            state.add_weigh_in(today(), kg);
                            on_next.call(());
                        }
                    },
                    "Save and continue"
                }
            }
        }
    }
}

#[component]
fn ReviewStep(on_next: EventHandler<()>) -> Element {
    let state = use_context::<AppState>();
    let coach = use_context::<Memo<CoachSnapshot>>();
    let mut chosen = use_signal(BTreeSet::<NaiveDate>::new);
    let today = today();

    // Review the days since the last check-in (at most two weeks), up to yesterday.
    let reviews = {
        let coach = coach.read();
        let log = state.log.read();
        let statuses = state.day_status.read();
        let from = coach
            .last_check_in
            .as_ref()
            .map_or(today - Duration::days(7), |c| c.date)
            .max(today - Duration::days(14));
        let first_logged = log.iter().map(|e| e.date).min();
        // Before anything was logged there's nothing to review.
        let from = first_logged.map_or(today, |first| from.max(first));
        checkin::days_to_review(from, today - Duration::days(1), |date| {
            let logged = day_totals(&log, date)
                .get(Nutrient::Energy)
                .filter(|k| *k > 0.0);
            let status = statuses.get(&date).copied().unwrap_or_default();
            (logged, status, coach.target_for(date).kcal)
        })
    };

    let to_apply = reviews.clone();

    rsx! {
        if reviews.is_empty() {
            p { "Your logging looks complete. Nothing to review." }
        } else {
            p { class: "muted",
                "Days with nothing logged are left out of your expenditure estimate unless they were fasts. Days logged only partly would make it look too low. Tick any that apply."
            }
            ul { class: "review-list",
                for review in reviews.iter().copied() {
                    {
                        let selected = chosen().contains(&review.date);
                        let (question, detail) = match review.suggestion {
                            Suggestion::MaybeFasted => ("I fasted this day", "Nothing logged".to_string()),
                            Suggestion::MaybePartial => (
                                "I only logged part of this day",
                                format!("{} kcal logged", fmt0(review.logged_kcal.unwrap_or(0.0))),
                            ),
                        };
                        rsx! {
                            li { key: "{review.date}",
                                button {
                                    class: if selected { "review-day selected" } else { "review-day" },
                                    role: "checkbox",
                                    aria_checked: "{selected}",
                                    onclick: move |_| {
                                        let mut set = chosen.write();
                                        if !set.remove(&review.date) {
                                            set.insert(review.date);
                                        }
                                    },
                                    span { class: "review-check", if selected { "✓" } }
                                    span { class: "review-text",
                                        strong { {review.date.format("%A %-d %b").to_string()} }
                                        span { class: "muted small", " · {detail}" }
                                        br {}
                                        span { "{question}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        button {
            class: "primary wide",
            onclick: move |_| {
                for review in &to_apply {
                    if chosen().contains(&review.date) {
                        let status = match review.suggestion {
                            Suggestion::MaybeFasted => DayStatus::Fasted,
                            Suggestion::MaybePartial => DayStatus::Partial,
                        };
                        state.set_day_status(review.date, status);
                    }
                }
                on_next.call(());
            },
            "Continue"
        }
    }
}

#[component]
fn UpdateStep() -> Element {
    let state = use_context::<AppState>();
    let coach = use_context::<Memo<CoachSnapshot>>();
    let coach = coach.read();
    let new_kcal = coach.expenditure_kcal();
    let old_kcal = coach.last_check_in.as_ref().map(|c| c.expenditure_kcal);
    let new_target = coach.live_target.kcal;
    let old_target = coach.average_target.kcal;
    let week = coach.live_week;
    let check_in = CheckIn {
        date: today(),
        expenditure_kcal: new_kcal,
        trend_kg: coach.trend_kg(),
        week: week.to_vec(),
    };
    let change = |old: f64, new: f64| {
        let diff = new - old;
        if diff.abs() < 1.0 {
            "no change".to_string()
        } else {
            format!("{diff:+.0} kcal")
        }
    };
    let expenditure_line = match old_kcal {
        Some(old) => format!(
            "{} → {} kcal ({})",
            fmt0(old),
            fmt0(new_kcal),
            change(old, new_kcal)
        ),
        None => format!("{} kcal", fmt0(new_kcal)),
    };
    // Before the first check-in the active target already follows the live estimate.
    let target_line = match old_kcal {
        Some(_) => format!(
            "{} → {} kcal ({})",
            fmt0(old_target),
            fmt0(new_target),
            change(old_target, new_target)
        ),
        None => format!("{} kcal", fmt0(new_target)),
    };
    let average = |f: fn(&MacroTargets) -> f64| fmt0(week.iter().map(f).sum::<f64>() / 7.0);
    let macros = [
        ("Protein", average(|d| d.protein_g)),
        ("Fat", average(|d| d.fat_g)),
        ("Carbs", average(|d| d.carbs_g)),
    ];

    rsx! {
        div { class: "update-summary",
            div { class: "update-row",
                span { class: "muted", "Expenditure" }
                strong { "{expenditure_line}" }
            }
            div { class: "update-row",
                span { class: "muted", "Average daily target" }
                strong { "{target_line}" }
            }
            for (name , grams) in macros {
                div { key: "{name}", class: "update-row",
                    span { class: "muted", "{name}" }
                    strong { "{grams} g" }
                }
            }
        }
        h3 { class: "section-title", "This week" }
        div { class: "checkin-week",
            for (i , day) in ["M", "T", "W", "T", "F", "S", "S"].iter().enumerate() {
                div { key: "{i}", class: "checkin-day",
                    span { class: "muted small", "{day}" }
                    strong { {fmt0(week[i].kcal)} }
                }
            }
        }
        p { class: "muted small", "These targets stay fixed until your next check-in. Changing your goal or program updates them straight away." }
        button { class: "primary wide", onclick: move |_| state.add_check_in(check_in.clone()), "Apply new targets" }
    }
}
