//! "Update available" card and the update check button.

use dioxus::prelude::*;

use crate::updater::{self, Release, UpdateState};

/// Checks GitHub for a newer release (Android only).
pub fn check_for_update(mut state: Signal<UpdateState>) {
    if !cfg!(target_os = "android") {
        return;
    }
    state.set(UpdateState::Checking);
    spawn(async move {
        state.set(match updater::check().await {
            Ok(Some(release)) => UpdateState::Available(release),
            Ok(None) => UpdateState::UpToDate,
            Err(message) => UpdateState::Failed(message),
        });
    });
}

/// Downloads and installs `release`, asking for the install permission first.
#[cfg(target_os = "android")]
fn start_update(mut state: Signal<UpdateState>, release: Release) {
    use std::time::Duration;

    use crate::updater::android::{self, Download};

    if !android::can_install() {
        android::open_install_permission_settings();
        state.set(UpdateState::NeedsPermission(release));
        return;
    }
    let Some(id) = android::start_download(&release.apk_url, &release.version) else {
        state.set(UpdateState::Failed("The download could not start.".into()));
        return;
    };
    state.set(UpdateState::Downloading(release.clone(), None));
    spawn(async move {
        loop {
            match android::download_state(id) {
                Download::Running(progress) => {
                    state.set(UpdateState::Downloading(release.clone(), progress))
                }
                Download::Done => {
                    if android::install(id) {
                        state.set(UpdateState::Installing(release.clone()));
                    } else {
                        state.set(UpdateState::Failed(
                            "The installer could not be opened.".into(),
                        ));
                    }
                    break;
                }
                Download::Failed => {
                    state.set(UpdateState::Failed(
                        "The download failed. Check your connection and try again.".into(),
                    ));
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });
}

#[cfg(not(target_os = "android"))]
fn start_update(_state: Signal<UpdateState>, _release: Release) {}

fn size_text(bytes: u64) -> String {
    if bytes == 0 {
        String::new()
    } else {
        format!(" · {:.0} MB", bytes as f64 / 1_048_576.0)
    }
}

/// Shown on the dashboard while an update is available or in progress.
#[component]
pub fn UpdateCard() -> Element {
    let mut state = use_context::<Signal<UpdateState>>();
    let current = state();
    let (release, status, action) = match &current {
        UpdateState::Available(r) => (r.clone(), None, Some("Update")),
        UpdateState::NeedsPermission(r) => (
            r.clone(),
            Some(
                "Allow OpenMacro to install apps in the screen that opened, then tap Update again."
                    .to_string(),
            ),
            Some("Update"),
        ),
        UpdateState::Downloading(r, progress) => (
            r.clone(),
            Some(match progress {
                Some(p) => format!("Downloading… {:.0}%", p * 100.0),
                None => "Downloading…".to_string(),
            }),
            None,
        ),
        UpdateState::Installing(r) => (
            r.clone(),
            Some("Follow the installer to finish updating.".to_string()),
            None,
        ),
        _ => return rsx! {},
    };
    let notes = release.notes.lines().take(4).collect::<Vec<_>>().join("\n");
    let size = size_text(release.apk_size);

    rsx! {
        section { class: "card update-card",
            div { class: "update-head",
                div {
                    h2 { "Update available" }
                    p { class: "muted small", "Version {release.version}{size} · you have {updater::CURRENT_VERSION}" }
                }
                if matches!(current, UpdateState::Available(_)) {
                    button { class: "icon-btn small", aria_label: "Not now", onclick: move |_| state.set(UpdateState::Idle),
                        super::Icon { name: "close" }
                    }
                }
            }
            if !notes.is_empty() {
                p { class: "update-notes", "{notes}" }
            }
            if let Some(status) = status {
                p { class: "muted small", "{status}" }
            }
            if let Some(label) = action {
                button {
                    class: "primary",
                    onclick: move |_| start_update(state, release.clone()),
                    "{label}"
                }
            }
        }
    }
}

/// Version line and a manual check, for the About section.
#[component]
pub fn UpdateCheck() -> Element {
    let state = use_context::<Signal<UpdateState>>();
    let message = match state() {
        UpdateState::Checking => Some("Checking…".to_string()),
        UpdateState::UpToDate => Some("You have the latest version.".to_string()),
        UpdateState::Available(r) | UpdateState::NeedsPermission(r) => Some(format!(
            "Version {} is available on the dashboard.",
            r.version
        )),
        UpdateState::Downloading(r, _) | UpdateState::Installing(r) => {
            Some(format!("Updating to {}…", r.version))
        }
        UpdateState::Failed(e) => Some(e),
        UpdateState::Idle => None,
    };
    rsx! {
        p { "OpenMacro {updater::CURRENT_VERSION} · MIT licensed" }
        if cfg!(target_os = "android") {
            div { class: "button-row",
                button { onclick: move |_| check_for_update(state), "Check for updates" }
            }
            if let Some(message) = message {
                p { class: "muted small", "{message}" }
            }
        }
    }
}
