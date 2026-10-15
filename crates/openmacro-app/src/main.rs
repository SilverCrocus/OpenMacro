#[cfg(target_os = "android")]
mod android;
mod back;
#[cfg(not(target_arch = "wasm32"))]
mod backup;
mod barcode;
mod components;
mod custom_food;
mod export;
mod food_search;
mod foods;
#[cfg(not(target_arch = "wasm32"))]
mod health;
mod keypad;
mod persist;
mod prefs;
mod seed;
mod state;
mod updater;

use dioxus::prelude::*;

use components::{Dashboard, FoodLog, Insight, More, Plan, Shell, check_for_update};
use state::{AppState, today};

const MAIN_CSS: Asset = asset!("/assets/main.css");
const ICON: Asset = asset!("/assets/icon-192.png");

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[layout(Shell)]
        #[route("/")]
        Dashboard {},
        #[route("/log")]
        FoodLog {},
        #[route("/plan")]
        Plan {},
        #[route("/more")]
        More {},
        #[route("/insight/:metric")]
        Insight { metric: String },
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let state = use_context_provider(AppState::load);
    let coach = use_memo(move || state::CoachSnapshot::compute(&state.data(), today()));
    use_context_provider(|| coach);

    // Look for a newer release once per launch (Android only).
    let update = use_context_provider(|| Signal::new(updater::UpdateState::Idle));
    use_hook(move || check_for_update(update));

    // Sharing with other health apps through Health Connect (Android only).
    #[cfg(not(target_arch = "wasm32"))]
    health::use_health_sync(state);

    // The full food database, installed from the app bundle on first launch.
    let foods = use_resource(food_search::open_bundled);
    use_context_provider(|| foods);

    // Display preferences: load once, then apply and save on every change.
    use_hook(move || {
        if let Some(saved) = state.persist.peek().load_prefs() {
            *prefs::PREFS.write() = saved;
        }
    });
    use_effect(move || {
        let current = prefs::PREFS();
        prefs::apply_theme(current.theme);
        state.persist.peek().save_prefs(&current);
    });

    // Save the estimate settings whenever they change.
    use_effect(move || {
        let settings = state.coach_settings.read().clone();
        state.persist.peek().save_coach_settings(&settings);
    });

    // Save the manual program's targets whenever they change.
    use_effect(move || {
        let targets = *state.manual_targets.read();
        state.persist.peek().save_manual_targets(targets);
    });

    // Save the check-in day whenever it changes.
    use_effect(move || {
        let day = state.check_in_day.read().to_string();
        state.persist.peek().save_check_in_day(&day);
    });

    // Save the food region whenever it changes.
    use_effect(move || {
        let code = state.region.read().code();
        state.persist.peek().save_region(code);
    });

    // Save profile, goal and program whenever any of them changes.
    use_effect(move || {
        let (profile, goal, program) = (
            *state.profile.read(),
            *state.goal.read(),
            state.program.read().clone(),
        );
        state
            .persist
            .peek()
            .save_settings(&profile, &goal, &program);
    });

    rsx! {
        document::Title { "OpenMacro" }
        document::Meta { name: "viewport", content: "width=device-width, initial-scale=1, viewport-fit=cover" }
        document::Meta { name: "theme-color", content: "#0f1115" }
        document::Stylesheet { href: MAIN_CSS }
        document::Link { rel: "icon", href: ICON }
        Router::<Route> {}
    }
}
