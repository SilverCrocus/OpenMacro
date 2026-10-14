//! Saving the foods on the plate as a recipe.

use dioxus::prelude::*;
use openmacro_core::food::{Amount, parse_quantity};
use openmacro_core::nutrients::Nutrient;
use openmacro_core::recipe::{Ingredient, Recipe};

use super::library::Field;
use super::{fmt0, macro_line};
use crate::back::use_back_handler;
use crate::foods::{fmt_qty, plate_item};
use crate::state::{AppState, PlateItem};

/// The recipe the form describes, if it's complete enough to save.
fn build(
    name: &str,
    servings: &str,
    cooked: &str,
    items: &[PlateItem],
) -> Result<Recipe, &'static str> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Give the recipe a name.");
    }
    let servings = parse_quantity(servings)
        .filter(|s| *s > 0.0)
        .ok_or("Enter how many servings it makes.")?;
    let cooked_grams = if cooked.trim().is_empty() {
        None
    } else {
        Some(
            parse_quantity(cooked)
                .filter(|g| *g > 0.0)
                .ok_or("Enter the cooked weight in grams, or leave it blank.")?,
        )
    };
    Ok(Recipe {
        name: name.to_string(),
        servings,
        cooked_grams,
        ingredients: items
            .iter()
            .map(|i| Ingredient {
                name: i.name.clone(),
                amount: i.amount.clone(),
                grams: i.grams,
                nutrients: i.nutrients.clone(),
                food_id: i.food_id.clone(),
            })
            .collect(),
    })
}

#[component]
pub fn RecipeForm(
    items: Vec<PlateItem>,
    on_done: EventHandler<()>,
    /// The id of a recipe being edited, to update instead of creating one.
    #[props(default)]
    existing: Option<String>,
) -> Element {
    let state = use_context::<AppState>();
    use_back_handler(move |_| on_done.call(()));
    let old = existing
        .as_ref()
        .and_then(|id| state.recipes.peek().get(id).cloned());
    let initial = use_hook(|| {
        old.as_ref().map_or_else(
            || (String::new(), "1".to_string(), String::new()),
            |r| {
                (
                    r.name.clone(),
                    fmt_qty(r.servings),
                    r.cooked_grams.map(fmt_qty).unwrap_or_default(),
                )
            },
        )
    });
    let mut name = use_signal(|| initial.0.clone());
    let mut servings = use_signal(|| initial.1.clone());
    let mut cooked = use_signal(|| initial.2.clone());
    let editing = use_signal(|| existing.clone());
    let mut tried = use_signal(|| false);

    let result = build(&name(), &servings(), &cooked(), &items);
    let preview = result.as_ref().ok().map(|recipe| {
        let food = recipe.to_food("");
        let serving = food
            .nutrients_for(Amount::Servings {
                index: 0,
                count: 1.0,
            })
            .unwrap_or_default();
        let weight = match recipe.weight() {
            Some(_) => format!("{} g", fmt0(food.servings[0].grams)),
            None => "weight unknown".to_string(),
        };
        format!(
            "Per serving ({weight}): {} kcal · {}",
            fmt0(serving.get_or_zero(Nutrient::Energy)),
            macro_line(&serving)
        )
    });
    let raw = items.iter().map(|i| i.grams).sum::<Option<f64>>();
    let weight_hint = match raw {
        Some(g) => format!(
            "The ingredients weigh {} g raw. Weigh the finished dish for a better per-gram figure, since cooking changes weight.",
            fmt0(g)
        ),
        None => "Some ingredients have no weight, so the recipe can be logged by the serving only, unless you enter the cooked weight.".to_string(),
    };

    rsx! {
        h3 { class: "section-title",
            if existing.is_some() { "Update recipe" } else { "Save as recipe" }
        }
        p { class: "muted small",
            if existing.is_some() {
                "The recipe will use the {items.len()} ingredients on the plate. Entries you've already logged keep their old values."
            } else {
                "The {items.len()} foods on your plate become one recipe you can log again from search or the Library."
            }
        }
        Field {
            label: "Name",
            initial: initial.0.clone(),
            numeric: false,
            on_input: move |v| name.set(v),
        }
        div { class: "field-grid",
            Field {
                label: "Servings it makes",
                initial: initial.1.clone(),
                numeric: true,
                on_input: move |v| servings.set(v),
            }
            Field {
                label: "Cooked weight (g, optional)",
                initial: initial.2.clone(),
                numeric: true,
                on_input: move |v| cooked.set(v),
            }
        }
        p { class: "muted small", "{weight_hint}" }
        if let Some(preview) = preview {
            p { class: "recipe-preview", "{preview}" }
        }
        if tried() {
            if let Err(message) = result {
                p { class: "error", "{message}" }
            }
        }
        div { class: "button-row form-actions",
            button { onclick: move |_| on_done.call(()), "Cancel" }
            button {
                class: "primary",
                onclick: move |_| {
                    tried.set(true);
                    let Ok(recipe) = build(&name(), &servings(), &cooked(), &items) else {
                        return;
                    };
                    if let Some(id) = editing() {
                        state.update_recipe(&id, &recipe);
                        state.close_plate();
                        return;
                    }
                    let food = state.add_recipe(&recipe);
                    // Swap the ingredients for one serving of the recipe, ready to log.
                    let serving = plate_item(&food, Amount::Servings { index: 0, count: 1.0 });
                    state.set_plate_items(serving.into_iter().collect());
                    on_done.call(());
                },
                "Save recipe"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use openmacro_core::nutrients::NutrientProfile;

    use super::*;

    fn item(grams: Option<f64>) -> PlateItem {
        PlateItem {
            name: "Rice".into(),
            amount: "1 cup".into(),
            nutrients: NutrientProfile::from_macros(200.0, 4.0, 0.5, 44.0),
            food_id: None,
            grams,
        }
    }

    #[test]
    fn needs_a_name_and_servings() {
        let items = [item(Some(160.0))];
        assert!(build(" ", "1", "", &items).is_err());
        assert!(build("Rice", "0", "", &items).is_err());
        assert!(build("Rice", "1", "abc", &items).is_err());
        let recipe = build(" Rice ", "1 1/2", "", &items).unwrap();
        assert_eq!(recipe.name, "Rice");
        assert_eq!(recipe.servings, 1.5);
        assert_eq!(recipe.cooked_grams, None);
        assert_eq!(recipe.ingredients[0].grams, Some(160.0));
        assert_eq!(
            build("Rice", "2", "300", &items).unwrap().cooked_grams,
            Some(300.0)
        );
    }
}
