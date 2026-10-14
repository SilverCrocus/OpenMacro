//! Copying a meal or a whole day to another day.

use chrono::{Duration, NaiveDate};
use dioxus::prelude::*;
use openmacro_core::nutrients::Nutrient;

use super::{Icon, fmt0, hour_label};
use crate::back::use_back_handler;
use crate::state::{AppState, Copying, today};

#[component]
pub fn CopySheet(copying: Copying) -> Element {
    let mut state = use_context::<AppState>();
    use_back_handler(move |_| state.copying.set(None));
    let source_date = copying.entries.first().map_or_else(today, |e| e.date);
    let source_hour = copying.entries.first().map_or(12, |e| e.hour);
    // Copying today's food most likely means planning tomorrow.
    let suggested = if source_date == today() {
        today() + Duration::days(1)
    } else {
        today()
    };
    let mut date = use_signal(|| suggested);
    let mut hour = use_signal(|| source_hour);
    let kcal: f64 = copying
        .entries
        .iter()
        .map(|e| e.nutrients.get_or_zero(Nutrient::Energy))
        .sum();
    let count = copying.entries.len();
    let what = if copying.whole_day {
        format!("All of {}", source_date.format("%A %-d %B"))
    } else {
        format!(
            "{} at {}",
            source_date.format("%A %-d %B"),
            hour_label(source_hour)
        )
    };
    let names = copying
        .entries
        .iter()
        .map(|e| e.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let entries = copying.entries.clone();
    let whole_day = copying.whole_day;

    rsx! {
        div { class: "sheet-backdrop",
            div { class: "sheet sheet-short", role: "dialog", aria_label: "Copy food",
                header { class: "sheet-header",
                    button {
                        class: "icon-btn",
                        aria_label: "Close",
                        onclick: move |_| state.copying.set(None),
                        Icon { name: "close" }
                    }
                    div {
                        div { class: "checkin-title",
                            if count == 1 { "Copy 1 item" } else { "Copy {count} items" }
                        }
                        div { class: "muted small", "{what} · {fmt0(kcal)} kcal" }
                    }
                }
                div { class: "sheet-body",
                    p { class: "muted small copy-names", "{names}" }
                    div { class: "field-grid",
                        label { class: "field",
                            span { "To" }
                            input {
                                r#type: "date",
                                value: "{date}",
                                onchange: move |e| {
                                    if let Ok(d) = e.value().parse::<NaiveDate>() {
                                        date.set(d);
                                    }
                                },
                            }
                        }
                        if !whole_day {
                            label { class: "field",
                                span { "At" }
                                select {
                                    onchange: move |e| {
                                        if let Ok(h) = e.value().parse() {
                                            hour.set(h);
                                        }
                                    },
                                    for h in 0..24u32 {
                                        option { key: "{h}", value: "{h}", selected: h == hour(), {hour_label(h)} }
                                    }
                                }
                            }
                        }
                    }
                    if whole_day {
                        p { class: "muted small", "Each item keeps its time." }
                    }
                    div { class: "chip-row",
                        for (label , offset) in [("Today", 0), ("Tomorrow", 1)] {
                            button {
                                key: "{label}",
                                class: if date() == today() + Duration::days(offset) { "chip active" } else { "chip" },
                                onclick: move |_| date.set(today() + Duration::days(offset)),
                                "{label}"
                            }
                        }
                    }
                    button {
                        class: "primary wide",
                        onclick: move |_| {
                            let at = if whole_day { None } else { Some(hour()) };
                            state.copy_entries(&entries, date(), at);
                        },
                        "Copy"
                    }
                }
            }
        }
    }
}
