//! Display preferences: theme, clock and units. Kept in a global signal so
//! formatting helpers anywhere can follow them.

use dioxus::prelude::*;

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
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct Prefs {
    pub theme: Theme,
    pub clock_24h: bool,
}

pub static PREFS: GlobalSignal<Prefs> = Signal::global(Prefs::default);

/// Applies the theme to the page.
pub fn apply_theme(theme: Theme) {
    document::eval(&format!(
        "document.documentElement.dataset.theme = '{}';",
        theme.code()
    ));
}
