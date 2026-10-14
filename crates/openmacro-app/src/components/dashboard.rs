//! Summary of the week, expenditure, weight trend and habits.

use chrono::Duration;
use dioxus::prelude::*;
use openmacro_core::expenditure::EstimateStatus;
use openmacro_core::nutrients::Nutrient;

use super::{Ring, Sparkline, fmt0, fmt1};
use crate::state::{AppState, CoachSnapshot, day_totals, today, week_start};

#[component]
pub fn Dashboard() -> Element {
    let state = use_context::<AppState>();
    let coach = use_context::<Memo<CoachSnapshot>>();
    let coach = coach.read();
    let today = today();
    let log = state.log.read();
    let weigh_ins = state.weigh_ins.read();

    // This week, consumed vs target.
    let start = week_start(today);
    let week: Vec<(String, f64, f64, bool)> = (0..7)
        .map(|i| {
            let date = start + Duration::days(i);
            let eaten = day_totals(&log, date).get_or_zero(Nutrient::Energy);
            (
                date.format("%a").to_string(),
                eaten,
                coach.target_for(date).kcal,
                date == today,
            )
        })
        .collect();
    let bar_max = week
        .iter()
        .map(|(_, e, t, _)| e.max(*t))
        .fold(1.0, f64::max);

    let today_kcal = day_totals(&log, today).get_or_zero(Nutrient::Energy);
    let today_target = coach.target_for(today).kcal;

    // Expenditure tile.
    let expenditure = coach.expenditure_kcal();
    let status = match coach.expenditure_status() {
        EstimateStatus::Initial => "Initial estimate",
        EstimateStatus::Holding => "Holding",
        EstimateStatus::Updated => "Updating",
    };
    let exp_series: Vec<f64> = coach
        .expenditure
        .iter()
        .rev()
        .take(30)
        .rev()
        .map(|p| p.kcal)
        .collect();

    // Weight trend tile.
    let trend_now = coach.trend_kg();
    let trend_week_ago = coach.trend.iter().rev().nth(7).map(|t| t.trend_kg);
    let trend_change = trend_now.zip(trend_week_ago).map(|(a, b)| a - b);
    let trend_series: Vec<f64> = coach
        .trend
        .iter()
        .rev()
        .take(30)
        .rev()
        .map(|t| t.trend_kg)
        .collect();

    // Energy balance over the last 7 complete days that were logged.
    let logged: Vec<f64> = (1..=7)
        .map(|i| day_totals(&log, today - Duration::days(i)).get_or_zero(Nutrient::Energy))
        .filter(|k| *k > 0.0)
        .collect();
    let balance = (!logged.is_empty())
        .then(|| logged.iter().sum::<f64>() / logged.len() as f64 - expenditure);

    // Habits this week.
    let weighed_days = (0..7)
        .filter(|i| {
            let d = start + Duration::days(*i);
            d <= today && weigh_ins.iter().any(|w| w.date == d)
        })
        .count();
    let logged_days = (0..7)
        .filter(|i| {
            let d = start + Duration::days(*i);
            d <= today && log.iter().any(|e| e.date == d)
        })
        .count();

    let today_text = format!("{} / {}", fmt0(today_kcal), fmt0(today_target));
    let trend_text = trend_now.map_or("—".into(), fmt1);
    let trend_change_text = trend_change.map_or("Not enough data".into(), |c| {
        format!("{c:+.2} kg this week")
    });
    let balance_text = balance.map_or("—".into(), |b| format!("{b:+.0}"));

    let empty = state.is_empty();
    drop((log, weigh_ins));

    rsx! {
        header { class: "page-header",
            h1 { "Dashboard" }
        }
        if empty {
            section { class: "card welcome",
                h2 { "Welcome to OpenMacro" }
                p { class: "muted",
                    "Log what you eat with the + button and weigh in when you can. After a few days your expenditure estimate starts adapting to you."
                }
                p { class: "muted", "Set your age, height and weight under More so the starting estimate fits you." }
                button {
                    class: "primary",
                    onclick: move |_| state.replace_data(crate::seed::demo(today)),
                    "Explore with demo data"
                }
            }
        }
        section { class: "card hero",
            Ring { fraction: today_kcal / today_target.max(1.0), size: 88 }
            div {
                div { class: "muted", "Today" }
                div { class: "hero-num", "{today_text}" }
                div { class: "muted", "kcal eaten / target" }
            }
        }
        section { class: "card",
            h2 { "This week" }
            div { class: "week-bars",
                for (label , eaten , target , is_today) in week {
                    div { key: "{label}", class: if is_today { "week-col today" } else { "week-col" },
                        div { class: "week-bar-area",
                            div { class: "week-target", style: "bottom: {target / bar_max * 100.0}%" }
                            div { class: "week-bar", style: "height: {eaten / bar_max * 100.0}%" }
                        }
                        span { class: "muted", "{label}" }
                    }
                }
            }
        }
        div { class: "tiles",
            section { class: "card tile",
                h3 { "Expenditure" }
                div { class: "tile-num", {fmt0(expenditure)} span { class: "unit", " kcal" } }
                div { class: "muted", "{status}" }
                Sparkline { values: exp_series, class: "exp" }
            }
            section { class: "card tile",
                h3 { "Weight trend" }
                div { class: "tile-num", "{trend_text}" span { class: "unit", " kg" } }
                div { class: "muted", "{trend_change_text}" }
                Sparkline { values: trend_series, class: "trend" }
            }
            section { class: "card tile",
                h3 { "Energy balance" }
                div { class: "tile-num", "{balance_text}" span { class: "unit", " kcal/day" } }
                div { class: "muted", "Last 7 days, intake − expenditure" }
            }
            section { class: "card tile",
                h3 { "Habits" }
                div { class: "habit", span { "Weigh-ins" } span { class: "num", "{weighed_days}/7" } }
                div { class: "habit", span { "Days logged" } span { class: "num", "{logged_days}/7" } }
            }
        }
        WeighInCard {}
    }
}

#[component]
fn WeighInCard() -> Element {
    let state = use_context::<AppState>();
    let mut input = use_signal(String::new);
    let mut saved = use_signal(|| false);
    let value = input()
        .trim()
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|v| (20.0..400.0).contains(v));
    rsx! {
        section { class: "card",
            h2 { "Weigh in" }
            div { class: "amount-row",
                input {
                    class: "qty",
                    r#type: "text",
                    "inputmode": "decimal",
                    placeholder: "kg",
                    aria_label: "Weight in kilograms",
                    value: "{input}",
                    oninput: move |e| {
                        input.set(e.value());
                        saved.set(false);
                    },
                }
                button {
                    class: "primary",
                    disabled: value.is_none(),
                    onclick: move |_| {
                        if let Some(kg) = value {
                            state.add_weigh_in(today(), kg);
                            input.set(String::new());
                            saved.set(true);
                        }
                    },
                    "Save"
                }
            }
            if saved() {
                p { class: "muted pad", "Saved. Your trend and expenditure have been updated." }
            }
        }
    }
}
