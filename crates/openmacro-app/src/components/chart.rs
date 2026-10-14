//! A small SVG time-series chart: a line with an optional band, dots and
//! bars around zero, with value gridlines and date labels.

use std::collections::BTreeMap;

use chrono::{Datelike, Duration, NaiveDate};
use dioxus::prelude::*;

/// How points are grouped before drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Grouping {
    #[default]
    Day,
    /// Averaged per week, dated by its Monday.
    Week,
    /// Averaged per calendar month, dated by its first day.
    Month,
}

impl Grouping {
    pub const ALL: [Grouping; 3] = [Grouping::Day, Grouping::Week, Grouping::Month];

    pub fn label(self) -> &'static str {
        match self {
            Grouping::Day => "D",
            Grouping::Week => "W",
            Grouping::Month => "M",
        }
    }

    fn period_start(self, date: NaiveDate) -> NaiveDate {
        match self {
            Grouping::Day => date,
            Grouping::Week => date - Duration::days(date.weekday().num_days_from_monday() as i64),
            Grouping::Month => date.with_day(1).unwrap_or(date),
        }
    }
}

/// Averages points per period, oldest first.
pub fn group(points: &[(NaiveDate, f64)], grouping: Grouping) -> Vec<(NaiveDate, f64)> {
    if grouping == Grouping::Day {
        return points.to_vec();
    }
    let mut sums: BTreeMap<NaiveDate, (f64, u32)> = BTreeMap::new();
    for (date, value) in points {
        let e = sums.entry(grouping.period_start(*date)).or_insert((0.0, 0));
        e.0 += value;
        e.1 += 1;
    }
    sums.into_iter()
        .map(|(d, (sum, n))| (d, sum / n as f64))
        .collect()
}

/// What to draw. Every series is keyed by date; `start..=end` is the x axis.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChartData {
    pub start: Option<NaiveDate>,
    pub end: Option<NaiveDate>,
    pub line: Vec<(NaiveDate, f64)>,
    /// Lower and upper bound around the line.
    pub band: Vec<(NaiveDate, f64, f64)>,
    pub dots: Vec<(NaiveDate, f64)>,
    /// Drawn from zero; negative values go down.
    pub bars: Vec<(NaiveDate, f64)>,
    /// Draw bars in one colour rather than by sign (above or below zero).
    pub plain_bars: bool,
    /// A dashed line, e.g. a target or a projection.
    pub dashed: Vec<(NaiveDate, f64)>,
    /// A dashed horizontal line across the chart, e.g. a goal.
    pub level: Option<f64>,
    pub decimals: usize,
    pub grouping: Grouping,
}

const WIDTH: f64 = 360.0;
const HEIGHT: f64 = 220.0;
const LEFT: f64 = 44.0;
const RIGHT: f64 = 8.0;
const TOP: f64 = 10.0;
const BOTTOM: f64 = 24.0;

/// Up to about `count` evenly spaced round values covering `min..=max`.
pub fn nice_ticks(min: f64, max: f64, count: usize) -> Vec<f64> {
    let (min, max) = if (max - min).abs() < 1e-9 {
        (min - 1.0, max + 1.0)
    } else {
        (min, max)
    };
    let rough = (max - min) / count.max(1) as f64;
    let magnitude = 10f64.powf(rough.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0]
        .iter()
        .map(|m| m * magnitude)
        .find(|s| *s >= rough)
        .unwrap_or(10.0 * magnitude);
    let first = (min / step).floor() * step;
    // From the first tick at or below `min` to the first at or above `max`.
    (0..20)
        .map(|i| first + i as f64 * step)
        .take_while(|v| *v < max + step - 1e-9)
        .collect()
}

/// Applies the data's grouping to every series.
fn grouped(mut data: ChartData) -> ChartData {
    let g = data.grouping;
    if g == Grouping::Day {
        return data;
    }
    data.line = group(&data.line, g);
    data.dots = group(&data.dots, g);
    data.bars = group(&data.bars, g);
    data.dashed = group(&data.dashed, g);
    let lower: Vec<_> = data.band.iter().map(|b| (b.0, b.1)).collect();
    let upper: Vec<_> = data.band.iter().map(|b| (b.0, b.2)).collect();
    data.band = group(&lower, g)
        .into_iter()
        .zip(group(&upper, g))
        .map(|((d, lo), (_, hi))| (d, lo, hi))
        .collect();
    data.start = data.start.map(|d| g.period_start(d));
    data
}

fn label(value: f64, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    if text.starts_with("-0") && text.trim_start_matches(['-', '0', '.']).is_empty() {
        text[1..].to_string()
    } else {
        text
    }
}

#[component]
pub fn Chart(data: ChartData, class: &'static str) -> Element {
    let data = grouped(data);
    let (Some(start), Some(end)) = (data.start, data.end) else {
        return rsx! {
            div { class: "chart-empty muted", "Not enough data yet." }
        };
    };
    let values = data
        .line
        .iter()
        .map(|p| p.1)
        .chain(data.band.iter().flat_map(|b| [b.1, b.2]))
        .chain(data.dots.iter().map(|p| p.1))
        .chain(data.bars.iter().map(|p| p.1))
        .chain(data.dashed.iter().map(|p| p.1))
        .chain(data.level)
        .chain((!data.bars.is_empty()).then_some(0.0));
    let (min, max) = values.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
        (lo.min(v), hi.max(v))
    });
    if !min.is_finite() {
        return rsx! {
            div { class: "chart-empty muted", "Not enough data yet." }
        };
    }
    let ticks = nice_ticks(min, max, 4);
    let lo = ticks.first().copied().unwrap_or(min).min(min);
    let hi = ticks.last().copied().unwrap_or(max).max(max);
    let span_days = (end - start).num_days().max(1) as f64;
    let plot_w = WIDTH - LEFT - RIGHT;
    let plot_h = HEIGHT - TOP - BOTTOM;
    let x = move |d: NaiveDate| LEFT + (d - start).num_days() as f64 / span_days * plot_w;
    let y = move |v: f64| TOP + (hi - v) / (hi - lo).max(1e-9) * plot_h;

    let line = data
        .line
        .iter()
        .enumerate()
        .map(|(i, (d, v))| {
            format!(
                "{}{:.1},{:.1}",
                if i == 0 { "M" } else { "L" },
                x(*d),
                y(*v)
            )
        })
        .collect::<String>();
    let dashed = data
        .dashed
        .iter()
        .enumerate()
        .map(|(i, (d, v))| {
            format!(
                "{}{:.1},{:.1}",
                if i == 0 { "M" } else { "L" },
                x(*d),
                y(*v)
            )
        })
        .collect::<String>();
    let level = data.level.map(y);
    let band = data
        .band
        .iter()
        .map(|(d, _, upper)| format!("{:.1},{:.1}", x(*d), y(*upper)))
        .chain(
            data.band
                .iter()
                .rev()
                .map(|(d, lower, _)| format!("{:.1},{:.1}", x(*d), y(*lower))),
        )
        .collect::<Vec<_>>()
        .join(" ");
    // Bars fill most of the gap to their neighbour (a day, a week or a month).
    let gap_days = data
        .bars
        .windows(2)
        .map(|w| (w[1].0 - w[0].0).num_days())
        .filter(|d| *d > 0)
        .min()
        .unwrap_or(1) as f64;
    let bar_w = (plot_w / (span_days + 1.0) * gap_days * 0.7).clamp(1.0, 24.0);
    let zero = y(0.0);
    let bars: Vec<(f64, f64, f64, bool, f64)> = data
        .bars
        .iter()
        .map(|(d, v)| {
            let top = y(v.max(0.0));
            let height = (y(v.min(0.0)) - top).max(0.5);
            // Keep bars inside the plot, clear of the value labels.
            let left = (x(*d) - bar_w / 2.0).max(LEFT + 1.0);
            let right = (x(*d) + bar_w / 2.0).min(WIDTH - RIGHT);
            (left, top, height, *v < 0.0, (right - left).max(1.0))
        })
        .collect();
    let mid = start + chrono::Duration::days((span_days / 2.0) as i64);
    let date_labels = [
        (x(start), "start", start),
        (x(mid), "middle", mid),
        (x(end), "end", end),
    ];
    let decimals = data.decimals;
    let plain_bars = data.plain_bars;

    rsx! {
        svg {
            class: "chart {class}",
            "viewBox": "0 0 {WIDTH} {HEIGHT}",
            role: "img",
            for tick in ticks.iter().copied().filter(|t| *t >= lo && *t <= hi) {
                g { key: "{tick}",
                    line {
                        class: "chart-grid",
                        x1: "{LEFT}",
                        x2: "{WIDTH - RIGHT}",
                        y1: "{y(tick):.1}",
                        y2: "{y(tick):.1}",
                    }
                    text {
                        class: "chart-label",
                        x: "{LEFT - 6.0}",
                        y: "{y(tick) + 3.5:.1}",
                        "text-anchor": "end",
                        {label(tick, decimals)}
                    }
                }
            }
            if !data.bars.is_empty() {
                line {
                    class: "chart-zero",
                    x1: "{LEFT}",
                    x2: "{WIDTH - RIGHT}",
                    y1: "{zero:.1}",
                    y2: "{zero:.1}",
                }
            }
            if !band.is_empty() {
                polygon { class: "chart-band", points: "{band}" }
            }
            for (i , (left , top , height , negative , width)) in bars.into_iter().enumerate() {
                rect {
                    key: "{i}",
                    class: match (plain_bars, negative) {
                        (true, _) => "chart-bar intake",
                        (false, true) => "chart-bar below",
                        (false, false) => "chart-bar above",
                    },
                    x: "{left:.1}",
                    y: "{top:.1}",
                    width: "{width:.1}",
                    height: "{height:.1}",
                    rx: "1",
                }
            }
            if let Some(level) = level {
                line {
                    class: "chart-level",
                    x1: "{LEFT}",
                    x2: "{WIDTH - RIGHT}",
                    y1: "{level:.1}",
                    y2: "{level:.1}",
                }
            }
            if !dashed.is_empty() {
                path { class: "chart-dashed", d: "{dashed}" }
            }
            if !line.is_empty() {
                path { class: "chart-line", d: "{line}" }
            }
            for (i , (d , v)) in data.dots.iter().enumerate() {
                circle {
                    key: "{i}",
                    class: "chart-dot",
                    cx: "{x(*d):.1}",
                    cy: "{y(*v):.1}",
                    r: "2.5",
                }
            }
            for (pos , anchor , date) in date_labels {
                text {
                    key: "{anchor}",
                    class: "chart-label",
                    x: "{pos:.1}",
                    y: "{HEIGHT - 6.0}",
                    "text-anchor": anchor,
                    {date.format("%-d %b").to_string()}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_are_round_and_cover_the_range() {
        assert_eq!(
            nice_ticks(2180.0, 2410.0, 4),
            vec![2100.0, 2200.0, 2300.0, 2400.0, 2500.0]
        );
        assert_eq!(
            nice_ticks(69.6, 72.3, 4),
            vec![69.0, 70.0, 71.0, 72.0, 73.0]
        );
        assert_eq!(
            nice_ticks(-620.0, 180.0, 4),
            vec![-800.0, -600.0, -400.0, -200.0, 0.0, 200.0]
        );
        assert_eq!(
            nice_ticks(0.0, 100.0, 4),
            vec![0.0, 25.0, 50.0, 75.0, 100.0]
        );
        // A flat series still gets a range.
        assert!(nice_ticks(70.0, 70.0, 4).len() >= 2);
    }

    #[test]
    fn grouping_averages_per_week_and_month() {
        let d = |m: u32, day: u32| NaiveDate::from_ymd_opt(2026, m, day).unwrap();
        // 12 October 2026 is a Monday.
        let points = [
            (d(10, 12), 1.0),
            (d(10, 14), 3.0),
            (d(10, 19), 10.0),
            (d(11, 2), 20.0),
        ];
        assert_eq!(
            group(&points, Grouping::Week),
            vec![(d(10, 12), 2.0), (d(10, 19), 10.0), (d(11, 2), 20.0)]
        );
        assert_eq!(
            group(&points, Grouping::Month),
            vec![(d(10, 1), 14.0 / 3.0), (d(11, 1), 20.0)]
        );
        assert_eq!(group(&points, Grouping::Day), points.to_vec());
    }

    #[test]
    fn labels_drop_negative_zero() {
        assert_eq!(label(-0.0, 0), "0");
        assert_eq!(label(-0.04, 1), "0.0");
        assert_eq!(label(-250.0, 0), "-250");
        assert_eq!(label(70.5, 1), "70.5");
    }
}
