//! More > Health Connect: sharing data with other health apps.

use chrono::NaiveDateTime;
use dioxus::prelude::*;

use crate::health::{self, Availability, PERMISSIONS, SETTINGS, STATUS, WINDOW_DAYS};
use crate::state::AppState;

/// "food, water and weight": what the granted permissions starting with
/// `prefix` cover.
fn covered(granted: &[String], prefix: &str) -> String {
    let names: Vec<&str> = PERMISSIONS
        .iter()
        .filter(|(p, _)| p.starts_with(prefix) && granted.iter().any(|g| g == p))
        .map(|(_, name)| *name)
        .collect();
    match names.split_last() {
        None => String::new(),
        Some((last, [])) => last.to_string(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

fn time_text(at: NaiveDateTime) -> String {
    let format = if crate::prefs::PREFS.read().clock_24h {
        "%H:%M"
    } else {
        "%-I:%M %p"
    };
    at.format(format).to_string()
}

#[component]
pub fn HealthConnect() -> Element {
    let state = use_context::<AppState>();
    use_hook(|| {
        spawn(health::refresh());
    });
    let status = STATUS.read().clone();
    let enabled = SETTINGS.read().enabled;
    let reads = covered(&status.granted, "READ_");
    let writes = covered(&status.granted, "WRITE_");
    let all_granted = PERMISSIONS
        .iter()
        .all(|(p, _)| status.granted.iter().any(|g| g == p));

    rsx! {
        section { class: "card",
            h2 { "Health Connect" }
            match status.availability {
                Availability::Unknown => rsx! {
                    p { class: "muted small", "Checking…" }
                },
                Availability::Unsupported => rsx! {
                    p { class: "muted small", "Health Connect isn't available on this phone." }
                },
                Availability::NeedsInstall => rsx! {
                    p { class: "muted small",
                        "Install or update Health Connect from the Play Store to share data with other health apps."
                    }
                    div { class: "button-row",
                        button { onclick: |_| health::open(), "Get Health Connect" }
                    }
                },
                Availability::Ready if !enabled => rsx! {
                    p { class: "muted small",
                        "Share with other health apps. Weight and body fat from apps such as your smart scale's come in as weigh-ins and measurements, and your food log, water, weigh-ins and body fat go out."
                    }
                    div { class: "button-row",
                        button { class: "primary", onclick: move |_| health::connect(state), "Connect" }
                    }
                },
                Availability::Ready => rsx! {
                    if status.granted.is_empty() {
                        p { "OpenMacro isn't allowed to use Health Connect yet." }
                    } else {
                        if !reads.is_empty() {
                            p { "Reads {reads}." }
                        }
                        if !writes.is_empty() {
                            p { "Writes {writes}." }
                        }
                    }
                    if status.syncing {
                        p { class: "muted small", "Syncing…" }
                    } else if let Some(last) = &status.last {
                        match &last.error {
                            None => rsx! {
                                p { class: "muted small", "Last synced {time_text(last.at)}" }
                            },
                            Some(error) => rsx! {
                                p { class: "error", "{error}" }
                            },
                        }
                    }
                    if let Some((at, change)) = &status.last_change {
                        p { class: "muted small", "{time_text(*at)}: {change}" }
                    }
                    p { class: "muted small",
                        "Syncs the last {WINDOW_DAYS} days while OpenMacro is open. Turning it off keeps what's already in Health Connect; you can remove that in Health Connect."
                    }
                    div { class: "button-row",
                        if !status.granted.is_empty() {
                            button {
                                disabled: status.syncing,
                                onclick: move |_| health::sync(state),
                                "Sync now"
                            }
                        }
                        if !all_granted {
                            button {
                                class: if status.granted.is_empty() { "primary" } else { "" },
                                onclick: move |_| health::ask_permissions(state),
                                if status.granted.is_empty() { "Allow access" } else { "Allow more" }
                            }
                        }
                        button { onclick: |_| health::open(), "Open Health Connect" }
                        button { onclick: |_| health::turn_off(), "Turn off" }
                    }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissions_read_as_a_list() {
        let granted: Vec<String> = [
            "WRITE_NUTRITION",
            "WRITE_HYDRATION",
            "WRITE_WEIGHT",
            "READ_WEIGHT",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(covered(&granted, "WRITE_"), "food, water and weight");
        assert_eq!(covered(&granted, "READ_"), "weight");
        assert_eq!(covered(&[], "READ_"), "");
    }
}
