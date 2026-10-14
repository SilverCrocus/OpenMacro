//! Detail pages behind the dashboard tiles: a chart over a chosen range and
//! a few numbers that explain it.

use std::collections::BTreeMap;

use chrono::{Duration, NaiveDate};
use dioxus::prelude::*;
use openmacro_core::KCAL_PER_KG_BODY_MASS;
use openmacro_core::expenditure::{DayStatus, EstimateStatus};
use openmacro_core::micros::{Standing, standard_goals, standing};
use openmacro_core::nutrients::{Nutrient, NutrientGroup};

use super::chart::{Chart, ChartData};
use super::{Icon, fmt0, fmt1};
use crate::Route;
use crate::prefs;
use crate::state::{AppState, CoachSnapshot, LogEntry, today};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Range {
    Week,
    Month,
    ThreeMonths,
    SixMonths,
    Year,
    All,
}

impl Range {
    const ALL: [Range; 6] = [
        Range::Week,
        Range::Month,
        Range::ThreeMonths,
        Range::SixMonths,
        Range::Year,
        Range::All,
    ];

    fn label(self) -> &'static str {
        match self {
            Range::Week => "1W",
            Range::Month => "1M",
            Range::ThreeMonths => "3M",
            Range::SixMonths => "6M",
            Range::Year => "1Y",
            Range::All => "All",
        }
    }

    fn days(self) -> Option<i64> {
        match self {
            Range::Week => Some(7),
            Range::Month => Some(30),
            Range::ThreeMonths => Some(91),
            Range::SixMonths => Some(182),
            Range::Year => Some(365),
            Range::All => None,
        }
    }

    /// The first day shown, given the first day with any data.
    pub fn start(self, today: NaiveDate, first: NaiveDate) -> NaiveDate {
        match self.days() {
            Some(days) => (today - Duration::days(days - 1)).max(first),
            None => first,
        }
    }
}

/// The range last picked on any insight page, kept while the app is open.
static RANGE: GlobalSignal<Range> = Signal::global(|| Range::Month);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Metric {
    Expenditure,
    Weight,
    Balance,
    Macros,
    Goal,
    Nutrients,
}

impl Metric {
    fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "expenditure" => Some(Metric::Expenditure),
            "weight" => Some(Metric::Weight),
            "energy-balance" => Some(Metric::Balance),
            "macros" => Some(Metric::Macros),
            "goal" => Some(Metric::Goal),
            "nutrients" => Some(Metric::Nutrients),
            _ => None,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Metric::Expenditure => "Expenditure",
            Metric::Weight => "Weight trend",
            Metric::Balance => "Energy balance",
            Metric::Macros => "Calories and macros",
            Metric::Goal => "Goal progress",
            Metric::Nutrients => "Nutrients",
        }
    }
}

/// The dashboard link for each page.
pub fn route(slug: &str) -> Route {
    Route::Insight {
        metric: slug.to_string(),
    }
}

#[component]
pub fn Insight(metric: String) -> Element {
    let Some(metric) = Metric::from_slug(&metric) else {
        return rsx! {
            p { "Page not found." }
        };
    };
    let range = RANGE();

    rsx! {
        header { class: "page-header insight-header",
            Link { to: Route::Dashboard {}, class: "icon-btn", aria_label: "Back to dashboard",
                Icon { name: "left" }
            }
            h1 { "{metric.title()}" }
        }
        div { class: "range-chips", role: "tablist",
            for r in Range::ALL {
                button {
                    key: "{r.label()}",
                    class: if r == range { "chip active" } else { "chip" },
                    role: "tab",
                    aria_selected: "{r == range}",
                    onclick: move |_| *RANGE.write() = r,
                    "{r.label()}"
                }
            }
        }
        match metric {
            Metric::Expenditure => rsx! {
                ExpenditureInsight { range }
            },
            Metric::Weight => rsx! {
                WeightInsight { range }
            },
            Metric::Balance => rsx! {
                BalanceInsight { range }
            },
            Metric::Macros => rsx! {
                MacrosInsight { range }
            },
            Metric::Goal => rsx! {
                GoalInsight { range }
            },
            Metric::Nutrients => rsx! {
                NutrientsInsight { range }
            },
        }
    }
}

/// One labelled number under the chart.
#[component]
fn Stat(label: String, value: String) -> Element {
    rsx! {
        div { class: "stat",
            span { class: "num", "{value}" }
            span { class: "muted", "{label}" }
        }
    }
}

fn signed(value: f64, decimals: usize, unit: &str) -> String {
    let rounded = format!("{value:+.decimals$}");
    let zero = rounded
        .trim_start_matches(['+', '-'])
        .trim_matches(['0', '.'])
        .is_empty();
    let text = if zero {
        format!("{:.decimals$}", 0.0)
    } else {
        rounded
    };
    format!("{text} {unit}")
}

/// Signed energy, in thousands once it would no longer fit a stat tile.
fn signed_energy(value: f64, unit: &str) -> String {
    if value.abs() >= 10_000.0 {
        signed(value / 1000.0, 1, &format!("k {unit}")).replacen(" k", "k", 1)
    } else {
        signed(value, 0, unit)
    }
}

#[component]
fn ExpenditureInsight(range: Range) -> Element {
    let coach = use_context::<Memo<CoachSnapshot>>();
    let coach = coach.read();
    let today = today();
    let Some(first) = coach.expenditure.first().map(|p| p.date) else {
        return rsx! {
            EmptyInsight {}
        };
    };
    let start = range.start(today, first);
    let points: Vec<_> = coach
        .expenditure
        .iter()
        .filter(|p| p.date >= start)
        .collect();
    let data = ChartData {
        start: Some(start),
        end: Some(today),
        line: points
            .iter()
            .map(|p| (p.date, prefs::energy_value(p.kcal)))
            .collect(),
        band: points
            .iter()
            .map(|p| {
                (
                    p.date,
                    prefs::energy_value(p.kcal - p.uncertainty_kcal),
                    prefs::energy_value(p.kcal + p.uncertainty_kcal),
                )
            })
            .collect(),
        decimals: 0,
        ..Default::default()
    };
    let current = coach.expenditure_kcal();
    let average = points.iter().map(|p| p.kcal).sum::<f64>() / points.len().max(1) as f64;
    let change = points
        .last()
        .zip(points.first())
        .map(|(l, f)| l.kcal - f.kcal)
        .unwrap_or(0.0);
    let uncertainty = points.last().map_or(0.0, |p| p.uncertainty_kcal);
    let status = match coach.expenditure_status() {
        EstimateStatus::Initial => {
            "This is still the starting estimate from your profile. It begins adapting once you've logged food and weighed in for a few days."
        }
        EstimateStatus::Holding => {
            "The estimate is holding at its last value because there isn't enough recent food logging or weigh-ins. It resumes once you log again."
        }
        EstimateStatus::Updated => {
            "The estimate updates daily by comparing what you eat with how your weight changes."
        }
    };

    rsx! {
        section { class: "card",
            Chart { data, class: "exp" }
            p { class: "muted small legend",
                span { class: "swatch exp" }
                "Expenditure"
                span { class: "swatch band" }
                "Likely range"
            }
        }
        section { class: "card",
            div { class: "stat-row",
                Stat { label: "Current", value: prefs::energy_text(current) }
                Stat { label: "Average", value: prefs::energy_text(average) }
                Stat { label: "Change", value: signed(prefs::energy_value(change), 0, prefs::energy_symbol()) }
            }
        }
        section { class: "card",
            h2 { "How it's estimated" }
            p { class: "muted", "{status}" }
            p { class: "muted",
                "Today's estimate is likely within ±{prefs::energy_text(uncertainty)}. Logging every day and weighing in often narrows that range."
            }
        }
    }
}

#[component]
fn WeightInsight(range: Range) -> Element {
    let coach = use_context::<Memo<CoachSnapshot>>();
    let coach = coach.read();
    let today = today();
    let Some(first) = coach.trend.first().map(|t| t.date) else {
        return rsx! {
            EmptyInsight {}
        };
    };
    let start = range.start(today, first);
    let points: Vec<_> = coach.trend.iter().filter(|t| t.date >= start).collect();
    let data = ChartData {
        start: Some(start),
        end: Some(today),
        line: points
            .iter()
            .map(|t| (t.date, prefs::weight_value(t.trend_kg)))
            .collect(),
        dots: points
            .iter()
            .filter_map(|t| t.scale_kg.map(|kg| (t.date, prefs::weight_value(kg))))
            .collect(),
        decimals: 1,
        ..Default::default()
    };
    let by_date: BTreeMap<NaiveDate, f64> =
        coach.trend.iter().map(|t| (t.date, t.trend_kg)).collect();
    let current = coach.trend_kg().unwrap_or(0.0);
    let change = points
        .last()
        .zip(points.first())
        .map(|(l, f)| l.trend_kg - f.trend_kg)
        .unwrap_or(0.0);
    let latest_scale = coach.trend.iter().rev().find_map(|t| t.scale_kg);
    let periods: Vec<(i64, Option<f64>)> = [3, 7, 14, 30, 90]
        .into_iter()
        .map(|days| {
            let then = by_date.get(&(today - Duration::days(days))).copied();
            (days, then.map(|then| current - then))
        })
        .collect();

    rsx! {
        section { class: "card",
            Chart { data, class: "trend" }
            p { class: "muted small legend",
                span { class: "swatch trend" }
                "Trend weight"
                span { class: "swatch dot" }
                "Scale weight"
            }
        }
        section { class: "card",
            div { class: "stat-row",
                Stat { label: "Trend", value: prefs::weight_text(current, 1) }
                Stat { label: "Latest weigh-in", value: latest_scale.map_or("—".into(), |kg| prefs::weight_text(kg, 1)) }
                Stat { label: "Change", value: signed(prefs::weight_value(change), 1, prefs::weight_symbol()) }
            }
        }
        section { class: "card",
            h2 { "Trend weight change" }
            table { class: "insight-table",
                thead {
                    tr {
                        th { "Over" }
                        th { "Change" }
                        th { "Per week" }
                    }
                }
                tbody {
                    for (days , delta) in periods {
                        tr { key: "{days}",
                            td { "{days} days" }
                            td { {delta.map_or("—".into(), |d| signed(prefs::weight_value(d), 2, prefs::weight_symbol()))} }
                            td {
                                {delta.map_or("—".into(), |d| signed(prefs::weight_value(d / days as f64 * 7.0), 2, prefs::weight_symbol()))}
                            }
                        }
                    }
                }
            }
            p { class: "muted small",
                "Trend weight smooths out day-to-day swings from water, salt and food in your gut, so it shows the direction you're really heading."
            }
        }
        WeighIns { start }
    }
}

/// The weigh-ins in the range, newest first, so a mistyped one can be removed.
#[component]
fn WeighIns(start: NaiveDate) -> Element {
    let state = use_context::<AppState>();
    let mut showing_all = use_signal(|| false);
    let mut list: Vec<_> = state
        .weigh_ins
        .read()
        .iter()
        .filter(|w| w.date >= start)
        .copied()
        .collect();
    list.reverse();
    let total = list.len();
    if !showing_all() {
        list.truncate(10);
    }
    if total == 0 {
        return rsx! {};
    }

    rsx! {
        section { class: "card",
            h2 { "Weigh-ins" }
            ul { class: "weigh-in-list",
                for (i , w) in list.into_iter().enumerate() {
                    li { key: "{i}-{w.date}-{w.kg}",
                        span { {w.date.format("%a %-d %b").to_string()} }
                        strong { {prefs::weight_text(w.kg, 1)} }
                        button {
                            class: "icon-btn small",
                            aria_label: "Delete the weigh-in on {w.date}",
                            onclick: move |_| state.delete_weigh_in(w),
                            Icon { name: "close" }
                        }
                    }
                }
            }
            if total > 10 && !showing_all() {
                button { class: "link-btn", onclick: move |_| showing_all.set(true), "Show all {total}" }
            }
        }
    }
}

#[component]
fn BalanceInsight(range: Range) -> Element {
    let state = use_context::<AppState>();
    let coach = use_context::<Memo<CoachSnapshot>>();
    let coach = coach.read();
    let today = today();
    let log = state.log.read();
    let statuses = state.day_status.read();
    let mut intake: BTreeMap<NaiveDate, f64> = BTreeMap::new();
    for e in log.iter() {
        *intake.entry(e.date).or_default() += e.nutrients.get_or_zero(Nutrient::Energy);
    }
    let expenditure: BTreeMap<NaiveDate, f64> =
        coach.expenditure.iter().map(|p| (p.date, p.kcal)).collect();
    let Some(first) = coach.expenditure.first().map(|p| p.date) else {
        return rsx! {
            EmptyInsight {}
        };
    };
    let start = range.start(today, first);
    // Complete days only: today is still in progress, partial days are
    // incomplete, and unlogged days count only when they were fasts.
    let days: Vec<(NaiveDate, f64)> = start
        .iter_days()
        .take_while(|d| *d < today)
        .filter_map(|date| {
            let eaten = match statuses.get(&date).copied().unwrap_or_default() {
                DayStatus::Partial => return None,
                DayStatus::Fasted => 0.0,
                DayStatus::Normal => *intake.get(&date)?,
            };
            Some((date, eaten - expenditure.get(&date)?))
        })
        .collect();
    let total: f64 = days.iter().map(|d| d.1).sum();
    let average = total / days.len().max(1) as f64;
    let implied_kg = total / KCAL_PER_KG_BODY_MASS;
    let data = ChartData {
        start: Some(start),
        end: Some(today),
        bars: days
            .iter()
            .map(|(d, v)| (*d, prefs::energy_value(*v)))
            .collect(),
        decimals: 0,
        ..Default::default()
    };
    let kind = if average < 0.0 { "deficit" } else { "surplus" };
    let direction = if total < 0.0 { "lost" } else { "gained" };

    rsx! {
        section { class: "card",
            if days.is_empty() {
                div { class: "chart-empty muted", "No complete days logged in this range." }
            } else {
                Chart { data, class: "balance" }
            }
            p { class: "muted small legend",
                span { class: "swatch below" }
                "Deficit"
                span { class: "swatch above" }
                "Surplus"
            }
        }
        section { class: "card",
            div { class: "stat-row",
                Stat { label: "Average per day", value: signed(prefs::energy_value(average), 0, prefs::energy_symbol()) }
                Stat { label: "Total", value: signed_energy(prefs::energy_value(total), prefs::energy_symbol()) }
                Stat { label: "Days counted", value: days.len().to_string() }
            }
        }
        section { class: "card",
            h2 { "What it means" }
            p { class: "muted",
                if days.is_empty() {
                    "Log complete days to see how your intake compares with your expenditure."
                } else {
                    "An average {kind} of {prefs::energy_text(average.abs())} a day. Over these {days.len()} days that's about {prefs::weight_text(implied_kg.abs(), 2)} of body mass {direction}, using 7700 kcal per kg."
                }
            }
            p { class: "muted small", "Energy balance is what you logged minus your estimated expenditure for each complete day. Partial days and today are left out." }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Macro {
    Calories,
    Protein,
    Fat,
    Carbs,
}

impl Macro {
    const ALL: [Macro; 4] = [Macro::Calories, Macro::Protein, Macro::Fat, Macro::Carbs];

    fn label(self) -> &'static str {
        match self {
            Macro::Calories => "Calories",
            Macro::Protein => "Protein",
            Macro::Fat => "Fat",
            Macro::Carbs => "Carbs",
        }
    }

    fn unit(self) -> &'static str {
        if self == Macro::Calories {
            prefs::energy_symbol()
        } else {
            "g"
        }
    }

    /// Amount eaten, energy in the user's unit.
    fn eaten(self, totals: &openmacro_core::nutrients::NutrientProfile) -> f64 {
        match self {
            Macro::Calories => prefs::energy_value(totals.get_or_zero(Nutrient::Energy)),
            Macro::Protein => totals.get_or_zero(Nutrient::Protein),
            Macro::Fat => totals.get_or_zero(Nutrient::Fat),
            Macro::Carbs => totals.get_or_zero(Nutrient::Carbs),
        }
    }

    /// Within 10% of target; protein is a minimum, so more is fine.
    fn on_target(self, eaten: f64, target: f64) -> bool {
        match self {
            Macro::Protein => eaten >= target * (1.0 - ON_TARGET),
            _ => (eaten - target).abs() <= target * ON_TARGET,
        }
    }

    fn target(self, t: &openmacro_core::targets::MacroTargets) -> f64 {
        match self {
            Macro::Calories => prefs::energy_value(t.kcal),
            Macro::Protein => t.protein_g,
            Macro::Fat => t.fat_g,
            Macro::Carbs => t.carbs_g,
        }
    }
}

static MACRO: GlobalSignal<Macro> = Signal::global(|| Macro::Calories);

/// Share of days within this fraction of target that count as on target.
const ON_TARGET: f64 = 0.10;

#[component]
fn MacrosInsight(range: Range) -> Element {
    let state = use_context::<AppState>();
    let coach = use_context::<Memo<CoachSnapshot>>();
    let coach = coach.read();
    let today = today();
    let picked = MACRO();
    let log = state.log.read();
    let statuses = state.day_status.read();
    let check_ins = state.check_ins.read();
    let Some(first) = log.iter().map(|e| e.date).min() else {
        return rsx! {
            EmptyInsight {}
        };
    };
    let start = range.start(today, first);
    // Complete, fully logged days: today is still in progress.
    let days: Vec<(NaiveDate, f64, f64)> = start
        .iter_days()
        .take_while(|d| *d < today)
        .filter(|d| statuses.get(d).copied().unwrap_or_default() != DayStatus::Partial)
        .filter_map(|date| {
            let totals = crate::state::day_totals(&log, date);
            // Days with nothing logged aren't counted.
            totals.get(Nutrient::Energy)?;
            let target = crate::state::target_on(date, &check_ins, &coach)
                .unwrap_or_else(|| coach.target_for(date));
            Some((date, picked.eaten(&totals), picked.target(&target)))
        })
        .collect();
    let n = days.len().max(1) as f64;
    let average = days.iter().map(|d| d.1).sum::<f64>() / n;
    let average_target = days.iter().map(|d| d.2).sum::<f64>() / n;
    let on_target = days
        .iter()
        .filter(|(_, eaten, target)| picked.on_target(*eaten, *target))
        .count();
    let data = ChartData {
        start: Some(start),
        end: Some(today - Duration::days(1)),
        bars: days.iter().map(|d| (d.0, d.1)).collect(),
        plain_bars: true,
        dashed: days.iter().map(|d| (d.0, d.2)).collect(),
        decimals: 0,
        ..Default::default()
    };
    let unit = picked.unit();
    let percent = if average_target > 0.0 {
        average / average_target * 100.0
    } else {
        0.0
    };

    rsx! {
        div { class: "range-chips macro-chips",
            for m in Macro::ALL {
                button {
                    key: "{m.label()}",
                    class: if m == picked { "chip active" } else { "chip" },
                    onclick: move |_| *MACRO.write() = m,
                    "{m.label()}"
                }
            }
        }
        section { class: "card",
            if days.is_empty() {
                div { class: "chart-empty muted", "No complete days logged in this range." }
            } else {
                Chart { data, class: "macros" }
            }
            p { class: "muted small legend",
                span { class: "swatch intake" }
                "Eaten"
                span { class: "swatch dashed" }
                "Target"
            }
        }
        section { class: "card",
            div { class: "stat-row",
                Stat { label: "Average per day", value: format!("{} {unit}", fmt0(average)) }
                Stat { label: "Of target", value: format!("{percent:.0}%") }
                Stat { label: "Days on target", value: format!("{on_target} of {}", days.len()) }
            }
        }
        p { class: "muted small pad",
            "A day is on target within {ON_TARGET * 100.0:.0}% of that day's target (for protein, anything from {100.0 - ON_TARGET * 100.0:.0}% up). Today and partly logged days are left out."
        }
    }
}

#[component]
fn GoalInsight(range: Range) -> Element {
    let state = use_context::<AppState>();
    let coach = use_context::<Memo<CoachSnapshot>>();
    let coach = coach.read();
    let today = today();
    let goal = (state.goal)();
    let (Some(first), Some(current)) = (coach.trend.first(), coach.trend_kg()) else {
        return rsx! {
            EmptyInsight {}
        };
    };
    let start_kg = first.trend_kg;
    let start = range.start(today, first.date);
    let to_go = goal.goal_weight_kg - current;
    let weekly_kg = current * goal.rate_pct_per_week / 100.0;
    // Days to the goal at the goal rate, if that rate heads towards it.
    let eta_days =
        (weekly_kg.abs() > 1e-6 && to_go.signum() == weekly_kg.signum() && to_go.abs() > 0.05)
            .then(|| (to_go / weekly_kg * 7.0).ceil() as i64);
    // Project at most as far ahead as the range looks back, and a year at most.
    let horizon = (today - start).num_days().clamp(28, 365);
    let projection_days = eta_days.map(|d| d.min(horizon));
    let end = today + Duration::days(projection_days.unwrap_or(0));
    let data = ChartData {
        start: Some(start),
        end: Some(end),
        line: coach
            .trend
            .iter()
            .filter(|t| t.date >= start)
            .map(|t| (t.date, prefs::weight_value(t.trend_kg)))
            .collect(),
        dashed: projection_days.map_or_else(Vec::new, |d| {
            vec![
                (today, prefs::weight_value(current)),
                (
                    end,
                    prefs::weight_value(current + weekly_kg * d as f64 / 7.0),
                ),
            ]
        }),
        level: Some(prefs::weight_value(goal.goal_weight_kg)),
        decimals: 1,
        ..Default::default()
    };
    let total = goal.goal_weight_kg - start_kg;
    let progress = if total.abs() > 0.05 {
        ((current - start_kg) / total * 100.0).clamp(0.0, 100.0)
    } else {
        100.0
    };
    let summary = match eta_days {
        _ if to_go.abs() <= 0.05 => "You're at your goal weight.".to_string(),
        Some(days) => format!(
            "At your goal rate of {:+.2}% a week ({:+.2} {}), you'd reach {} around {}.",
            goal.rate_pct_per_week,
            prefs::weight_value(weekly_kg),
            prefs::weight_symbol(),
            prefs::weight_text(goal.goal_weight_kg, 1),
            (today + Duration::days(days)).format("%-d %B %Y")
        ),
        None => {
            "Your goal rate doesn't move you towards your goal weight. Change it on the Plan tab."
                .to_string()
        }
    };

    rsx! {
        section { class: "card",
            Chart { data, class: "goal" }
            p { class: "muted small legend",
                span { class: "swatch trend" }
                "Trend weight"
                span { class: "swatch dashed" }
                "Projection"
                span { class: "swatch level" }
                "Goal"
            }
        }
        section { class: "card",
            div { class: "stat-row",
                Stat { label: "Started", value: prefs::weight_text(start_kg, 1) }
                Stat { label: "Now", value: prefs::weight_text(current, 1) }
                Stat { label: "Goal", value: prefs::weight_text(goal.goal_weight_kg, 1) }
            }
            div { class: "goal-progress",
                div { class: "bar-track",
                    div { class: "bar-fill goal-fill", style: "width: {progress:.0}%" }
                }
                p { class: "muted small", "{progress:.0}% of the way · {prefs::weight_text(to_go.abs(), 1)} to go" }
            }
        }
        section { class: "card",
            p { class: "muted", "{summary}" }
            p { class: "muted small", "Started is your first trend weight in OpenMacro." }
        }
        GoalHistory {}
    }
}

/// Goals in force at past check-ins, newest first, to look back on or reuse.
#[component]
fn GoalHistory() -> Element {
    let mut state = use_context::<AppState>();
    let current = (state.goal)();
    let check_ins = state.check_ins.read();
    // One row per change of goal.
    let mut history: Vec<(NaiveDate, openmacro_core::log::Goal)> = Vec::new();
    for c in check_ins.iter() {
        if let Some(goal) = c.goal {
            if history.last().is_none_or(|(_, last)| *last != goal) {
                history.push((c.date, goal));
            }
        }
    }
    history.reverse();
    if history.is_empty() {
        return rsx! {};
    }
    let describe = |g: &openmacro_core::log::Goal| {
        if g.is_maintain() {
            format!("Maintain {}", prefs::weight_text(g.goal_weight_kg, 1))
        } else {
            format!(
                "{} to {} at {:+.2}% a week",
                if g.rate_pct_per_week < 0.0 {
                    "Lose"
                } else {
                    "Gain"
                },
                prefs::weight_text(g.goal_weight_kg, 1),
                g.rate_pct_per_week
            )
        }
    };

    rsx! {
        section { class: "card",
            h2 { "Goal history" }
            ul { class: "goal-history",
                for (date , goal) in history {
                    li { key: "{date}",
                        div {
                            div { "{describe(&goal)}" }
                            div { class: "muted small", "From the check-in on {date.format(\"%-d %B %Y\")}" }
                        }
                        if goal == current {
                            span { class: "muted small", "Current" }
                        } else {
                            button { class: "chip", onclick: move |_| state.goal.set(goal), "Use" }
                        }
                    }
                }
            }
        }
    }
}

/// One nutrient's average day against its standard goal.
#[derive(Debug, Clone, PartialEq)]
pub struct NutrientRow {
    pub nutrient: Nutrient,
    pub average: f64,
    pub goal: openmacro_core::nutrients::NutrientGoal,
    pub standing: Standing,
    /// Share of the foods logged in the range that have data for it.
    pub coverage: f64,
}

/// Average intake of every nutrient with a standard goal over the complete,
/// fully logged days in `[start, end)`, and how many such days there were.
pub fn nutrient_rows(
    log: &[LogEntry],
    statuses: &BTreeMap<NaiveDate, DayStatus>,
    start: NaiveDate,
    end: NaiveDate,
    goals: &BTreeMap<Nutrient, openmacro_core::nutrients::NutrientGoal>,
) -> (Vec<NutrientRow>, usize) {
    let days: Vec<NaiveDate> = start
        .iter_days()
        .take_while(|d| *d < end)
        .filter(|d| statuses.get(d).copied().unwrap_or_default() != DayStatus::Partial)
        .filter(|d| log.iter().any(|e| e.date == *d))
        .collect();
    let entries: Vec<&LogEntry> = log.iter().filter(|e| days.contains(&e.date)).collect();
    let rows = goals
        .iter()
        .map(|(&nutrient, goal)| {
            let total: f64 = entries
                .iter()
                .map(|e| e.nutrients.get_or_zero(nutrient))
                .sum();
            let average = total / days.len().max(1) as f64;
            let known = entries
                .iter()
                .filter(|e| e.nutrients.get(nutrient).is_some())
                .count();
            NutrientRow {
                nutrient,
                average,
                goal: *goal,
                standing: standing(goal, average),
                coverage: known as f64 / entries.len().max(1) as f64,
            }
        })
        .collect();
    (rows, days.len())
}

/// Small amounts with a decimal, larger ones whole: "1.6", "18", "2300".
fn amount(value: f64) -> String {
    if value < 10.0 {
        fmt1(value)
    } else {
        fmt0(value)
    }
}

/// The standard goals for the user's profile and current targets.
pub fn user_goals(
    state: &AppState,
    coach: &CoachSnapshot,
) -> BTreeMap<Nutrient, openmacro_core::nutrients::NutrientGoal> {
    let stats = state.profile.read().stats;
    let mut goals = standard_goals(stats.sex, stats.age_years, coach.average_target.kcal);
    for (key, goal) in &state.coach_settings.read().nutrient_goals {
        if let Some(n) = Nutrient::from_key(key) {
            goals.insert(n, *goal);
        }
    }
    goals
}

/// Edits one nutrient's floor, target and ceiling, or resets it to standard.
#[component]
fn NutrientGoalEditor(
    nutrient: Nutrient,
    goal: openmacro_core::nutrients::NutrientGoal,
) -> Element {
    let mut state = use_context::<AppState>();
    let custom = state
        .coach_settings
        .read()
        .nutrient_goals
        .contains_key(nutrient.key());
    let mut draft = use_signal(|| goal);
    let mut error = use_signal(|| None::<&'static str>);
    let unit = nutrient.unit().symbol();
    let field =
        |label: &'static str,
         value: Option<f64>,
         set: fn(&mut openmacro_core::nutrients::NutrientGoal, Option<f64>)| {
            rsx! {
                label { class: "field",
                    span { "{label} ({unit})" }
                    input {
                        r#type: "text",
                        "inputmode": "decimal",
                        placeholder: "none",
                        initial_value: value.map(amount).unwrap_or_default(),
                        oninput: move |e| {
                            let text = e.value().trim().replace(',', ".");
                            let parsed = if text.is_empty() { Some(None) } else { text.parse::<f64>().ok().filter(|v| *v >= 0.0).map(Some) };
                            match parsed {
                                Some(v) => {
                                    set(&mut draft.write(), v);
                                    error.set(None);
                                }
                                None => error.set(Some("Enter a number, or leave it empty.")),
                            }
                        },
                    }
                }
            }
        };

    rsx! {
        div { class: "goal-editor",
            div { class: "field-grid three",
                {field("Floor", goal.floor, |g, v| g.floor = v)}
                {field("Target", goal.target, |g, v| g.target = v)}
                {field("Ceiling", goal.ceiling, |g, v| g.ceiling = v)}
            }
            if let Some(message) = error() {
                p { class: "error", "{message}" }
            } else if !draft().is_valid() {
                p { class: "error", "The floor, target and ceiling need to be in order." }
            }
            div { class: "button-row",
                button {
                    class: "primary",
                    disabled: error().is_some() || !draft().is_valid(),
                    onclick: move |_| {
                        state.coach_settings.write().nutrient_goals.insert(nutrient.key().to_string(), draft());
                    },
                    "Save target"
                }
                if custom {
                    button {
                        onclick: move |_| {
                            state.coach_settings.write().nutrient_goals.remove(nutrient.key());
                        },
                        "Reset to standard"
                    }
                }
            }
        }
    }
}

#[component]
fn NutrientsInsight(range: Range) -> Element {
    let state = use_context::<AppState>();
    let coach = use_context::<Memo<CoachSnapshot>>();
    let coach = coach.read();
    let today = today();
    let mut open = use_signal(|| None::<Nutrient>);
    let log = state.log.read();
    let Some(first) = log.iter().map(|e| e.date).min() else {
        return rsx! {
            EmptyInsight {}
        };
    };
    let start = range.start(today, first);
    let goals = user_goals(&state, &coach);
    let (rows, days) = nutrient_rows(&log, &state.day_status.read(), start, today, &goals);
    // Nutrients none of the logged foods report can't be judged.
    let known = rows.iter().filter(|r| r.coverage > 0.0).count();
    let met = rows
        .iter()
        .filter(|r| r.coverage > 0.0 && r.standing == Standing::Met)
        .count();
    let groups = [
        (
            "Carbs and fats",
            vec![NutrientGroup::Carbohydrate, NutrientGroup::Fat],
        ),
        ("Vitamins", vec![NutrientGroup::Vitamin]),
        ("Minerals", vec![NutrientGroup::Mineral]),
        ("Other", vec![NutrientGroup::Other]),
    ];
    // Top foods for the opened nutrient, over the same days.
    let sources = open().map(|n| {
        let mut by_food: BTreeMap<String, f64> = BTreeMap::new();
        for e in log.iter().filter(|e| e.date >= start && e.date < today) {
            if let Some(v) = e.nutrients.get(n).filter(|v| *v > 0.0) {
                *by_food.entry(e.name.clone()).or_default() += v;
            }
        }
        let total: f64 = by_food.values().sum();
        let mut list: Vec<(String, f64)> = by_food
            .into_iter()
            .map(|(name, v)| (name, v / total.max(1e-9)))
            .collect();
        list.sort_by(|a, b| b.1.total_cmp(&a.1));
        list.truncate(5);
        list
    });

    rsx! {
        section { class: "card",
            div { class: "stat-row",
                Stat { label: "On target", value: format!("{met} of {known}") }
                Stat { label: "Days counted", value: days.to_string() }
            }
            p { class: "muted small",
                "Daily averages over complete days, against standard reference intakes for your age and sex. Tap a nutrient for its main sources."
            }
        }
        for (title , members) in groups {
            section { key: "{title}", class: "card",
                h2 { "{title}" }
                ul { class: "nutrient-list",
                    for row in rows.iter().filter(|r| members.contains(&r.nutrient.group())) {
                        {
                            let n = row.nutrient;
                            let unit = n.unit().symbol();
                            let reference = row.goal.target.or(row.goal.ceiling).unwrap_or(1.0);
                            let width = (row.average / reference * 100.0).clamp(0.0, 100.0);
                            let (label, class) = match row.standing {
                                _ if row.coverage == 0.0 => ("No data", "none"),
                                Standing::Low => ("Low", "low"),
                                Standing::Under => ("Under", "under"),
                                Standing::Met => ("On target", "met"),
                                Standing::Over => ("Over", "over"),
                            };
                            let goal_text = match (row.goal.target, row.goal.ceiling) {
                                (Some(t), Some(c)) => format!("{} / {} (max {}) {unit}", amount(row.average), amount(t), amount(c)),
                                (Some(t), None) => format!("{} / {} {unit}", amount(row.average), amount(t)),
                                (None, Some(c)) => format!("{} {unit}, max {}", amount(row.average), amount(c)),
                                (None, None) => format!("{} {unit}", amount(row.average)),
                            };
                            let is_open = open() == Some(n);
                            rsx! {
                                li { key: "{n.key()}",
                                    button {
                                        class: "nutrient-row",
                                        onclick: move |_| open.set(if is_open { None } else { Some(n) }),
                                        div { class: "nutrient-head",
                                            span { class: "nutrient-name", "{n.name()}" }
                                            span { class: "standing {class}", "{label}" }
                                        }
                                        div { class: "bar-track",
                                            div { class: "bar-fill standing-{class}", style: "width: {width:.0}%" }
                                        }
                                        div { class: "muted small", "{goal_text}" }
                                        if row.coverage > 0.0 && row.coverage < 0.8 {
                                            div { class: "muted small", "Only {row.coverage * 100.0:.0}% of foods logged have data for this." }
                                        }
                                    }
                                    if is_open {
                                        NutrientGoalEditor { key: "{n.key()}-{row.goal.target:?}-{row.goal.ceiling:?}", nutrient: n, goal: row.goal }
                                        if let Some(list) = &sources {
                                            ul { class: "nutrient-sources",
                                                if list.is_empty() {
                                                    li { class: "muted small", "None of the foods logged in this range have it." }
                                                }
                                                for (name , share) in list.iter() {
                                                    li { key: "{name}",
                                                        span { "{name}" }
                                                        span { class: "muted", "{share * 100.0:.0}%" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EmptyInsight() -> Element {
    rsx! {
        section { class: "card",
            p { class: "muted", "Log food and weigh in for a few days to see this chart." }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    #[test]
    fn ranges_start_no_earlier_than_the_data() {
        assert_eq!(Range::Week.start(d(15), d(1)), d(9));
        assert_eq!(Range::Month.start(d(15), d(1)), d(1));
        assert_eq!(Range::All.start(d(15), d(3)), d(3));
    }

    #[test]
    fn signed_values_hide_negative_zero() {
        assert_eq!(signed(-0.004, 2, "kg"), "0.00 kg");
        assert_eq!(signed(-0.43, 2, "kg"), "-0.43 kg");
        assert_eq!(signed(120.0, 0, "kcal"), "+120 kcal");
        assert_eq!(signed_energy(-14975.0, "kcal"), "-15.0k kcal");
        assert_eq!(signed_energy(-2729.0, "kcal"), "-2729 kcal");
        assert_eq!(signed_energy(-62655.0, "kJ"), "-62.7k kJ");
    }
}
