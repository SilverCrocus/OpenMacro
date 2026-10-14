//! The logging sheet: stage foods on a plate, then log them together.

use dioxus::prelude::*;
use openmacro_core::food::{Amount, Food, parse_quantity};
use openmacro_core::nutrients::{
    KCAL_PER_G_CARBS, KCAL_PER_G_FAT, KCAL_PER_G_PROTEIN, Nutrient, NutrientProfile,
};

use super::{Icon, fmt0, fmt1, hour_label, macro_line};
use crate::foods::{FOODS, fmt_qty, plate_item, search};
use crate::state::{AppState, CoachSnapshot, PlateItem, day_totals};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Tab {
    Search,
    QuickAdd,
}

#[component]
pub fn Logger() -> Element {
    let mut state = use_context::<AppState>();
    let coach = use_context::<Memo<CoachSnapshot>>();
    let mut tab = use_signal(|| Tab::Search);
    let mut detail = use_signal(|| None::<usize>);
    let query = use_signal(String::new);

    let plate = state.plate.read().clone();
    let plate_kcal = plate.totals().get_or_zero(Nutrient::Energy);
    let day_kcal = day_totals(&state.log.read(), plate.date).get_or_zero(Nutrient::Energy);
    let target_kcal = coach.read().target_for(plate.date).kcal;
    let total_text = format!(
        "{} / {} kcal",
        fmt0(day_kcal + plate_kcal),
        fmt0(target_kcal)
    );
    let date_text = plate.date.format("%a %-d %b").to_string();
    let count = plate.items.len();
    let log_text = if count == 1 {
        "Log 1 item".to_string()
    } else {
        format!("Log {count} items")
    };

    rsx! {
        div { class: "sheet-backdrop",
            div { class: "sheet", role: "dialog", aria_label: "Log food",
                header { class: "sheet-header",
                    button {
                        class: "icon-btn",
                        aria_label: "Close and discard plate",
                        onclick: move |_| state.close_plate(),
                        Icon { name: "close" }
                    }
                    div { class: "sheet-when",
                        span { class: "muted", "{date_text}" }
                        select {
                            class: "chip",
                            aria_label: "Time",
                            onchange: move |e| {
                                if let Ok(hour) = e.value().parse() {
                                    state.plate.write().hour = hour;
                                }
                            },
                            for h in 0..24u32 {
                                option { key: "{h}", value: "{h}", selected: h == plate.hour, {hour_label(h)} }
                            }
                        }
                    }
                    div { class: "sheet-total", "{total_text}" }
                }
                if count > 0 {
                    PlateView { items: plate.items.clone() }
                }
                nav { class: "sheet-tabs",
                    button {
                        class: if tab() == Tab::Search { "active" },
                        onclick: move |_| tab.set(Tab::Search),
                        "Search"
                    }
                    button {
                        class: if tab() == Tab::QuickAdd { "active" },
                        onclick: move |_| tab.set(Tab::QuickAdd),
                        "Quick Add"
                    }
                    button { disabled: true, title: "Coming soon", "Scan" }
                    button { disabled: true, title: "Coming soon", "Library" }
                }
                div { class: "sheet-body",
                    {
                        match (tab(), detail()) {
                            (Tab::Search, Some(index)) => rsx! {
                                FoodDetail { index, on_back: move |_| detail.set(None) }
                            },
                            (Tab::Search, None) => rsx! {
                                SearchTab { query, on_select: move |i| detail.set(Some(i)) }
                            },
                            (Tab::QuickAdd, _) => rsx! {
                                QuickAdd {}
                            },
                        }
                    }
                }
                if count > 0 {
                    footer { class: "sheet-footer",
                        button { class: "primary wide", onclick: move |_| state.log_plate(), "{log_text}" }
                    }
                }
            }
        }
    }
}

#[component]
fn PlateView(items: Vec<PlateItem>) -> Element {
    let state = use_context::<AppState>();
    let totals: NutrientProfile = items.iter().map(|i| &i.nutrients).sum();
    let summary = format!(
        "{} kcal · {}",
        fmt0(totals.get_or_zero(Nutrient::Energy)),
        macro_line(&totals)
    );
    rsx! {
        section { class: "plate",
            div { class: "plate-head",
                h3 { "Plate" }
                span { class: "muted", "{summary}" }
            }
            for (index , item) in items.into_iter().enumerate() {
                div { key: "{index}", class: "plate-item",
                    div { class: "entry-main",
                        div { class: "entry-name", "{item.name}" }
                        div { class: "entry-sub", {format!("{} · {}", item.amount, macro_line(&item.nutrients))} }
                    }
                    div { class: "entry-kcal", {fmt0(item.nutrients.get_or_zero(Nutrient::Energy))} }
                    button {
                        class: "icon-btn small",
                        aria_label: "Remove {item.name}",
                        onclick: move |_| state.remove_from_plate(index),
                        Icon { name: "close" }
                    }
                }
            }
        }
    }
}

#[component]
fn SearchTab(query: Signal<String>, on_select: EventHandler<usize>) -> Element {
    let state = use_context::<AppState>();
    let mut query = query;
    let q = query();
    let results = search(&FOODS, &q);
    rsx! {
        label { class: "search-box",
            Icon { name: "search" }
            input {
                r#type: "search",
                placeholder: "Search foods",
                value: "{q}",
                oninput: move |e| query.set(e.value()),
            }
        }
        if q.trim().is_empty() {
            p { class: "muted pad", "Search the built-in sample foods. The full USDA and Open Food Facts database is next." }
        } else if results.is_empty() {
            p { class: "muted pad", "No matches. Try Quick Add." }
        }
        ul { class: "results",
            for (index , food) in results {
                li { key: "{food.id}", class: "result",
                    button { class: "result-main", onclick: move |_| on_select.call(index),
                        div { class: "entry-name", "{food.name}" }
                        div { class: "entry-sub", {default_summary(food)} }
                    }
                    button {
                        class: "icon-btn add",
                        aria_label: "Add {food.name} to plate",
                        onclick: move |_| {
                            if let Some(item) = plate_item(food, food.default_amount()) {
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

/// "155 kcal · 13P 11F 1C · 1 large (50 g)"
fn default_summary(food: &Food) -> String {
    let amount = food.default_amount();
    match (
        food.nutrients_for(amount),
        crate::foods::amount_label(food, amount),
    ) {
        (Some(n), Some(label)) => {
            format!(
                "{} kcal · {} · {}",
                fmt0(n.get_or_zero(Nutrient::Energy)),
                macro_line(&n),
                label
            )
        }
        _ => String::new(),
    }
}

#[component]
fn FoodDetail(index: usize, on_back: EventHandler<()>) -> Element {
    let state = use_context::<AppState>();
    let coach = use_context::<Memo<CoachSnapshot>>();
    let food: &'static Food = &FOODS[index];
    let mut qty = use_signal(|| "1".to_string());
    let mut unit = use_signal(|| 0usize);

    // Units are the food's servings followed by grams.
    let grams_unit = food.servings.len();
    let amount = parse_quantity(&qty()).map(|q| {
        if unit() < grams_unit {
            Amount::Servings {
                index: unit(),
                count: q,
            }
        } else {
            Amount::Grams(q)
        }
    });
    let item = amount.and_then(|a| plate_item(food, a));
    let target = coach.read().target_for(state.plate.read().date);

    let per = item
        .as_ref()
        .map(|i| i.nutrients.clone())
        .unwrap_or_default();
    let kcal = per.get_or_zero(Nutrient::Energy);
    let share = |grams: f64, kcal_per_g: f64| {
        if kcal > 0.0 {
            grams * kcal_per_g / kcal * 100.0
        } else {
            0.0
        }
    };
    let headline = [
        (
            "Protein",
            per.get_or_zero(Nutrient::Protein),
            share(per.get_or_zero(Nutrient::Protein), KCAL_PER_G_PROTEIN),
            "protein",
        ),
        (
            "Fat",
            per.get_or_zero(Nutrient::Fat),
            share(per.get_or_zero(Nutrient::Fat), KCAL_PER_G_FAT),
            "fat",
        ),
        (
            "Carbs",
            per.get_or_zero(Nutrient::Carbs),
            share(per.get_or_zero(Nutrient::Carbs), KCAL_PER_G_CARBS),
            "carbs",
        ),
    ];
    let impact = [
        ("Calories", kcal / target.kcal.max(1.0)),
        (
            "Protein",
            per.get_or_zero(Nutrient::Protein) / target.protein_g.max(1.0),
        ),
        (
            "Fat",
            per.get_or_zero(Nutrient::Fat) / target.fat_g.max(1.0),
        ),
        (
            "Carbs",
            per.get_or_zero(Nutrient::Carbs) / target.carbs_g.max(1.0),
        ),
    ];
    let nutrient_rows: Vec<(String, String)> = per
        .iter()
        .map(|(n, v)| {
            (
                n.name().to_string(),
                format!("{} {}", fmt1(v), n.unit().symbol()),
            )
        })
        .collect();

    rsx! {
        button { class: "link-btn", onclick: move |_| on_back.call(()),
            Icon { name: "left" }
            "Results"
        }
        h2 { class: "detail-title", "{food.name}" }
        div { class: "detail-headline",
            div { class: "big-kcal",
                span { class: "num", {fmt0(kcal)} }
                span { class: "muted", "kcal" }
            }
            for (label , grams , pct , class) in headline {
                div { key: "{label}", class: "headline-macro {class}",
                    span { class: "num", {fmt1(grams)} }
                    span { class: "muted", "{label} · {pct:.0}%" }
                }
            }
        }
        div { class: "amount-row",
            input {
                class: "qty",
                r#type: "text",
                "inputmode": "decimal",
                aria_label: "Quantity",
                value: "{qty}",
                oninput: move |e| qty.set(e.value()),
            }
            select {
                aria_label: "Unit",
                onchange: move |e| unit.set(e.value().parse().unwrap_or(0)),
                {
                    food.servings
                        .iter()
                        .enumerate()
                        .map(|(i, s)| {
                            let text = format!("{} ({} g)", s.label, fmt_qty(s.grams));
                            rsx! {
                                option { key: "{i}", value: "{i}", selected: unit() == i, "{text}" }
                            }
                        })
                }
                option { value: "{grams_unit}", selected: unit() == grams_unit, "g" }
            }
        }
        if item.is_none() {
            p { class: "error", "Enter an amount like 1, 1.5 or 1 1/2." }
        }
        h3 { class: "section-title", "Impact on today's targets" }
        div { class: "impact",
            for (label , fraction) in impact {
                div { key: "{label}", class: "impact-item",
                    super::Ring { fraction, size: 48 }
                    span { class: "impact-pct", "{fraction * 100.0:.0}%" }
                    span { class: "muted", "{label}" }
                }
            }
        }
        h3 { class: "section-title", "Nutrients" }
        table { class: "nutrients",
            tbody {
                for (name , value) in nutrient_rows {
                    tr { key: "{name}",
                        td { "{name}" }
                        td { class: "num", "{value}" }
                    }
                }
            }
        }
        button {
            class: "primary wide",
            disabled: item.is_none(),
            onclick: move |_| {
                if let Some(item) = item.clone() {
                    state.add_to_plate(item);
                    on_back.call(());
                }
            },
            "Add to plate"
        }
    }
}

fn parse_number(s: &str) -> Option<f64> {
    s.trim()
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v >= 0.0)
}

#[component]
fn QuickAdd() -> Element {
    let state = use_context::<AppState>();
    let mut name = use_signal(String::new);
    let kcal = use_signal(String::new);
    let protein = use_signal(String::new);
    let fat = use_signal(String::new);
    let carbs = use_signal(String::new);

    let p = parse_number(&protein()).unwrap_or(0.0);
    let f = parse_number(&fat()).unwrap_or(0.0);
    let c = parse_number(&carbs()).unwrap_or(0.0);
    let macro_kcal = p * KCAL_PER_G_PROTEIN + f * KCAL_PER_G_FAT + c * KCAL_PER_G_CARBS;
    let energy = parse_number(&kcal()).or((macro_kcal > 0.0).then_some(macro_kcal));
    let hint = format!("Macros add up to {} kcal", fmt0(macro_kcal));

    rsx! {
        label { class: "field",
            span { "Name (optional)" }
            input {
                r#type: "text",
                placeholder: "Quick add",
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
        p { class: "muted pad", "{hint}" }
        button {
            class: "primary wide",
            disabled: energy.is_none(),
            onclick: move |_| {
                let Some(energy) = energy else { return };
                let label = name();
                state
                    .add_to_plate(PlateItem {
                        name: if label.trim().is_empty() { "Quick add".into() } else { label.trim().to_string() },
                        amount: "1 entry".into(),
                        nutrients: NutrientProfile::from_macros(energy, p, f, c),
                    });
                for mut field in [name, kcal, protein, fat, carbs] {
                    field.set(String::new());
                }
            },
            "Add to plate"
        }
    }
}

#[component]
fn NumberField(label: &'static str, unit: &'static str, value: Signal<String>) -> Element {
    let mut value = value;
    rsx! {
        label { class: "field",
            span { "{label}" }
            div { class: "field-input",
                input {
                    r#type: "text",
                    "inputmode": "decimal",
                    aria_label: "{label} ({unit})",
                    value: "{value}",
                    oninput: move |e| value.set(e.value()),
                }
                span { class: "unit", "{unit}" }
            }
        }
    }
}
