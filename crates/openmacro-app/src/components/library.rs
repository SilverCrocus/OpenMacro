//! The Library tab: the user's own foods, and the form to create one.

use dioxus::prelude::*;
use openmacro_core::food::{Food, FoodSource};
use openmacro_core::nutrients::Nutrient;

use super::{Icon, fmt0, macro_line};
use crate::back::use_back_handler;
use crate::custom_food::{self, Basis, Input, LabelStyle};
use crate::food_search::FoodRegion;
use crate::foods::{amount_label, plate_item};
use crate::state::AppState;

#[component]
pub fn LibraryTab(on_select: EventHandler<Food>, new_barcode: Signal<Option<String>>) -> Element {
    let state = use_context::<AppState>();
    let mut creating = use_signal(|| false);
    let mut editing = use_signal(|| None::<Food>);
    let mut new_barcode = new_barcode;
    let foods = state.custom_foods.read().clone();
    let favorites = state.favorites.read().clone();

    if let Some(food) = editing() {
        return rsx! {
            CustomFoodForm { barcode: String::new(), existing: Some(food), on_done: move |_| editing.set(None) }
        };
    }
    if creating() || new_barcode().is_some() {
        return rsx! {
            CustomFoodForm {
                barcode: new_barcode().unwrap_or_default(),
                on_done: move |_| {
                    creating.set(false);
                    new_barcode.set(None);
                },
            }
        };
    }

    rsx! {
        if !favorites.is_empty() {
            h3 { class: "section-title", "Favorites" }
            ul { class: "results",
                for food in favorites {
                    {
                        let open = food.clone();
                        let origin = food.brand.clone().unwrap_or_default();
                        rsx! {
                            li { key: "fav-{food.id}", class: "result",
                                button { class: "result-main", onclick: move |_| on_select.call(open.clone()),
                                    div { class: "entry-name", "★ {food.name}" }
                                    if !origin.is_empty() {
                                        div { class: "entry-origin", "{origin}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        div { class: "library-head",
            h3 { class: "section-title", "Your foods" }
            button { class: "chip", onclick: move |_| creating.set(true),
                Icon { name: "plus" }
                "New food"
            }
        }
        if foods.is_empty() {
            p { class: "muted pad",
                "Foods you create appear here and in search. Add things the database doesn't have, like home cooking or local products. To make a recipe, put its ingredients on the plate and tap Save as recipe."
            }
        }
        ul { class: "results",
            for food in foods {
                {
                    let summary = {
                        let amount = food.default_amount();
                        match (food.nutrients_for(amount), amount_label(&food, amount)) {
                            (Some(n), Some(label)) => format!(
                                "{}{} kcal · {} · {}",
                                if food.source == FoodSource::Recipe { "Recipe · " } else { "" },
                                fmt0(n.get_or_zero(Nutrient::Energy)),
                                macro_line(&n),
                                label
                            ),
                            _ => String::new(),
                        }
                    };
                    let id = food.id.clone();
                    let open = food.clone();
                    let change = food.clone();
                    rsx! {
                        li { key: "{food.id}", class: "result",
                            button { class: "result-main", onclick: move |_| on_select.call(open.clone()),
                                div { class: "entry-name", "{food.name}" }
                                div { class: "entry-sub", "{summary}" }
                                if let Some(brand) = &food.brand {
                                    div { class: "entry-origin", "{brand}" }
                                }
                            }
                            button {
                                class: "icon-btn small",
                                aria_label: "Edit {food.name}",
                                onclick: move |_| {
                                    if change.source == FoodSource::Recipe {
                                        state.edit_recipe(&change.id);
                                    } else {
                                        editing.set(Some(change.clone()));
                                    }
                                },
                                Icon { name: "edit" }
                            }
                            button {
                                class: "icon-btn small",
                                aria_label: "Delete {food.name}",
                                onclick: move |_| state.delete_custom_food(&id),
                                Icon { name: "close" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A text field that reports edits without writing its value back, so fast
/// typing never loses characters.
#[component]
pub(super) fn Field(
    label: String,
    initial: String,
    numeric: bool,
    on_input: EventHandler<String>,
) -> Element {
    rsx! {
        label { class: "field",
            span { "{label}" }
            input {
                r#type: "text",
                "inputmode": if numeric { "decimal" } else { "text" },
                initial_value: "{initial}",
                oninput: move |e| on_input.call(e.value()),
            }
        }
    }
}

#[component]
fn CustomFoodForm(
    barcode: String,
    on_done: EventHandler<()>,
    /// A saved food to edit instead of creating a new one.
    #[props(default)]
    existing: Option<Food>,
) -> Element {
    let state = use_context::<AppState>();
    use_back_handler(move |_| on_done.call(()));
    let region = (state.region)();
    // The id being edited; a signal so `save` stays `Copy`.
    let editing = use_signal(|| existing.as_ref().map(|f| f.id.clone()));
    let mut form = use_signal(|| match &existing {
        Some(food) => Input::from_food(food),
        None => Input {
            barcode: barcode.clone(),
            serving_label: "serving".into(),
            label: if region == FoodRegion::Australia {
                LabelStyle::Australian
            } else {
                LabelStyle::UnitedStates
            },
            energy_in_kj: region == FoodRegion::Australia,
            ..Input::default()
        },
    });
    // What the fields start with; they aren't bound to `form` afterwards.
    let initial = use_hook(|| form.peek().clone());
    let mut error = use_signal(|| None::<String>);
    let current = form();

    let mut save = move |add: bool| match custom_food::build(&form()) {
        Ok(mut food) => {
            if let Some(id) = editing() {
                food.id = id;
                state.update_custom_food(food);
            } else {
                let saved = state.add_custom_food(food);
                if add {
                    if let Some(item) = plate_item(&saved, saved.default_amount()) {
                        state.add_to_plate(item);
                    }
                }
            }
            on_done.call(());
        }
        Err(message) => error.set(Some(message)),
    };

    let per = match current.basis {
        Basis::Serving => "per serving",
        Basis::Per100g => "per 100 g",
        Basis::Per100ml => "per 100 ml",
    };
    let carbs_label = match current.label {
        LabelStyle::Australian => format!("Carbohydrate (g, {per})"),
        LabelStyle::UnitedStates => format!("Total carbohydrate (g, {per})"),
    };

    rsx! {
        button { class: "link-btn", onclick: move |_| on_done.call(()),
            Icon { name: "left" }
            "Library"
        }
        h2 { class: "detail-title",
            if existing.is_some() { "Edit food" } else { "New food" }
        }

        Field { label: "Name", initial: initial.name.clone(), numeric: false, on_input: move |v| form.write().name = v }
        Field { label: "Brand (optional)", initial: initial.brand.clone(), numeric: false, on_input: move |v| form.write().brand = v }
        Field {
            label: "Barcode (optional)",
            initial: initial.barcode.clone(),
            numeric: true,
            on_input: move |v| form.write().barcode = v,
        }

        div { class: "choice",
            span { class: "choice-label", "Nutrition is entered" }
            div { class: "segmented",
                for (value , text) in [(Basis::Serving, "Per serving"), (Basis::Per100g, "Per 100 g"), (Basis::Per100ml, "Per 100 ml")] {
                    button {
                        key: "{text}",
                        class: if current.basis == value { "active" },
                        onclick: move |_| form.write().basis = value,
                        "{text}"
                    }
                }
            }
        }
        div { class: "field-grid",
            Field {
                label: "Serving name",
                initial: initial.serving_label.clone(),
                numeric: false,
                on_input: move |v| form.write().serving_label = v,
            }
            Field {
                label: if current.basis == Basis::Serving { "Serving weight (g)" } else { "Serving weight (g, optional)" },
                initial: initial.serving_grams.clone(),
                numeric: true,
                on_input: move |v| form.write().serving_grams = v,
            }
        }

        div { class: "choice",
            span { class: "choice-label", "Label style" }
            div { class: "segmented",
                for (value , text) in [(LabelStyle::Australian, "Australian / NZ"), (LabelStyle::UnitedStates, "US")] {
                    button {
                        key: "{text}",
                        class: if current.label == value { "active" },
                        onclick: move |_| form.write().label = value,
                        "{text}"
                    }
                }
            }
        }

        div { class: "energy-row",
            Field {
                label: format!("Energy ({per})"),
                initial: initial.energy.clone(),
                numeric: true,
                on_input: move |v| form.write().energy = v,
            }
            div { class: "segmented small-units",
                button { class: if !current.energy_in_kj { "active" }, onclick: move |_| form.write().energy_in_kj = false, "kcal" }
                button { class: if current.energy_in_kj { "active" }, onclick: move |_| form.write().energy_in_kj = true, "kJ" }
            }
        }
        div { class: "field-grid",
            Field { label: format!("Protein (g, {per})"), initial: initial.protein.clone(), numeric: true, on_input: move |v| form.write().protein = v }
            Field { label: format!("Fat (g, {per})"), initial: initial.fat.clone(), numeric: true, on_input: move |v| form.write().fat = v }
            Field { label: "– saturated (g)", initial: initial.saturated.clone(), numeric: true, on_input: move |v| form.write().saturated = v }
            Field { label: carbs_label, initial: initial.carbs.clone(), numeric: true, on_input: move |v| form.write().carbs = v }
            Field { label: "– sugars (g)", initial: initial.sugars.clone(), numeric: true, on_input: move |v| form.write().sugars = v }
            Field { label: "Fibre (g)", initial: initial.fiber.clone(), numeric: true, on_input: move |v| form.write().fiber = v }
        }
        div { class: "energy-row",
            Field {
                label: if current.salt { "Salt (g)" } else { "Sodium (mg)" },
                initial: initial.sodium.clone(),
                numeric: true,
                on_input: move |v| form.write().sodium = v,
            }
            div { class: "segmented small-units",
                button { class: if !current.salt { "active" }, onclick: move |_| form.write().salt = false, "Sodium" }
                button { class: if current.salt { "active" }, onclick: move |_| form.write().salt = true, "Salt" }
            }
        }

        if let Some(message) = error() {
            p { class: "error", "{message}" }
        }
        div { class: "button-row form-actions",
            if existing.is_some() {
                button { onclick: move |_| on_done.call(()), "Cancel" }
                button { class: "primary", onclick: move |_| save(false), "Save" }
            } else {
                button { onclick: move |_| save(false), "Create" }
                button { class: "primary", onclick: move |_| save(true), "Create & add" }
            }
        }
    }
}
