mod barcode;
mod components;
mod food_search;
mod foods;
mod keypad;
mod persist;
mod seed;
mod state;
mod updater;

use dioxus::prelude::*;

use components::{Dashboard, FoodLog, More, Plan, Shell, check_for_update};
use state::{AppState, today};

const MAIN_CSS: Asset = asset!("/assets/main.css");

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

    // The full food database, installed from the app bundle on first launch.
    let foods = use_resource(food_search::open_bundled);
    use_context_provider(|| foods);

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
        Router::<Route> {}
    }
}
