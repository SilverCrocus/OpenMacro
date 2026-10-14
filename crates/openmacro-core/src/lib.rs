//! Pure domain logic for OpenMacro.
//!
//! This crate has no I/O. Everything is stored in metric units internally
//! (kilograms, centimetres, kilocalories, grams); conversion to display units
//! happens at the edges via [`units`].
//!
//! Module overview:
//! - [`nutrients`]: the tracked nutrient set and per-amount nutrient profiles.
//! - [`food`]: foods, servings and quantity parsing.
//! - [`log`]: logged food entries, profile and goal.
//! - [`bmr`]: BMR equations and the initial expenditure estimate.
//! - [`trend`]: smoothed trend weight from noisy scale weigh-ins.
//! - [`expenditure`]: adaptive daily energy expenditure from intake and weight change.
//! - [`targets`]: goal-driven calorie targets, weekly distribution and macro splits.
//! - [`checkin`]: weekly check-in timing and the days to review before one.

pub mod bmr;
pub mod checkin;
pub mod expenditure;
pub mod food;
pub mod log;
pub mod micros;
pub mod nutrients;
pub mod recipe;
pub mod targets;
pub mod trend;
pub mod units;

/// Approximate energy content of one kilogram of body-mass change, in kcal.
///
/// The classic "3500 kcal per pound" rule expressed per kilogram. Body-mass
/// change is a mix of fat, lean tissue and water, so this is a modelling
/// convenience rather than a physiological constant.
pub const KCAL_PER_KG_BODY_MASS: f64 = 7700.0;
