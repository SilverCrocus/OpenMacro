//! Display preferences: theme, clock and units. Kept in a global signal so
//! formatting helpers anywhere can follow them.

use dioxus::prelude::*;
pub use openmacro_core::units::{EnergyUnit, WeightUnit};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(
    not(target_arch = "wasm32"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub enum Theme {
    #[default]
    Dark,
    Light,
    /// Follow the phone's light or dark setting.
    System,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::Dark, Theme::Light, Theme::System];

    pub fn label(self) -> &'static str {
        match self {
            Theme::Dark => "Dark",
            Theme::Light => "Light",
            Theme::System => "System",
        }
    }

    /// The `data-theme` value the stylesheet keys on.
    fn code(self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
            Theme::System => "system",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(
    not(target_arch = "wasm32"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub enum FoodEntryTab {
    #[default]
    Search,
    Scan,
    Ai,
    QuickAdd,
    Library,
}

impl FoodEntryTab {
    pub const ALL: [Self; 5] = [
        Self::Search,
        Self::Scan,
        Self::Ai,
        Self::QuickAdd,
        Self::Library,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Search => "Search",
            Self::Scan => "Scan",
            Self::Ai => "AI",
            Self::QuickAdd => "Quick Add",
            Self::Library => "Library",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    not(target_arch = "wasm32"),
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct Prefs {
    pub theme: Theme,
    pub clock_24h: bool,
    pub default_food_tab: FoodEntryTab,
    pub energy: EnergyUnit,
    pub weight: WeightUnit,
    /// The food log's first and last hours (earlier or later entries
    /// still show).
    pub log_first_hour: u32,
    pub log_last_hour: u32,
    /// Show each hour's calories beside its label.
    pub hour_totals: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            clock_24h: false,
            default_food_tab: FoodEntryTab::default(),
            energy: EnergyUnit::default(),
            weight: WeightUnit::default(),
            log_first_hour: 6,
            log_last_hour: 23,
            hour_totals: false,
        }
    }
}

pub static PREFS: GlobalSignal<Prefs> = Signal::global(Prefs::default);

// Everything is stored in kcal and kg; these convert for display and input.

fn energy_unit() -> EnergyUnit {
    PREFS.read().energy
}

fn weight_unit() -> WeightUnit {
    PREFS.read().weight
}

/// "kcal" or "kJ".
pub fn energy_symbol() -> &'static str {
    energy_unit().symbol()
}

/// Energy in the user's unit, as a number.
pub fn energy_value(kcal: f64) -> f64 {
    energy_unit().from_kcal(kcal)
}

/// Energy in the user's unit, rounded, without the unit: "1894".
pub fn energy(kcal: f64) -> String {
    crate::components::unsigned_zero(format!("{:.0}", energy_value(kcal)))
}

/// Energy with its unit: "1894 kcal".
pub fn energy_text(kcal: f64) -> String {
    format!("{} {}", energy(kcal), energy_symbol())
}

/// A number typed in the user's energy unit, in kcal.
pub fn kcal_from_input(value: f64) -> f64 {
    energy_unit().to_kcal(value)
}

/// "kg" or "lb".
pub fn weight_symbol() -> &'static str {
    weight_unit().symbol()
}

/// Weight in the user's unit, as a number.
pub fn weight_value(kg: f64) -> f64 {
    weight_unit().from_kg(kg)
}

/// Weight in the user's unit with `decimals` places, without the unit.
pub fn weight(kg: f64, decimals: usize) -> String {
    crate::components::unsigned_zero(format!("{:.decimals$}", weight_value(kg)))
}

/// Weight with its unit: "70.6 kg".
pub fn weight_text(kg: f64, decimals: usize) -> String {
    format!("{} {}", weight(kg, decimals), weight_symbol())
}

/// A number typed in the user's weight unit, in kg.
pub fn kg_from_input(value: f64) -> f64 {
    weight_unit().to_kg(value)
}

/// Lengths follow the weight unit: inches with pounds, else centimetres.
fn uses_inches() -> bool {
    weight_unit() == WeightUnit::Pounds
}

/// "cm" or "in".
pub fn length_symbol() -> &'static str {
    if uses_inches() { "in" } else { "cm" }
}

pub fn length_value(cm: f64) -> f64 {
    if uses_inches() {
        cm / openmacro_core::units::CM_PER_INCH
    } else {
        cm
    }
}

pub fn cm_from_input(value: f64) -> f64 {
    if uses_inches() {
        value * openmacro_core::units::CM_PER_INCH
    } else {
        value
    }
}

/// Applies the theme to the page.
pub fn apply_theme(theme: Theme) {
    document::eval(&format!(
        "document.documentElement.dataset.theme = '{}';",
        theme.code()
    ));
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn older_preferences_keep_their_settings_and_open_search() {
        let prefs: Prefs = serde_json::from_str(r#"{"theme":"Light","clock_24h":true}"#).unwrap();
        assert_eq!(prefs.theme, Theme::Light);
        assert!(prefs.clock_24h);
        assert_eq!(prefs.default_food_tab, FoodEntryTab::Search);
    }

    #[test]
    fn every_food_tab_can_be_saved_and_restored() {
        for tab in FoodEntryTab::ALL {
            let prefs = Prefs {
                default_food_tab: tab,
                ..Prefs::default()
            };
            let saved = serde_json::to_string(&prefs).unwrap();
            assert_eq!(serde_json::from_str::<Prefs>(&saved).unwrap(), prefs);
        }
    }
}
