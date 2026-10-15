//! The logging sheet: stage foods on a plate, then log them together.

use std::rc::Rc;

use dioxus::prelude::*;
use openmacro_core::food::Food;
use openmacro_core::nutrients::{
    KCAL_PER_G_CARBS, KCAL_PER_G_FAT, KCAL_PER_G_PROTEIN, Nutrient, NutrientProfile,
};

use super::{Icon, fmt1, hour_label, macro_line};
use crate::back::use_back_handler;
use crate::food_search::{self, Database, FoundFood};
use crate::foods::plate_item;
use crate::keypad::{self, Key};
use crate::prefs;
use crate::state::{AppState, CoachSnapshot, LogEntry, PlateItem, day_totals};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Tab {
    Search,
    Scan,
    Ai,
    QuickAdd,
    Library,
}

#[component]
pub fn Logger() -> Element {
    let mut state = use_context::<AppState>();
    let coach = use_context::<Memo<CoachSnapshot>>();
    // Back keeps the plate for later; editing a recipe is simply cancelled.
    use_back_handler(move |_| {
        if (state.recipe_edit)().is_some() {
            state.close_plate();
        } else {
            state.hide_plate();
        }
    });
    let mut confirm_discard = use_signal(|| false);
    // A plate item being re-measured: its index, food and current amount.
    let mut item_edit = use_signal(|| None::<(usize, Food, LogEntry)>);
    let foods_db = use_context::<Resource<Option<Database>>>();
    // A food being copied into a new custom food.
    let mut copying_food = use_signal(|| None::<Food>);
    let mut tab = use_signal(|| Tab::Search);
    let mut detail = use_signal(|| None::<Food>);
    // Set when a scanned barcode isn't found, to prefill a new custom food.
    let mut new_barcode = use_signal(|| None::<String>);
    let query = use_signal(String::new);
    let mut saving_recipe = use_signal(|| false);

    let plate = state.plate.read().clone();
    let plate_kcal = plate.totals().get_or_zero(Nutrient::Energy);
    let day_kcal = day_totals(&state.log.read(), plate.date).get_or_zero(Nutrient::Energy);
    let target_kcal = coach.read().target_for(plate.date).kcal;
    let total_text = format!(
        "{} / {} {}",
        prefs::energy(day_kcal + plate_kcal),
        prefs::energy(target_kcal),
        prefs::energy_symbol()
    );
    let date_text = plate.date.format("%a %-d %b").to_string();
    let count = plate.items.len();
    // Set while a recipe's ingredients are on the plate for editing.
    let recipe_edit = (state.recipe_edit)();
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
                        onclick: move |_| {
                            if count > 0 && recipe_edit.is_none() {
                                confirm_discard.set(true);
                            } else {
                                state.close_plate();
                            }
                        },
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
                if confirm_discard() {
                    div { class: "discard-confirm", role: "alert",
                        span {
                            if count == 1 { "Discard the item on your plate?" } else { "Discard the {count} items on your plate?" }
                        }
                        button { onclick: move |_| confirm_discard.set(false), "Keep" }
                        button {
                            class: "danger",
                            onclick: move |_| {
                                confirm_discard.set(false);
                                state.close_plate();
                            },
                            "Discard"
                        }
                    }
                }
                if count > 0 {
                    PlateView {
                        items: plate.items.clone(),
                        meal: plate.meal.clone(),
                        on_edit: move |index: usize| {
                            let plate = state.plate.read();
                            let Some(item) = plate.items.get(index) else { return };
                            // Foods made up on the spot (AI estimates) are rebuilt
                            // from the item itself.
                            let found = item.food_id.as_deref().and_then(|id| {
                                let guard = foods_db.read();
                                let db = guard.as_ref().and_then(Option::as_ref);
                                food_search::resolve(db, &state.custom_foods.read(), id)
                                    .or_else(|| food_search::rebuild(id, &item.name, &item.amount, &item.nutrients))
                            });
                            if let Some(found) = found {
                                let entry = LogEntry {
                                    id: 0,
                                    date: plate.date,
                                    hour: plate.hour,
                                    name: item.name.clone(),
                                    amount: item.amount.clone(),
                                    nutrients: item.nutrients.clone(),
                                    food_id: item.food_id.clone(),
                                    meal: None,
                                };
                                item_edit.set(Some((index, found.food, entry)));
                            }
                        },
                    }
                }
                if !saving_recipe() {
                    nav { class: "sheet-tabs",
                        for (value , label) in [
                            (Tab::Search, "Search"),
                            (Tab::Scan, "Scan"),
                            (Tab::Ai, "AI"),
                            (Tab::QuickAdd, "Quick Add"),
                            (Tab::Library, "Library"),
                        ]
                        {
                            button {
                                key: "{label}",
                                class: if tab() == value && detail().is_none() { "active" },
                                onclick: move |_| {
                                    detail.set(None);
                                    tab.set(value);
                                },
                                "{label}"
                            }
                        }
                    }
                }
                div { class: "sheet-body",
                    if saving_recipe() {
                        super::recipe::RecipeForm {
                            items: plate.items.clone(),
                            existing: recipe_edit.clone(),
                            on_done: move |_| saving_recipe.set(false),
                        }
                    } else {
                        if let Some(food) = copying_food() {
                            super::library::CustomFoodForm {
                                barcode: String::new(),
                                template: Some(food),
                                on_done: move |_| {
                                    copying_food.set(None);
                                    detail.set(None);
                                    tab.set(Tab::Library);
                                },
                            }
                        } else {
                        match (item_edit(), detail(), tab()) {
                            (Some((index, food, entry)), _, _) => rsx! {
                                FoodDetail {
                                    key: "plate-{index}",
                                    food,
                                    editing: Some(entry),
                                    on_back: move |_| item_edit.set(None),
                                    on_save: move |item| {
                                        state.replace_plate_item(index, item);
                                        item_edit.set(None);
                                    },
                                }
                            },
                            (None, Some(food), _) => rsx! {
                                FoodDetail {
                                    food,
                                    on_back: move |_| detail.set(None),
                                    on_copy: move |food| copying_food.set(Some(food)),
                                }
                            },
                            (None, None, Tab::Search) => rsx! {
                                SearchTab { query, on_select: move |food| detail.set(Some(food)) }
                            },
                            (None, None, Tab::Scan) => rsx! {
                                super::scan::ScanTab {
                                    on_found: move |food| detail.set(Some(food)),
                                    on_create: move |code| {
                                        new_barcode.set(Some(code));
                                        tab.set(Tab::Library);
                                    },
                                }
                            },
                            (None, None, Tab::Library) => rsx! {
                                super::library::LibraryTab { on_select: move |food| detail.set(Some(food)), new_barcode }
                            },
                            (None, None, Tab::Ai) => rsx! {
                                super::ai_log::AiTab {}
                            },
                            (None, None, Tab::QuickAdd) => rsx! {
                                QuickAdd {}
                            },
                        }
                        }
                    }
                }
                if recipe_edit.is_some() && detail().is_none() && !saving_recipe() {
                    footer { class: "sheet-footer plate-actions",
                        button { onclick: move |_| state.close_plate(), "Cancel" }
                        button {
                            class: "primary",
                            disabled: count == 0,
                            onclick: move |_| saving_recipe.set(true),
                            "Update recipe"
                        }
                    }
                } else if count > 0 && detail().is_none() && !saving_recipe() {
                    footer { class: "sheet-footer plate-actions",
                        if count > 1 {
                            button { onclick: move |_| saving_recipe.set(true), "Save as recipe" }
                        }
                        button { class: "primary", onclick: move |_| state.log_plate(), "{log_text}" }
                    }
                }
            }
        }
    }
}

#[component]
fn PlateView(items: Vec<PlateItem>, meal: Option<String>, on_edit: EventHandler<usize>) -> Element {
    let state = use_context::<AppState>();
    let totals: NutrientProfile = items.iter().map(|i| &i.nutrients).sum();
    let summary = format!(
        "{} · {}",
        prefs::energy_text(totals.get_or_zero(Nutrient::Energy)),
        macro_line(&totals)
    );
    rsx! {
        section { class: "plate",
            div { class: "plate-head",
                h3 { class: "plate-title",
                    if (state.recipe_edit)().is_some() {
                        "Ingredients"
                    } else if let Some(meal) = &meal {
                        "{meal}"
                    } else {
                        "Plate"
                    }
                }
                span { class: "muted", "{summary}" }
            }
            for (index , item) in items.into_iter().enumerate() {
                div { key: "{index}", class: "plate-item",
                    button {
                        class: "entry-main entry-open",
                        aria_label: "Change the amount of {item.name}",
                        disabled: item.food_id.is_none(),
                        onclick: move |_| on_edit.call(index),
                        div { class: "entry-name", "{item.name}" }
                        div { class: "entry-sub", {format!("{} · {}", item.amount, macro_line(&item.nutrients))} }
                    }
                    div { class: "entry-kcal", {prefs::energy(item.nutrients.get_or_zero(Nutrient::Energy))} }
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
    let recent = if q.trim().is_empty() {
        food_search::recent_foods(db, &state.custom_foods.read(), &state.log.read(), 15)
    } else {
        Vec::new()
    };
    let results = if q.trim().is_empty() {
        food_search::Results::default()
    } else {
        let mut history = food_search::history_counts(&state.log.read());
        // Favorites rank above everything you've merely logged before.
        for favorite in state.favorites.read().iter() {
            *history.entry(favorite.id.clone()).or_insert(0) += 1_000;
        }
        food_search::search(db, &state.custom_foods.read(), &history, &q, region)
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
            if !recent.is_empty() {
                ResultSection { title: "Recent", foods: recent, on_select }
            }
        } else if results.is_empty() {
            p { class: "muted pad", "No matches. Try another spelling or Quick Add." }
        }
        if !results.yours.is_empty() {
            ResultSection { key: "Your foods-{q}", title: "Your foods", foods: results.yours, on_select }
        }
        if !results.common.is_empty() {
            ResultSection { key: "Common-{q}", title: "Common", foods: results.common, on_select }
        }
        if !results.branded.is_empty() {
            ResultSection { key: "Branded-{q}", title: "Branded", foods: results.branded, on_select }
        }
    }
}

#[component]
fn ResultSection(
    title: &'static str,
    foods: Vec<FoundFood>,
    on_select: EventHandler<Food>,
) -> Element {
    /// Results shown before "See more".
    const SHOWN: usize = 8;
    let state = use_context::<AppState>();
    let mut expanded = use_signal(|| false);
    let total = foods.len();
    let hidden = if expanded() {
        0
    } else {
        total.saturating_sub(SHOWN)
    };
    rsx! {
        h3 { class: "section-title result-section", "{title}" }
        ul { class: "results",
            for (i , found) in foods.into_iter().take(total - hidden).enumerate() {
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
        if hidden > 0 {
            button { class: "link-btn see-more", onclick: move |_| expanded.set(true), "See {hidden} more" }
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
                "{} · {} · {}",
                prefs::energy_text(n.get_or_zero(Nutrient::Energy)),
                macro_line(&n),
                label
            )
        }
        _ => String::new(),
    }
}

#[component]
pub(super) fn FoodDetail(
    food: Food,
    on_back: EventHandler<()>,
    /// A logged entry of this food to edit: the keypad starts at its amount
    /// and `on_save` replaces Log and Add.
    #[props(default)]
    editing: Option<LogEntry>,
    #[props(default)] on_save: Option<EventHandler<PlateItem>>,
    /// Offers "Copy to my foods", to make an editable copy.
    #[props(default)]
    on_copy: Option<EventHandler<Food>>,
) -> Element {
    let state = use_context::<AppState>();
    use_back_handler(move |_| on_back.call(()));
    let coach = use_context::<Memo<CoachSnapshot>>();
    let food = Rc::new(food);
    let units = keypad::units(&food);
    let start = editing
        .as_ref()
        .and_then(|e| keypad::amount_from_label(&food, &e.amount));
    // Foods without named servings start at 100 g rather than 1 g.
    let default_qty = if food.servings.is_empty() { "100" } else { "1" };
    let mut qty = use_signal(|| {
        start
            .as_ref()
            .map_or_else(|| default_qty.to_string(), |(q, _)| q.clone())
    });
    let mut fresh = use_signal(|| true);
    let mut unit = use_signal(|| start.as_ref().map_or(units[0], |(_, u)| *u));

    let item = keypad::item_for(&food, &qty(), unit());
    let date = editing
        .as_ref()
        .map_or_else(|| state.plate.read().date, |e| e.date);
    let target = coach.read().target_for(date);

    let per = item
        .as_ref()
        .map(|i| i.nutrients.clone())
        .unwrap_or_default();
    let kcal = per.get_or_zero(Nutrient::Energy);
    // Share of the energy coming from each macro. Carbs use net carbs: fibre
    // contributes little energy, and including it pushes the total past 100%.
    let protein_kcal = per.get_or_zero(Nutrient::Protein) * KCAL_PER_G_PROTEIN;
    let fat_kcal = per.get_or_zero(Nutrient::Fat) * KCAL_PER_G_FAT;
    let carbs_kcal = per.net_carbs().unwrap_or(0.0) * KCAL_PER_G_CARBS;
    let macro_total = protein_kcal + fat_kcal + carbs_kcal;
    let share = |part: f64| {
        if macro_total > 0.0 {
            part / macro_total * 100.0
        } else {
            0.0
        }
    };
    let headline = [
        (
            "Protein",
            per.get_or_zero(Nutrient::Protein),
            share(protein_kcal),
            "protein",
        ),
        (
            "Fat",
            per.get_or_zero(Nutrient::Fat),
            share(fat_kcal),
            "fat",
        ),
        (
            "Carbs",
            per.get_or_zero(Nutrient::Carbs),
            share(carbs_kcal),
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
                if n == Nutrient::Energy {
                    format!(
                        "{} {}",
                        fmt1(prefs::energy_value(v)),
                        prefs::energy_symbol()
                    )
                } else {
                    format!("{} {}", fmt1(v), n.unit().symbol())
                },
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
    let save = item.clone();
    // Logging straight away would log a recipe's ingredients being edited.
    let can_log = (state.recipe_edit)().is_none();

    rsx! {
        button { class: "link-btn", onclick: move |_| on_back.call(()),
            Icon { name: "left" }
            if editing.is_some() { "Cancel" } else { "Results" }
        }
        div { class: "detail-title-row",
            h2 { class: "detail-title", "{food.name}" }
            if !food.id.is_empty() {
                {
                    let starred = state.is_favorite(&food.id);
                    let star_food = food.clone();
                    rsx! {
                        button {
                            class: if starred { "icon-btn star active" } else { "icon-btn star" },
                            aria_label: if starred { "Remove from favorites" } else { "Add to favorites" },
                            aria_pressed: "{starred}",
                            onclick: move |_| state.toggle_favorite(&star_food),
                            if starred { "★" } else { "☆" }
                        }
                    }
                }
            }
        }
        if let Some(brand) = &food.brand {
            p { class: "muted detail-brand", "{brand}" }
        }
        if let Some(on_copy) = on_copy {
            {
                let copy = (*food).clone();
                rsx! {
                    button {
                        class: "link-btn copy-food",
                        onclick: move |_| on_copy.call(copy.clone()),
                        Icon { name: "copy" }
                        "Copy to my foods"
                    }
                }
            }
        }
        div { class: "detail-headline",
            div { class: "big-kcal",
                span { class: "num", {prefs::energy(kcal)} }
                span { class: "muted", {prefs::energy_symbol()} }
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
                if let Some(on_save) = on_save {
                    button {
                        class: "key-add key-save",
                        disabled: save.is_none(),
                        onclick: move |_| {
                            if let Some(item) = save.clone() {
                                on_save.call(item);
                            }
                        },
                        "Save"
                    }
                } else {
                    if can_log {
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
                    }
                    button {
                        class: if can_log { "key-add" } else { "key-add key-save" },
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
    let alcohol = use_signal(String::new);

    let p = parse_number(&protein()).unwrap_or(0.0);
    let f = parse_number(&fat()).unwrap_or(0.0);
    let c = parse_number(&carbs()).unwrap_or(0.0);
    let a = parse_number(&alcohol()).unwrap_or(0.0);
    let macro_kcal = p * KCAL_PER_G_PROTEIN
        + f * KCAL_PER_G_FAT
        + c * KCAL_PER_G_CARBS
        + a * openmacro_core::nutrients::KCAL_PER_G_ALCOHOL;
    let energy = parse_number(&kcal())
        .map(prefs::kcal_from_input)
        .or((macro_kcal > 0.0).then_some(macro_kcal));
    let hint = format!("Macros add up to {}", prefs::energy_text(macro_kcal));

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
            NumberField { label: "Energy", unit: prefs::energy_symbol(), value: kcal }
            NumberField { label: "Protein", unit: "g", value: protein }
            NumberField { label: "Fat", unit: "g", value: fat }
            NumberField { label: "Carbs", unit: "g", value: carbs }
            NumberField { label: "Alcohol", unit: "g", value: alcohol }
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
                        food_id: None,
                        name: if label.trim().is_empty() { "Quick add".into() } else { label.trim().to_string() },
                        amount: "1 entry".into(),
                        nutrients: {
                            let n = NutrientProfile::from_macros(energy, p, f, c);
                            if a > 0.0 { n.with(Nutrient::Alcohol, a) } else { n }
                        },
                        grams: None,
                    });
                for mut field in [name, kcal, protein, fat, carbs, alcohol] {
                    field.set(String::new());
                }
            },
            "Add to plate"
        }
    }
}

#[component]
pub(super) fn NumberField(
    label: &'static str,
    unit: &'static str,
    value: Signal<String>,
) -> Element {
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
