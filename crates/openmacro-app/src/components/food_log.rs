//! The day's food as a timeline of clock hours.

use chrono::{Datelike, Duration, NaiveDate};
use dioxus::prelude::*;
use openmacro_core::expenditure::DayStatus;
use openmacro_core::nutrients::{Nutrient, NutrientProfile};

use super::{Icon, MacroStrip, Ring, hour_label, macro_line};
use crate::state::{AppState, CoachSnapshot, Copying, LogEntry, day_totals, today, week_start};

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
    let prefs = crate::prefs::PREFS();
    // The chosen range, stretched to include any entries outside it.
    let first_hour = entries
        .first()
        .map_or(prefs.log_first_hour, |e| e.hour.min(prefs.log_first_hour));
    let last_hour = entries
        .last()
        .map_or(prefs.log_last_hour, |e| e.hour.max(prefs.log_last_hour))
        .max(first_hour);
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
            for hour in first_hour..=last_hour {
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
    let hour_kcal: f64 = entries
        .iter()
        .map(|e| e.nutrients.get_or_zero(Nutrient::Energy))
        .sum();
    let show_total = crate::prefs::PREFS.read().hour_totals && !entries.is_empty();
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
                if show_total {
                    span { class: "hour-total", {crate::prefs::energy_text(hour_kcal)} }
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
                    for row in rows(entries) {
                        match row {
                            Row::Entry(entry) => rsx! {
                                EntryRow { key: "{entry.id}", entry }
                            },
                            Row::Meal(name, entries) => rsx! {
                                MealRow { key: "meal-{entries[0].id}", name, entries }
                            },
                        }
                    }
                }
            }
        }
    }
}

/// One row of an hour: an entry, or a meal of several entries logged
/// together (an AI-logged plate).
#[derive(Debug, Clone, PartialEq)]
enum Row {
    Entry(LogEntry),
    Meal(String, Vec<LogEntry>),
}

/// An hour's entries as rows, in order, each meal where its first entry is.
fn rows(entries: Vec<LogEntry>) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    for entry in &entries {
        let Some(meal) = &entry.meal else {
            out.push(Row::Entry(entry.clone()));
            continue;
        };
        if out
            .iter()
            .any(|r| matches!(r, Row::Meal(name, _) if name == meal))
        {
            continue;
        }
        let members: Vec<LogEntry> = entries
            .iter()
            .filter(|e| e.meal.as_ref() == Some(meal))
            .cloned()
            .collect();
        if members.len() > 1 {
            out.push(Row::Meal(meal.clone(), members));
        } else {
            out.push(Row::Entry(entry.clone()));
        }
    }
    out
}

/// A meal as one row; tapping it shows its foods, each editable.
#[component]
fn MealRow(name: String, entries: Vec<LogEntry>) -> Element {
    let state = use_context::<AppState>();
    let mut open = use_signal(|| false);
    let mut confirm_delete = use_signal(|| false);
    let totals: NutrientProfile = entries.iter().map(|e| &e.nutrients).sum();
    let kcal = crate::prefs::energy(totals.get_or_zero(Nutrient::Energy));
    let count = entries.len();
    let summary = format!("{count} items · {}", macro_line(&totals));
    let ids: Vec<i64> = entries.iter().map(|e| e.id).collect();
    rsx! {
        div { class: if open() { "meal open" } else { "meal" },
            div { class: "entry meal-head",
                button {
                    class: "entry-main entry-open",
                    aria_expanded: open(),
                    aria_label: if open() { "Hide the foods in {name}" } else { "Show the foods in {name}" },
                    onclick: move |_| open.set(!open()),
                    div { class: "entry-name", "{name}" }
                    div { class: "entry-sub", "{summary}" }
                }
                div { class: "entry-kcal", "{kcal}" }
                span { class: "meal-chevron", Icon { name: "right" } }
                button {
                    class: "icon-btn small",
                    aria_label: "Delete {name}",
                    onclick: move |_| confirm_delete.set(true),
                    Icon { name: "close" }
                }
            }
            if confirm_delete() {
                div { class: "discard-confirm", role: "alert",
                    span {
                        if count == 1 { "Delete this item?" } else { "Delete all {count} items?" }
                    }
                    button { onclick: move |_| confirm_delete.set(false), "Keep" }
                    button {
                        class: "danger",
                        onclick: move |_| {
                            confirm_delete.set(false);
                            for id in &ids {
                                state.delete_entry(*id);
                            }
                        },
                        "Delete"
                    }
                }
            }
            if open() {
                div { class: "meal-items",
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
    let kcal = crate::prefs::energy(entry.nutrients.get_or_zero(Nutrient::Energy));
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

#[cfg(test)]
mod tests {
    use openmacro_core::nutrients::NutrientProfile;

    use super::*;

    fn entry(id: i64, meal: Option<&str>) -> LogEntry {
        LogEntry {
            id,
            date: today(),
            hour: 12,
            name: format!("Food {id}"),
            amount: "100 g".into(),
            nutrients: NutrientProfile::new(),
            food_id: None,
            meal: meal.map(str::to_string),
        }
    }

    #[test]
    fn meals_group_where_their_first_entry_is() {
        let rows = rows(vec![
            entry(1, None),
            entry(2, Some("Curry")),
            entry(3, None),
            entry(4, Some("Curry")),
            entry(5, Some("Toast")),
        ]);
        let shape: Vec<String> = rows
            .iter()
            .map(|r| match r {
                Row::Entry(e) => e.id.to_string(),
                Row::Meal(name, entries) => format!(
                    "{name}{:?}",
                    entries.iter().map(|e| e.id).collect::<Vec<_>>()
                ),
            })
            .collect();
        // A meal of one shows as a plain entry.
        assert_eq!(shape, ["1", "Curry[2, 4]", "3", "5"]);
    }
}
