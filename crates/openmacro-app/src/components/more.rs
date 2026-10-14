//! Profile, data management and about.

use dioxus::prelude::*;
use openmacro_core::bmr::{self, ActivityLevel, BmrEquation, Sex};

use super::fmt0;
use crate::food_search::{self, Database, FoodRegion};
use crate::state::{AppState, Data, today};

#[component]
pub fn More() -> Element {
    let mut state = use_context::<AppState>();
    let profile = (state.profile)();
    let stats = profile.stats;
    let prior = bmr::initial_expenditure(&stats, profile.equation, profile.activity);
    let body_fat_pct = stats
        .body_fat_fraction
        .map(|f| format!("{:.0}", f * 100.0))
        .unwrap_or_default();
    let prior_text = format!("{} kcal/day", fmt0(prior));
    let mut confirm_clear = use_signal(|| false);
    let saved_at = state.persist.read().location();
    let foods = use_context::<Resource<Option<Database>>>();
    let (food_count, credits) = {
        let guard = foods.read();
        let db = guard.as_ref().and_then(Option::as_ref);
        (food_search::food_count(db), food_search::attribution(db))
    };
    let region = (state.region)();

    rsx! {
        header { class: "page-header",
            h1 { "More" }
        }
        section { class: "card",
            h2 { "Profile" }
            p { class: "muted small", "Used for the starting expenditure estimate before you have logged enough data." }
            div { class: "field-grid",
                label { class: "field",
                    span { "Sex" }
                    select {
                        onchange: move |e| state.profile.write().stats.sex = if e.value() == "female" { Sex::Female } else { Sex::Male },
                        option { value: "male", selected: stats.sex == Sex::Male, "Male" }
                        option { value: "female", selected: stats.sex == Sex::Female, "Female" }
                    }
                }
                ProfileNumber {
                    label: "Age",
                    value: stats.age_years,
                    on_change: move |v| state.profile.write().stats.age_years = v,
                }
                ProfileNumber {
                    label: "Height (cm)",
                    value: stats.height_cm,
                    on_change: move |v| state.profile.write().stats.height_cm = v,
                }
                ProfileNumber {
                    label: "Weight (kg)",
                    value: stats.weight_kg,
                    on_change: move |v| state.profile.write().stats.weight_kg = v,
                }
                label { class: "field",
                    span { "Body fat % (optional)" }
                    input {
                        r#type: "number",
                        step: "1",
                        value: "{body_fat_pct}",
                        onchange: move |e| {
                            state.profile.write().stats.body_fat_fraction = e
                                .value()
                                .parse::<f64>()
                                .ok()
                                .filter(|v| (2.0..70.0).contains(v))
                                .map(|v| v / 100.0);
                        },
                    }
                }
                label { class: "field",
                    span { "Activity" }
                    select {
                        onchange: move |e| {
                            state.profile.write().activity = match e.value().as_str() {
                                "sedentary" => ActivityLevel::Sedentary,
                                "light" => ActivityLevel::Light,
                                "very" => ActivityLevel::Very,
                                "extreme" => ActivityLevel::Extreme,
                                _ => ActivityLevel::Moderate,
                            };
                        },
                        for (value , text , level) in [
                            ("sedentary", "Sedentary", ActivityLevel::Sedentary),
                            ("light", "Light", ActivityLevel::Light),
                            ("moderate", "Moderate", ActivityLevel::Moderate),
                            ("very", "Very active", ActivityLevel::Very),
                            ("extreme", "Extremely active", ActivityLevel::Extreme),
                        ]
                        {
                            option { key: "{value}", value, selected: profile.activity == level, "{text}" }
                        }
                    }
                }
                label { class: "field",
                    span { "BMR equation" }
                    select {
                        onchange: move |e| {
                            state.profile.write().equation = match e.value().as_str() {
                                "katch" => BmrEquation::KatchMcArdle,
                                "cunningham" => BmrEquation::Cunningham,
                                _ => BmrEquation::MifflinStJeor,
                            };
                        },
                        for (value , text , eq) in [
                            ("mifflin", "Mifflin-St Jeor", BmrEquation::MifflinStJeor),
                            ("katch", "Katch-McArdle (needs body fat)", BmrEquation::KatchMcArdle),
                            ("cunningham", "Cunningham (needs body fat)", BmrEquation::Cunningham),
                        ]
                        {
                            option { key: "{value}", value, selected: profile.equation == eq, "{text}" }
                        }
                    }
                }
            }
            p { class: "muted pad", "Starting estimate: {prior_text}" }
        }
        section { class: "card",
            h2 { "Food database" }
            label { class: "field",
                span { "Products and spellings from" }
                select {
                    onchange: move |e| {
                        if let Some(r) = FoodRegion::from_code(&e.value()) {
                            state.region.set(r);
                        }
                    },
                    for r in FoodRegion::ALL {
                        option { key: "{r.code()}", value: r.code(), selected: region == r, {r.label()} }
                    }
                }
            }
            p { class: "muted small", "{food_count} foods available to search." }
            if let Some(credits) = credits {
                p { class: "muted small", "{credits}" }
            } else {
                p { class: "muted small",
                    "Using built-in sample foods. Builds with the bundled database include USDA FoodData Central, the Australian Food Composition Database (FSANZ) and Open Food Facts."
                }
            }
        }
        section { class: "card",
            h2 { "Data" }
            p { class: "muted small",
                match &saved_at {
                    Some(path) => rsx! { "Saved on this device: {path}" },
                    None => rsx! { "Not saved: this preview keeps data in memory until you close it." },
                }
            }
            div { class: "button-row",
                button { onclick: move |_| state.replace_data(crate::seed::demo(today())), "Replace with demo data" }
                if confirm_clear() {
                    button {
                        class: "danger",
                        onclick: move |_| {
                            state.replace_data(Data::empty((state.profile)()));
                            confirm_clear.set(false);
                        },
                        "Really clear everything"
                    }
                } else {
                    button { onclick: move |_| confirm_clear.set(true), "Clear all data" }
                }
            }
        }
        section { class: "card",
            h2 { "About" }
            p { "OpenMacro {env!(\"CARGO_PKG_VERSION\")} · MIT licensed" }
            p { class: "muted small",
                "An independent, open-source nutrition tracker. Not affiliated with any commercial app."
            }
            a { href: "https://github.com/SilverCrocus/OpenMacro", target: "_blank", "github.com/SilverCrocus/OpenMacro" }
        }
    }
}

#[component]
fn ProfileNumber(label: &'static str, value: f64, on_change: EventHandler<f64>) -> Element {
    rsx! {
        label { class: "field",
            span { "{label}" }
            input {
                r#type: "number",
                step: "any",
                value: "{value}",
                onchange: move |e| {
                    if let Ok(v) = e.value().parse::<f64>() {
                        if v > 0.0 {
                            on_change.call(v);
                        }
                    }
                },
            }
        }
    }
}
