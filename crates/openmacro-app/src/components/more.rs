//! Profile, data management and about.

use dioxus::prelude::*;
use openmacro_core::bmr::{self, ActivityLevel, BmrEquation, Sex};

use crate::export;
use crate::food_search::{self, Database, FoodRegion};
use crate::prefs::{EnergyUnit, PREFS, Theme, WeightUnit};
use crate::state::{AppState, CoachSnapshot, Data, today};

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
    let prior_text = format!("{}/day", crate::prefs::energy_text(prior));
    let weight_label: &'static str = if crate::prefs::weight_symbol() == "lb" {
        "Weight (lb)"
    } else {
        "Weight (kg)"
    };
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
                    label: weight_label,
                    value: (crate::prefs::weight_value(stats.weight_kg) * 10.0).round() / 10.0,
                    on_change: move |v| state.profile.write().stats.weight_kg = crate::prefs::kg_from_input(v),
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
        DisplaySettings {}
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
            BackupData {}
            ExportData {}
        }
        section { class: "card",
            h2 { "About" }
            super::update::UpdateCheck {}
            p { class: "muted small",
                "An independent, open-source nutrition tracker. Not affiliated with any commercial app."
            }
            a { href: "https://github.com/SilverCrocus/OpenMacro", target: "_blank", "github.com/SilverCrocus/OpenMacro" }
        }
    }
}

/// Asks for a file and returns its text, or null if none was picked. The
/// input lives outside the Dioxus tree: Dioxus intercepts clicks on its own
/// file inputs to open a desktop dialog, which Android doesn't have, while
/// a plain input gets the WebView's file chooser.
#[cfg(not(target_arch = "wasm32"))]
const PICK_BACKUP_JS: &str = "
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.json,application/json';
    input.style.display = 'none';
    document.body.appendChild(input);
    const text = await new Promise((resolve) => {
        input.addEventListener('change', async () => {
            const file = input.files[0];
            resolve(file ? await file.text() : null);
        }, { once: true });
        input.addEventListener('cancel', () => resolve(null), { once: true });
        input.click();
    });
    input.remove();
    return text;
";

/// Saving everything to one backup file, and restoring from one.
#[cfg(not(target_arch = "wasm32"))]
#[component]
fn BackupData() -> Element {
    use crate::backup::BackupFile;

    let mut state = use_context::<AppState>();
    let mut busy = use_signal(|| false);
    let mut outcome = use_signal(|| None::<Result<String, String>>);
    let mut pending = use_signal(|| None::<BackupFile>);
    let pending_summary = pending.read().as_ref().map(BackupFile::summary);

    rsx! {
        h3 { class: "section-title", "Backup" }
        p { class: "muted small",
            "Save everything to one file: food log, weigh-ins, check-ins, your foods and recipes, favorites and settings. Keep it somewhere safe, such as Google Drive, to restore on a new phone."
        }
        div { class: "button-row",
            button {
                disabled: busy(),
                onclick: move |_| {
                    busy.set(true);
                    outcome.set(None);
                    let snapshot = state.persist.read().snapshot();
                    spawn(async move {
                        let result = match snapshot {
                            Some(data) => {
                                let file = BackupFile::new(data, chrono::Utc::now());
                                export::save(vec![export::ExportFile {
                                    name: file.file_name(),
                                    mime: "application/json",
                                    contents: file.to_json(),
                                }])
                                .await
                            }
                            None => Err("Couldn't read your data.".into()),
                        };
                        outcome.set(Some(result));
                        busy.set(false);
                    });
                },
                "Save backup"
            }
            button {
                onclick: move |_| {
                    outcome.set(None);
                    spawn(async move {
                        let text = document::eval(PICK_BACKUP_JS).await;
                        if let Ok(serde_json::Value::String(text)) = text {
                            match BackupFile::parse(&text) {
                                Ok(backup) => pending.set(Some(backup)),
                                Err(message) => outcome.set(Some(Err(message))),
                            }
                        }
                    });
                },
                "Restore from file"
            }
        }
        if let Some(summary) = pending_summary {
            div { class: "restore-confirm",
                p { "{summary}" }
                p { class: "error", "Restoring replaces everything on this device with the backup. This can't be undone." }
                div { class: "button-row",
                    button { onclick: move |_| pending.set(None), "Cancel" }
                    button {
                        class: "danger",
                        onclick: move |_| {
                            let Some(backup) = pending.take() else { return };
                            let result = state.persist.write().restore(&backup.data);
                            if result.is_ok() {
                                state.reload();
                            }
                            outcome.set(Some(result.map(|_| "Backup restored.".to_string())));
                        },
                        "Replace everything"
                    }
                }
            }
        }
        match outcome() {
            Some(Ok(message)) => rsx! {
                p { class: "muted small", "{message}" }
            },
            Some(Err(message)) => rsx! {
                p { class: "error", "{message}" }
            },
            None => rsx! {},
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[component]
fn BackupData() -> Element {
    rsx! {
        h3 { class: "section-title", "Backup" }
        p { class: "muted small", "Backups are available in the Android app." }
    }
}

/// Exports the user's history as CSV files.
#[component]
fn ExportData() -> Element {
    let state = use_context::<AppState>();
    let coach = use_context::<Memo<CoachSnapshot>>();
    let mut busy = use_signal(|| false);
    let mut outcome = use_signal(|| None::<Result<String, String>>);
    let empty = state.is_empty();

    rsx! {
        h3 { class: "section-title", "Export" }
        p { class: "muted small",
            "Your food log, weigh-ins and a daily summary (intake, weight trend, expenditure and targets) as CSV files, for spreadsheets or other apps."
        }
        button {
            disabled: busy() || empty,
            onclick: move |_| {
                busy.set(true);
                outcome.set(None);
                let files = export::files(&state.data(), &coach.read(), today());
                spawn(async move {
                    outcome.set(Some(export::save(files).await));
                    busy.set(false);
                });
            },
            if busy() { "Exporting…" } else { "Export CSV files" }
        }
        match outcome() {
            Some(Ok(message)) => rsx! {
                p { class: "muted small", "{message}" }
            },
            Some(Err(message)) => rsx! {
                p { class: "error", "{message}" }
            },
            None => rsx! {},
        }
    }
}

/// Theme, units and clock.
#[component]
fn DisplaySettings() -> Element {
    let prefs = PREFS();
    rsx! {
        section { class: "card",
            h2 { "Display" }
            div { class: "choice",
                span { class: "choice-label", "Theme" }
                div { class: "segmented",
                    for theme in Theme::ALL {
                        button {
                            key: "{theme.label()}",
                            class: if prefs.theme == theme { "active" },
                            onclick: move |_| PREFS.write().theme = theme,
                            "{theme.label()}"
                        }
                    }
                }
            }
            div { class: "choice",
                span { class: "choice-label", "Energy" }
                div { class: "segmented",
                    for unit in [EnergyUnit::Kilocalories, EnergyUnit::Kilojoules] {
                        button {
                            key: "{unit.symbol()}",
                            class: if prefs.energy == unit { "active" },
                            onclick: move |_| PREFS.write().energy = unit,
                            "{unit.symbol()}"
                        }
                    }
                }
            }
            div { class: "choice",
                span { class: "choice-label", "Weight" }
                div { class: "segmented",
                    for unit in [WeightUnit::Kilograms, WeightUnit::Pounds] {
                        button {
                            key: "{unit.symbol()}",
                            class: if prefs.weight == unit { "active" },
                            onclick: move |_| PREFS.write().weight = unit,
                            "{unit.symbol()}"
                        }
                    }
                }
            }
            div { class: "choice",
                span { class: "choice-label", "Clock" }
                div { class: "segmented",
                    button {
                        class: if !prefs.clock_24h { "active" },
                        onclick: move |_| PREFS.write().clock_24h = false,
                        "12-hour"
                    }
                    button {
                        class: if prefs.clock_24h { "active" },
                        onclick: move |_| PREFS.write().clock_24h = true,
                        "24-hour"
                    }
                }
            }
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
