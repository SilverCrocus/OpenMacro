//! Screens and shared UI pieces.

mod chart;
mod checkin;
mod copy;
mod dashboard;
mod edit_entry;
mod food_log;
mod insight;
mod library;
mod logger;
mod more;
mod plan;
mod recipe;
mod scan;
mod update;

pub use dashboard::Dashboard;
pub use food_log::FoodLog;
pub use insight::Insight;
pub use more::More;
pub use plan::Plan;
pub use update::check_for_update;

use dioxus::prelude::*;
use openmacro_core::nutrients::{Nutrient, NutrientProfile};
use openmacro_core::targets::MacroTargets;

use crate::Route;
use crate::state::{AppState, current_hour};
use checkin::CheckInSheet;
use logger::Logger;

#[component]
pub fn Shell() -> Element {
    let state = use_context::<AppState>();
    let plate_open = state.plate.read().open;
    let check_in_open = (state.check_in_open)();
    let editing = (state.editing)();
    let copying = (state.copying)();

    // Each tab starts at the top instead of inheriting the previous tab's scroll position.
    let route = use_route::<Route>();
    use_effect(use_reactive((&route,), |_| {
        document::eval("window.scrollTo(0, 0)");
    }));
    #[cfg(target_os = "android")]
    crate::back::use_system_back(route != Route::Dashboard {});

    rsx! {
        div { class: "app",
            main { class: "content", Outlet::<Route> {} }
            NavBar {}
            if plate_open {
                Logger {}
            }
            if check_in_open {
                CheckInSheet {}
            }
            if let Some(entry) = editing {
                edit_entry::EditEntrySheet { key: "{entry.id}", entry }
            }
            if let Some(copying) = copying {
                copy::CopySheet { copying }
            }
        }
    }
}

#[component]
fn NavBar() -> Element {
    let state = use_context::<AppState>();
    let waiting = state.plate.read().items.len();
    rsx! {
        nav { class: "tabbar",
            Link { to: Route::Dashboard {}, class: "tab", active_class: "active",
                Icon { name: "dashboard" }
                span { "Dashboard" }
            }
            Link { to: Route::FoodLog {}, class: "tab", active_class: "active",
                Icon { name: "log" }
                span { "Log" }
            }
            button {
                class: "tab-add",
                aria_label: if waiting > 0 { "Log food ({waiting} on the plate)" } else { "Log food" },
                onclick: move |_| state.open_plate((state.selected_date)(), current_hour()),
                Icon { name: "plus" }
                if waiting > 0 {
                    span { class: "tab-add-badge", "{waiting}" }
                }
            }
            Link { to: Route::Plan {}, class: "tab", active_class: "active",
                Icon { name: "plan" }
                span { "Plan" }
            }
            Link { to: Route::More {}, class: "tab", active_class: "active",
                Icon { name: "more" }
                span { "More" }
            }
        }
    }
}

/// Simple stroked line icons.
#[component]
pub fn Icon(name: &'static str) -> Element {
    let body = match name {
        "dashboard" => rsx! {
            rect { x: "3", y: "3", width: "7", height: "9", rx: "1.5" }
            rect { x: "14", y: "3", width: "7", height: "5", rx: "1.5" }
            rect { x: "14", y: "12", width: "7", height: "9", rx: "1.5" }
            rect { x: "3", y: "16", width: "7", height: "5", rx: "1.5" }
        },
        "log" => rsx! {
            line { x1: "9", y1: "6", x2: "20", y2: "6" }
            line { x1: "9", y1: "12", x2: "20", y2: "12" }
            line { x1: "9", y1: "18", x2: "20", y2: "18" }
            circle { cx: "4.5", cy: "6", r: "1" }
            circle { cx: "4.5", cy: "12", r: "1" }
            circle { cx: "4.5", cy: "18", r: "1" }
        },
        "plus" => rsx! {
            line { x1: "12", y1: "5", x2: "12", y2: "19" }
            line { x1: "5", y1: "12", x2: "19", y2: "12" }
        },
        "plan" => rsx! {
            circle { cx: "12", cy: "12", r: "9" }
            circle { cx: "12", cy: "12", r: "5" }
            circle { cx: "12", cy: "12", r: "1" }
        },
        "more" => rsx! {
            circle { cx: "5", cy: "12", r: "1.5" }
            circle { cx: "12", cy: "12", r: "1.5" }
            circle { cx: "19", cy: "12", r: "1.5" }
        },
        "close" => rsx! {
            line { x1: "6", y1: "6", x2: "18", y2: "18" }
            line { x1: "18", y1: "6", x2: "6", y2: "18" }
        },
        "left" => rsx! {
            polyline { points: "15 18 9 12 15 6" }
        },
        "right" => rsx! {
            polyline { points: "9 18 15 12 9 6" }
        },
        "copy" => rsx! {
            rect { x: "8", y: "8", width: "12", height: "12", rx: "2" }
            path { d: "M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2" }
        },
        "edit" => rsx! {
            path { d: "M4 20h4L19 9a2.8 2.8 0 0 0-4-4L4 16v4z" }
            line { x1: "13.5", y1: "6.5", x2: "17.5", y2: "10.5" }
        },
        "search" => rsx! {
            circle { cx: "11", cy: "11", r: "7" }
            line { x1: "20", y1: "20", x2: "16", y2: "16" }
        },
        _ => rsx! {},
    };
    rsx! {
        svg {
            class: "icon",
            "viewBox": "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            "stroke-width": "2",
            "stroke-linecap": "round",
            "stroke-linejoin": "round",
            {body}
        }
    }
}

/// A progress ring; `fraction` of 1.0 is a full circle.
#[component]
pub fn Ring(fraction: f64, size: u32) -> Element {
    let pct = fraction.clamp(0.0, 1.0) * 100.0;
    let class = if fraction > 1.05 {
        "ring-fill over"
    } else {
        "ring-fill"
    };
    rsx! {
        svg { class: "ring", width: "{size}", height: "{size}", "viewBox": "0 0 36 36",
            circle { class: "ring-track", cx: "18", cy: "18", r: "15.9155", fill: "none", "stroke-width": "3.5" }
            if pct >= 0.5 {
                circle {
                class,
                cx: "18",
                cy: "18",
                r: "15.9155",
                fill: "none",
                "stroke-width": "3.5",
                "stroke-dasharray": "{pct} 100",
                "stroke-linecap": "round",
                transform: "rotate(-90 18 18)",
            }
            }
        }
    }
}

/// A line through `values`, scaled to fill its box.
#[component]
pub fn Sparkline(values: Vec<f64>, class: &'static str) -> Element {
    if values.len() < 2 {
        return rsx! {
            svg { class: "spark {class}" }
        };
    }
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = (max - min).max(1e-9);
    let step = 100.0 / (values.len() - 1) as f64;
    let points = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            format!(
                "{:.2},{:.2}",
                i as f64 * step,
                28.0 - (v - min) / span * 26.0
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    rsx! {
        svg { class: "spark {class}", "viewBox": "0 0 100 30", "preserveAspectRatio": "none",
            polyline {
                points,
                fill: "none",
                stroke: "currentColor",
                "stroke-width": "2",
                "vector-effect": "non-scaling-stroke",
            }
        }
    }
}

/// One compact row of calories and macros against targets. Tapping it switches
/// between what has been eaten and what remains.
#[component]
pub fn MacroStrip(totals: NutrientProfile, target: MacroTargets) -> Element {
    let mut remaining = use_signal(|| false);
    let cells = [
        (
            crate::prefs::energy_symbol(),
            Nutrient::Energy,
            target.kcal,
            "kcal",
        ),
        ("P", Nutrient::Protein, target.protein_g, "protein"),
        ("F", Nutrient::Fat, target.fat_g, "fat"),
        ("C", Nutrient::Carbs, target.carbs_g, "carbs"),
    ];
    let mode = if remaining() { "Remaining" } else { "Eaten" };
    rsx! {
        button {
            class: "macro-strip",
            aria_label: "{mode}. Tap to switch between eaten and remaining.",
            onclick: move |_| remaining.set(!remaining()),
            for (letter , nutrient , goal , class) in cells {
                {
                    let value = totals.get_or_zero(nutrient);
                    let shown = if remaining() { (goal - value).max(0.0) } else { value };
                    let width = (value / goal.max(1.0) * 100.0).clamp(0.0, 100.0);
                    let over = value > goal * 1.05;
                    let num = move |v: f64| {
                        if nutrient == Nutrient::Energy { crate::prefs::energy(v) } else { fmt0(v) }
                    };
                    rsx! {
                        div { key: "{letter}", class: if over { "macro-cell {class} over" } else { "macro-cell {class}" },
                            div { class: "macro-cell-text",
                                span { class: "macro-letter", "{letter}" }
                                span { class: "num", {num(shown)} }
                                if remaining() {
                                    span { class: "muted", "left" }
                                } else {
                                    span { class: "muted", "/{num(goal)}" }
                                }
                            }
                            div { class: "bar-track",
                                div { class: "bar-fill", style: "width: {width}%" }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn fmt0(v: f64) -> String {
    unsigned_zero(format!("{:.0}", v))
}

pub fn fmt1(v: f64) -> String {
    unsigned_zero(format!("{:.1}", v))
}

/// "-0" and "-0.0" (from rounding tiny negatives, or summing nothing,
/// which gives -0.0) read as zero.
pub fn unsigned_zero(text: String) -> String {
    match text.strip_prefix('-') {
        Some(rest) if rest.chars().all(|c| c == '0' || c == '.') => rest.to_string(),
        _ => text,
    }
}

/// An hour in the user's clock format.
pub fn hour_label(hour: u32) -> String {
    hour_text(hour, crate::prefs::PREFS.read().clock_24h)
}

fn hour_text(hour: u32, clock_24h: bool) -> String {
    if clock_24h {
        return format!("{hour:02}:00");
    }
    match hour {
        0 => "12 AM".into(),
        1..=11 => format!("{hour} AM"),
        12 => "12 PM".into(),
        _ => format!("{} PM", hour - 12),
    }
}

/// "12P 4F 30C"
pub fn macro_line(n: &NutrientProfile) -> String {
    format!(
        "{}P {}F {}C",
        fmt0(n.get_or_zero(Nutrient::Protein)),
        fmt0(n.get_or_zero(Nutrient::Fat)),
        fmt0(n.get_or_zero(Nutrient::Carbs))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_negative_zero() {
        assert_eq!(fmt0(-0.0), "0");
        assert_eq!(fmt0(-0.3), "0");
        assert_eq!(fmt1(-0.04), "0.0");
        assert_eq!(fmt0(-12.0), "-12");
        assert_eq!(fmt0([0.0f64; 0].iter().sum::<f64>()), "0");
    }

    #[test]
    fn hour_labels() {
        assert_eq!(hour_text(0, false), "12 AM");
        assert_eq!(hour_text(9, false), "9 AM");
        assert_eq!(hour_text(12, false), "12 PM");
        assert_eq!(hour_text(23, false), "11 PM");
        assert_eq!(hour_text(0, true), "00:00");
        assert_eq!(hour_text(9, true), "09:00");
        assert_eq!(hour_text(23, true), "23:00");
    }
}
