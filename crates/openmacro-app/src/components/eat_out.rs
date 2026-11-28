//! The logging sheet's Eat out tab: fast-food chains, each one's menu, and
//! what to order there to fit the rest of the day.

use dioxus::prelude::*;
use openmacro_core::eat_out::{Pick, Suggestions, suggest};
use openmacro_core::food::{Amount, Food};

use super::Icon;
use super::logger::{ResultSection, amount_summary};
use crate::back::use_back_handler;
use crate::eat_out::{self, Chain, MenuItem, Part};
use crate::food_search::{EAT_OUT_SOURCE, FoundFood};
use crate::foods::plate_item;
use crate::prefs;
use crate::state::AppState;

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
                "Pick a chain to see its Australian menu, with suggestions that fit the rest of your day."
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

/// The energy leaving `part` off saves: "146 kcal", or "about 90 kcal" when
/// it's estimated rather than the chain's own figure.
pub fn saving(part: &Part) -> String {
    if part.published {
        prefs::energy_text(part.kcal())
    } else {
        format!(
            "about {}",
            prefs::energy_text((part.kcal() / 10.0).round() * 10.0)
        )
    }
}

/// Leaving off less than this isn't worth a tip.
const TIP_KCAL: f64 = 40.0;

/// "Ask for no mayo to save 146 kcal", for the part that saves the most.
fn tip(item: &MenuItem) -> Option<String> {
    let part = item.parts.first().filter(|p| p.kcal() >= TIP_KCAL)?;
    Some(format!("Ask for no {} to save {}", part.name, saving(part)))
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
    // Back goes to the chain list rather than closing the sheet.
    use_back_handler(move |_| on_back.call(()));
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
            format!(
                "Nothing here is meal-sized for the {left} you have left today, so these are the lightest."
            )
        }
        Suggestions::Lightest(_) => {
            "You've reached today's target, so these are the lightest.".to_string()
        }
    };
    let picks = suggestions.picks().to_vec();
    rsx! {
        button { class: "link-btn", onclick: move |_| on_back.call(()),
            Icon { name: "left" }
            "All chains"
        }
        h2 { class: "detail-title", "{chain.name}" }
        if !picks.is_empty() {
            Suggested { key: "suggested-{chain.id}", chain: chain.id.clone(), picks, on_select }
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

/// The suggested orders, each at its count: a pizza as several slices.
#[component]
fn Suggested(chain: String, picks: Vec<Pick>, on_select: EventHandler<Food>) -> Element {
    let state = use_context::<AppState>();
    let Some(chain): Option<&Chain> = eat_out::chain(&chain) else {
        return rsx! {};
    };
    rsx! {
        h3 { class: "section-title result-section", "Suggested for you" }
        ul { class: "results",
            for pick in picks {
                {
                    let item = &chain.items[pick.index];
                    let food = item.food.clone();
                    let amount = Amount::Servings {
                        index: 0,
                        count: f64::from(pick.count),
                    };
                    let (open, add) = (food.clone(), food.clone());
                    let tip = tip(item);
                    rsx! {
                        li { key: "{food.id}", class: "result",
                            button { class: "result-main", onclick: move |_| on_select.call(open.clone()),
                                div { class: "entry-name", "{food.name}" }
                                div { class: "entry-sub", {amount_summary(&food, amount)} }
                                if let Some(tip) = tip {
                                    div { class: "entry-tip", "{tip}" }
                                }
                                div { class: "entry-origin", "{chain.name} · {EAT_OUT_SOURCE}" }
                            }
                            button {
                                class: "icon-btn add",
                                aria_label: "Add {food.name} to plate",
                                onclick: move |_| {
                                    if let Some(item) = plate_item(&add, amount) {
                                        state.add_to_plate(item);
                                    }
                                },
                                Icon { name: "plus" }
                            }
                        }
                    }
                }
            }
        }
    }
}
