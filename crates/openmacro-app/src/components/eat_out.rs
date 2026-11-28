//! The logging sheet's Eat out tab: fast-food chains, each one's menu, and
//! what to order there to fit the rest of the day.

use dioxus::prelude::*;
use openmacro_core::eat_out::{Suggestions, suggest};
use openmacro_core::food::Food;

use super::Icon;
use super::logger::ResultSection;
use crate::eat_out::{self, MenuItem};
use crate::food_search::{EAT_OUT_SOURCE, FoundFood};
use crate::prefs;

/// Suggestions shown above a menu.
const SUGGESTED: usize = 3;

/// `chain` is the open chain's id, kept by the sheet so a menu stays open
/// while one of its items is looked at.
#[component]
pub fn EatOutTab(
    chain: Signal<Option<String>>,
    kcal_left: f64,
    kcal_target: f64,
    hour: u32,
    on_select: EventHandler<Food>,
) -> Element {
    let mut chain = chain;
    match chain().as_deref().and_then(eat_out::chain) {
        Some(open) => rsx! {
            Menu {
                id: open.id.clone(),
                kcal_left,
                kcal_target,
                hour,
                on_back: move |_| chain.set(None),
                on_select,
            }
        },
        None => rsx! {
            p { class: "muted pad",
                "Pick a chain to see its menu, with suggestions that fit the rest of your day."
            }
            ul { class: "results",
                for c in eat_out::chains() {
                    li { key: "{c.id}", class: "result",
                        button {
                            class: "result-main",
                            onclick: move |_| chain.set(Some(c.id.clone())),
                            div { class: "entry-name", "{c.name}" }
                            div { class: "entry-sub", "{c.items.len()} items" }
                        }
                    }
                }
            }
        },
    }
}

fn found(item: &MenuItem) -> FoundFood {
    FoundFood {
        food: item.food.clone(),
        source: EAT_OUT_SOURCE,
    }
}

#[component]
fn Menu(
    id: String,
    kcal_left: f64,
    kcal_target: f64,
    hour: u32,
    on_back: EventHandler<()>,
    on_select: EventHandler<Food>,
) -> Element {
    let Some(chain) = eat_out::chain(&id) else {
        return rsx! {};
    };
    let suggestions = suggest(&chain.choices(hour), kcal_left, kcal_target, SUGGESTED);
    let left = prefs::energy_text(kcal_left.max(0.0));
    let why = match &suggestions {
        Suggestions::Fits(_) => {
            format!(
                "Meal-sized picks that fit the {left} you have left today, with the most protein for their calories."
            )
        }
        Suggestions::Lightest(_) if kcal_left > 0.0 => {
            format!("Nothing here fits the {left} you have left today, so these are the lightest.")
        }
        Suggestions::Lightest(_) => {
            "You've reached today's target, so these are the lightest.".to_string()
        }
    };
    let suggested: Vec<FoundFood> = suggestions
        .indexes()
        .iter()
        .map(|&i| found(&chain.items[i]))
        .collect();
    rsx! {
        button { class: "link-btn", onclick: move |_| on_back.call(()),
            Icon { name: "left" }
            "All chains"
        }
        h2 { class: "detail-title", "{chain.name}" }
        if !suggested.is_empty() {
            ResultSection { key: "suggested-{chain.id}", title: "Suggested for you", foods: suggested, on_select }
            p { class: "muted small", "{why}" }
        }
        for (category , items) in chain.sections() {
            ResultSection {
                key: "{chain.id}-{category}",
                title: category,
                foods: items.into_iter().map(found).collect::<Vec<_>>(),
                on_select,
            }
        }
        p { class: "muted small pad",
            "Nutrition from "
            a { href: "{chain.source.url}", target: "_blank", {chain.source_line()} }
            ". Recipes change, so treat it as a guide."
        }
    }
}
