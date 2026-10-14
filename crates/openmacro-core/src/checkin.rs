//! Weekly check-ins: when they're due and which days to review.
//!
//! Targets are recalculated at a weekly check-in rather than every day, so
//! the plan is stable for a week. Before a check-in the user reviews days
//! that look incomplete (logged far below target) or unlogged (were they
//! fasts?), since both change the expenditure estimate.

use chrono::{Datelike, Duration, NaiveDate, Weekday};

use crate::expenditure::DayStatus;
use crate::log::Goal;
use crate::targets::MacroTargets;

/// A completed check-in and the targets it set.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CheckIn {
    pub date: NaiveDate,
    pub expenditure_kcal: f64,
    pub trend_kg: Option<f64>,
    /// Monday through Sunday.
    pub week: Vec<MacroTargets>,
    /// The goal in force at the check-in, for the goal history. Absent in
    /// check-ins saved before it was recorded.
    #[cfg_attr(feature = "serde", serde(default))]
    pub goal: Option<Goal>,
}

/// The next date on or after `from` that falls on `weekday`.
pub fn next_weekday(from: NaiveDate, weekday: Weekday) -> NaiveDate {
    let ahead = (7 + weekday.num_days_from_monday() as i64
        - from.weekday().num_days_from_monday() as i64)
        % 7;
    from + Duration::days(ahead)
}

/// Fewest days between check-ins. Checking in early or late moves the next
/// one back onto the check-in weekday, so gaps stay between this and
/// `MIN_GAP_DAYS + 6`, centred on a week.
pub const MIN_GAP_DAYS: i64 = 4;

/// When the next check-in is due. Before the first check-in it's the next
/// check-in weekday (today if today is that day). Afterwards it's the first
/// check-in weekday at least [`MIN_GAP_DAYS`] after the last one.
pub fn due_date(today: NaiveDate, weekday: Weekday, last: Option<NaiveDate>) -> NaiveDate {
    match last {
        None => next_weekday(today, weekday),
        Some(last) => next_weekday(last + Duration::days(MIN_GAP_DAYS), weekday),
    }
}

/// What a day under review might be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Suggestion {
    /// Nothing logged: unlogged days are ignored unless marked as fasts.
    MaybeFasted,
    /// Logged well below target: maybe only partly logged.
    MaybePartial,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DayReview {
    pub date: NaiveDate,
    pub logged_kcal: Option<f64>,
    pub suggestion: Suggestion,
}

/// Share of the day's calorie target below which a day looks incomplete.
pub const PARTIAL_THRESHOLD: f64 = 0.5;

/// Days in `[from, to]` worth confirming before a check-in. `day` returns the
/// logged energy (None when nothing was logged), the day's status and its
/// calorie target. Days already marked partial or fasted are skipped.
pub fn days_to_review(
    from: NaiveDate,
    to: NaiveDate,
    mut day: impl FnMut(NaiveDate) -> (Option<f64>, DayStatus, f64),
) -> Vec<DayReview> {
    from.iter_days()
        .take_while(|d| *d <= to)
        .filter_map(|date| {
            let (logged, status, target) = day(date);
            if status != DayStatus::Normal {
                return None;
            }
            let suggestion = match logged {
                None => Suggestion::MaybeFasted,
                Some(kcal) if kcal < target * PARTIAL_THRESHOLD => Suggestion::MaybePartial,
                Some(_) => return None,
            };
            Some(DayReview {
                date,
                logged_kcal: logged,
                suggestion,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        // 12 October 2026 is a Monday.
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    #[test]
    fn next_weekday_includes_today() {
        assert_eq!(next_weekday(d(12), Weekday::Mon), d(12));
        assert_eq!(next_weekday(d(13), Weekday::Mon), d(19));
        assert_eq!(next_weekday(d(15), Weekday::Sat), d(17));
    }

    #[test]
    fn due_dates() {
        // First check-in: the next Monday, or today if it's Monday.
        assert_eq!(due_date(d(15), Weekday::Mon, None), d(19));
        assert_eq!(due_date(d(19), Weekday::Mon, None), d(19));
        // After checking in on Monday the 12th, next is Monday the 19th.
        assert_eq!(due_date(d(14), Weekday::Mon, Some(d(12))), d(19));
        // Two days early (Saturday the 17th) is too close; next is the 26th.
        assert_eq!(due_date(d(18), Weekday::Mon, Some(d(17))), d(26));
        // Four days early (Thursday the 15th) keeps Monday the 19th.
        assert_eq!(due_date(d(16), Weekday::Mon, Some(d(15))), d(19));
        // Two days late (Wednesday the 21st) is back on Monday the 26th.
        assert_eq!(due_date(d(22), Weekday::Mon, Some(d(21))), d(26));
    }

    #[test]
    fn review_flags_unlogged_and_low_days() {
        let reviews = days_to_review(d(12), d(16), |date| match date.day() {
            12 => (Some(1800.0), DayStatus::Normal, 2000.0),
            13 => (None, DayStatus::Normal, 2000.0),
            14 => (Some(600.0), DayStatus::Normal, 2000.0),
            15 => (None, DayStatus::Fasted, 2000.0),
            _ => (Some(300.0), DayStatus::Partial, 2000.0),
        });
        assert_eq!(
            reviews,
            vec![
                DayReview {
                    date: d(13),
                    logged_kcal: None,
                    suggestion: Suggestion::MaybeFasted
                },
                DayReview {
                    date: d(14),
                    logged_kcal: Some(600.0),
                    suggestion: Suggestion::MaybePartial
                },
            ]
        );
    }
}
