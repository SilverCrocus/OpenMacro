//! The logging sheet: stage foods on a plate, then log them together.

use std::rc::Rc;

use dioxus::prelude::*;
use openmacro_core::food::Food;
use openmacro_core::nutrients::{
    KCAL_PER_G_CARBS, KCAL_PER_G_FAT, KCAL_PER_G_PROTEIN, Nutrient, NutrientProfile,
};

use super::{Icon, fmt0, fmt1, hour_label, macro_line};
use crate::food_search::{self, Database, FoundFood};
use crate::foods::plate_item;
use crate::keypad::{self, Key};
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
    let mut detail = use_signal(|| None::<Food>);
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
                            (Tab::Search, Some(food)) => rsx! {
                                FoodDetail { food, on_back: move |_| detail.set(None) }
                            },
                            (Tab::Search, None) => rsx! {
                                SearchTab { query, on_select: move |food| detail.set(Some(food)) }
                            },
                            (Tab::QuickAdd, _) => rsx! {
                                QuickAdd {}
                            },
                        }
                    }
                }
                if count > 0 && detail().is_none() {
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
fn SearchTab(query: Signal<String>, on_select: EventHandler<Food>) -> Element {
    let state = use_context::<AppState>();
    let foods = use_context::<Resource<Option<Database>>>();
    let mut query = query;
    let q = query();
    let region = (state.region)();
    let guard = foods.read();
    let db = guard.as_ref().and_then(Option::as_ref);
    let has_db = db.is_some();
    let results = if q.trim().is_empty() {
        food_search::Results::default()
    } else {
        food_search::search(db, &q, region)
    };
    let hint = if has_db {
        format!(
            "Search {} foods from USDA, AFCD and Open Food Facts.",
            food_search::food_count(db)
        )
    } else {
        "Search the built-in sample foods.".to_string()
    };
    drop(guard);

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
            p { class: "muted pad", "{hint}" }
        } else if results.is_empty() {
            p { class: "muted pad", "No matches. Try another spelling or Quick Add." }
        }
        if !results.common.is_empty() {
            ResultSection { title: "Common", foods: results.common, on_select }
        }
        if !results.branded.is_empty() {
            ResultSection { title: "Branded", foods: results.branded, on_select }
        }
    }
}

#[component]
fn ResultSection(
    title: &'static str,
    foods: Vec<FoundFood>,
    on_select: EventHandler<Food>,
) -> Element {
    let state = use_context::<AppState>();
    rsx! {
        h3 { class: "section-title result-section", "{title}" }
        ul { class: "results",
            for (i , found) in foods.into_iter().enumerate() {
                {
                    let food = Rc::new(found.food);
                    let origin = match &food.brand {
                        Some(brand) => format!("{brand} · {}", found.source),
                        None => found.source.to_string(),
                    };
                    let (open, add) = (food.clone(), food.clone());
                    rsx! {
                        li { key: "{title}{i}", class: "result",
                            button { class: "result-main", onclick: move |_| on_select.call((*open).clone()),
                                div { class: "entry-name", "{food.name}" }
                                div { class: "entry-sub", {default_summary(&food)} }
                                div { class: "entry-origin", "{origin}" }
                            }
                            button {
                                class: "icon-btn add",
                                aria_label: "Add {food.name} to plate",
                                onclick: move |_| {
                                    if let Some(item) = plate_item(&add, add.default_amount()) {
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
fn FoodDetail(food: Food, on_back: EventHandler<()>) -> Element {
    let state = use_context::<AppState>();
    let coach = use_context::<Memo<CoachSnapshot>>();
    let food = Rc::new(food);
    let units = keypad::units(&food);
    let mut qty = use_signal(|| "1".to_string());
    let mut fresh = use_signal(|| true);
    let mut unit = use_signal(|| units[0]);

    let item = keypad::item_for(&food, &qty(), unit());
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

    let shown_qty = if qty().is_empty() {
        "0".to_string()
    } else {
        qty()
    };
    let amount_detail = match &item {
        Some(i) => i.amount.clone(),
        None => "Enter an amount like 1, 1.5 or 1 1/2".to_string(),
    };
    let mut press = move |key: Key| {
        qty.set(keypad::apply_key(&qty(), key, fresh()));
        fresh.set(false);
    };
    let keys: [(&str, &str, Key); 12] = [
        ("1", "1", Key::Digit(1)),
        ("2", "2", Key::Digit(2)),
        ("3", "3", Key::Digit(3)),
        ("/", "Fraction", Key::Slash),
        ("4", "4", Key::Digit(4)),
        ("5", "5", Key::Digit(5)),
        ("6", "6", Key::Digit(6)),
        ("␣", "Space", Key::Space),
        ("7", "7", Key::Digit(7)),
        ("8", "8", Key::Digit(8)),
        ("9", "9", Key::Digit(9)),
        ("⌫", "Delete", Key::Backspace),
    ];
    let add = item.clone();
    let log_now = item.clone();

    rsx! {
        button { class: "link-btn", onclick: move |_| on_back.call(()),
            Icon { name: "left" }
            "Results"
        }
        h2 { class: "detail-title", "{food.name}" }
        if let Some(brand) = &food.brand {
            p { class: "muted detail-brand", "{brand}" }
        }
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
        div { class: "keypad-dock",
            div { class: "amount-display", "aria-live": "polite",
                span { class: if fresh() { "qty-text fresh" } else { "qty-text" }, "{shown_qty}" }
                span { class: if item.is_some() { "amount-detail" } else { "amount-detail error" }, "{amount_detail}" }
            }
            div { class: "unit-chips", role: "radiogroup", aria_label: "Unit",
                for u in units {
                    {
                        let chip_food = food.clone();
                        let label = keypad::unit_label(&food, u);
                        rsx! {
                            button {
                                key: "{u:?}",
                                role: "radio",
                                aria_checked: "{unit() == u}",
                                class: if unit() == u { "chip active" } else { "chip" },
                                onclick: move |_| {
                                    let from = unit();
                                    if from != u {
                                        if let Some(converted) = keypad::convert(&chip_food, &qty(), from, u) {
                                            qty.set(converted);
                                        }
                                        unit.set(u);
                                        fresh.set(true);
                                    }
                                },
                                "{label}"
                            }
                        }
                    }
                }
            }
            div { class: "keypad",
                for (text , label , key) in keys {
                    button { key: "{label}", aria_label: "{label}", onclick: move |_| press(key), "{text}" }
                }
                button { aria_label: "Decimal point", onclick: move |_| press(Key::Dot), "." }
                button { aria_label: "0", onclick: move |_| press(Key::Digit(0)), "0" }
                button {
                    class: "key-log",
                    disabled: log_now.is_none(),
                    onclick: move |_| {
                        if let Some(item) = log_now.clone() {
                            state.add_to_plate(item);
                            state.log_plate();
                        }
                    },
                    "Log"
                }
                button {
                    class: "key-add",
                    disabled: add.is_none(),
                    onclick: move |_| {
                        if let Some(item) = add.clone() {
                            state.add_to_plate(item);
                            on_back.call(());
                        }
                    },
                    "Add"
                }
            }
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
