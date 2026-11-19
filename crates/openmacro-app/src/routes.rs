// Routable generates an unreachable error branch when parsing a String route segment.
#![allow(unreachable_code)]

use dioxus::prelude::*;

use crate::components::{Dashboard, FoodLog, Insight, More, Plan, Shell};

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
