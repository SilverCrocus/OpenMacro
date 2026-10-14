//! The day's food as a timeline of clock hours.

use chrono::{Datelike, Duration, NaiveDate};
use dioxus::prelude::*;
use openmacro_core::expenditure::DayStatus;
use openmacro_core::nutrients::Nutrient;

use super::{Icon, MacroStrip, Ring, fmt0, hour_label, macro_line};
use crate::state::{AppState, CoachSnapshot, Copying, LogEntry, day_totals, today, week_start};

const FIRST_HOUR: u32 = 6;
const LAST_HOUR: u32 = 23;

#[component]
pub fn FoodLog() -> Element {
    let mut state = use_context::<AppState>();
    let coach = use_context::<Memo<CoachSnapshot>>();
    let coach = coach.read();
    let selected = (state.selected_date)();
    let start = week_start(selected);
    let log = state.log.read().clone();
    let status = state
        .day_status
        .read()
        .get(&selected)
        .copied()
        .unwrap_or_default();

    let week: Vec<(NaiveDate, f64)> = (0..7)
        .map(|i| {
            let date = start + Duration::days(i);
            let eaten = day_totals(&log, date).get_or_zero(Nutrient::Energy);
            (date, eaten / coach.target_for(date).kcal.max(1.0))
        })
        .collect();
    let totals = day_totals(&log, selected);
    let target = coach.target_for(selected);

    let mut entries: Vec<LogEntry> = log.into_iter().filter(|e| e.date == selected).collect();
    entries.sort_by_key(|e| (e.hour, e.id));
    let first_hour = entries
        .first()
        .map_or(FIRST_HOUR, |e| e.hour.min(FIRST_HOUR));
    let title = day_title(selected);

    rsx! {
        div { class: "sticky-head",
            header { class: "page-header",
                h1 { "{title}" }
                div { class: "header-actions",
                    button {
                        class: "icon-btn",
                        aria_label: "Previous week",
                        onclick: move |_| state.selected_date.set(selected - Duration::days(7)),
                        Icon { name: "left" }
                    }
                    button { class: "chip", onclick: move |_| state.selected_date.set(today()), "Today" }
                    button {
                        class: "icon-btn",
                        aria_label: "Next week",
                        onclick: move |_| state.selected_date.set(selected + Duration::days(7)),
                        Icon { name: "right" }
                    }
                }
            }
            div { class: "week-strip",
                for (date , fraction) in week {
                    button {
                        key: "{date}",
                        class: if date == selected { "day-pill selected" } else { "day-pill" },
                        aria_label: date.format("%A %-d %B").to_string(),
                        onclick: move |_| state.selected_date.set(date),
                        span { class: "dow", {date.format("%a").to_string()} }
                        Ring { fraction, size: 30 }
                        span { class: "dom", "{date.day()}" }
                    }
                }
            }
            MacroStrip { totals, target }
        }
        div { class: "day-status",
            span { class: "muted", "Day is" }
            for (value , label) in [
                (DayStatus::Normal, "Logged normally"),
                (DayStatus::Partial, "Partially logged"),
                (DayStatus::Fasted, "Fasted"),
            ]
            {
                button {
                    key: "{label}",
                    class: if status == value { "chip active" } else { "chip" },
                    onclick: move |_| state.set_day_status(selected, value),
                    "{label}"
                }
            }
            if !entries.is_empty() {
                {
                    let day = entries.clone();
                    rsx! {
                        button {
                            class: "chip copy-day",
                            onclick: move |_| {
                                state
                                    .copying
                                    .set(
                                        Some(Copying {
                                            entries: day.clone(),
                                            whole_day: true,
                                        }),
                                    )
                            },
                            Icon { name: "copy" }
                            "Copy day"
                        }
                    }
                }
            }
        }
        div { class: "timeline",
            for hour in first_hour..=LAST_HOUR {
                HourRow {
                    key: "{hour}",
                    date: selected,
                    hour,
                    entries: entries.iter().filter(|e| e.hour == hour).cloned().collect::<Vec<_>>(),
                }
            }
        }
    }
}

fn day_title(date: NaiveDate) -> String {
    let today = today();
    if date == today {
        "Today".into()
    } else if date == today - Duration::days(1) {
        "Yesterday".into()
    } else if date == today + Duration::days(1) {
        "Tomorrow".into()
    } else {
        date.format("%a %-d %b").to_string()
    }
}

#[component]
fn HourRow(date: NaiveDate, hour: u32, entries: Vec<LogEntry>) -> Element {
    let mut state = use_context::<AppState>();
    let meal = entries.clone();
    let label = hour_label(hour);
    let class = if entries.is_empty() {
        "hour-row"
    } else {
        "hour-row filled"
    };
    rsx! {
        div { class,
            div { class: "hour-head",
                span { class: "hour-label", "{label}" }
                button {
                    class: "hour-add",
                    aria_label: "Add food at {label}",
                    onclick: move |_| state.open_plate(date, hour),
                    Icon { name: "plus" }
                }
                if !meal.is_empty() {
                    button {
                        class: "hour-add hour-copy",
                        aria_label: "Copy food at {label}",
                        onclick: move |_| {
                            state
                                .copying
                                .set(
                                    Some(Copying {
                                        entries: meal.clone(),
                                        whole_day: false,
                                    }),
                                )
                        },
                        Icon { name: "copy" }
                    }
                }
            }
            if class != "hour-row" {
                div { class: "hour-entries",
                    for entry in entries {
                        EntryRow { key: "{entry.id}", entry }
                    }
                }
            }
        }
    }
}

#[component]
fn EntryRow(entry: LogEntry) -> Element {
    let mut state = use_context::<AppState>();
    let id = entry.id;
    let open = entry.clone();
    let kcal = fmt0(entry.nutrients.get_or_zero(Nutrient::Energy));
    let macros = macro_line(&entry.nutrients);
    rsx! {
        div { class: "entry",
            button {
                class: "entry-main entry-open",
                aria_label: "Edit {entry.name}",
                onclick: move |_| state.editing.set(Some(open.clone())),
                div { class: "entry-name", "{entry.name}" }
                div { class: "entry-sub", "{entry.amount} · {macros}" }
            }
            div { class: "entry-kcal", "{kcal}" }
            button {
                class: "icon-btn small",
                aria_label: "Delete {entry.name}",
                onclick: move |_| state.delete_entry(id),
                Icon { name: "close" }
            }
        }
    }
}
