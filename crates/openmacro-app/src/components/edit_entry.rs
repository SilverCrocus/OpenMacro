//! Editing something already logged: its amount and time.

use dioxus::prelude::*;
use openmacro_core::food::parse_quantity;
use openmacro_core::nutrients::Nutrient;

use super::logger::{FoodDetail, NumberField};
use super::{Icon, hour_label};
use crate::back::use_back_handler;
use crate::food_search::{self, Database};
use crate::foods::fmt_qty;
use crate::state::{AppState, LogEntry, PlateItem};

#[component]
pub fn EditEntrySheet(entry: LogEntry) -> Element {
    let mut state = use_context::<AppState>();
    let foods = use_context::<Resource<Option<Database>>>();
    let mut hour = use_signal(|| entry.hour);
    use_back_handler(move |_| state.editing.set(None));

    // The food it came from, to change the amount with its servings. Quick
    // adds, and foods no longer available, get their numbers edited instead.
    let food = entry.food_id.as_deref().and_then(|id| {
        let guard = foods.read();
        let db = guard.as_ref().and_then(Option::as_ref);
        food_search::resolve(db, &state.custom_foods.read(), id).map(|found| found.food)
    });
    let id = entry.id;
    let date_text = entry.date.format("%a %-d %b").to_string();

    rsx! {
        div { class: "sheet-backdrop",
            div { class: "sheet", role: "dialog", aria_label: "Edit {entry.name}",
                header { class: "sheet-header",
                    button {
                        class: "icon-btn",
                        aria_label: "Close without saving",
                        onclick: move |_| state.editing.set(None),
                        Icon { name: "close" }
                    }
                    div { class: "sheet-when",
                        span { class: "muted", "{date_text}" }
                        select {
                            class: "chip",
                            aria_label: "Time",
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
                    div { class: "sheet-total", "Editing" }
                }
                div { class: "sheet-body",
                    if let Some(food) = food {
                        FoodDetail {
                            food,
                            editing: Some(entry.clone()),
                            on_back: move |_| state.editing.set(None),
                            on_save: move |item| state.update_entry(id, hour(), item),
                        }
                    } else {
                        NumbersForm { entry: entry.clone(), on_save: move |item| state.update_entry(id, hour(), item) }
                    }
                }
            }
        }
    }
}

/// Edits a quick add's name, calories and macros directly.
#[component]
fn NumbersForm(entry: LogEntry, on_save: EventHandler<PlateItem>) -> Element {
    let start = |n: Nutrient| entry.nutrients.get(n).map(fmt_qty).unwrap_or_default();
    let mut name = use_signal(|| entry.name.clone());
    let kcal = use_signal(|| start(Nutrient::Energy));
    let protein = use_signal(|| start(Nutrient::Protein));
    let fat = use_signal(|| start(Nutrient::Fat));
    let carbs = use_signal(|| start(Nutrient::Carbs));
    let number = |text: String| {
        if text.trim().is_empty() {
            Some(0.0)
        } else {
            parse_quantity(&text)
        }
    };
    let energy = number(kcal()).filter(|k| *k > 0.0);
    let valid = energy.is_some()
        && [protein(), fat(), carbs()]
            .into_iter()
            .all(|t| number(t).is_some());

    rsx! {
        p { class: "muted small", "{entry.amount}" }
        label { class: "field",
            span { "Name" }
            input {
                r#type: "text",
                value: "{name}",
                oninput: move |e| name.set(e.value()),
            }
        }
        div { class: "field-grid",
            NumberField { label: "Calories", unit: "kcal", value: kcal }
            NumberField { label: "Protein", unit: "g", value: protein }
            NumberField { label: "Fat", unit: "g", value: fat }
            NumberField { label: "Carbs", unit: "g", value: carbs }
        }
        button {
            class: "primary wide",
            disabled: !valid,
            onclick: move |_| {
                let mut nutrients = entry.nutrients.clone();
                for (nutrient, text) in [
                    (Nutrient::Energy, kcal()),
                    (Nutrient::Protein, protein()),
                    (Nutrient::Fat, fat()),
                    (Nutrient::Carbs, carbs()),
                ] {
                    if let Some(value) = number(text) {
                        nutrients.set(nutrient, value);
                    }
                }
                let label = name();
                on_save.call(PlateItem {
                    name: if label.trim().is_empty() { entry.name.clone() } else { label.trim().to_string() },
                    amount: entry.amount.clone(),
                    nutrients,
                    food_id: entry.food_id.clone(),
                    grams: None,
                });
            },
            "Save"
        }
    }
}
