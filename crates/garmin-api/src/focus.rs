//! The day or the range of days that all pages and the coach look at.
//!
//! A single day has `start == end`. A view takes `today`: the pages read it
//! with `end`. The desktop app and the mobile bridge share this type, so both
//! clip a series and an activity list the same way.

use chrono::{Datelike, Duration, NaiveDate};

use crate::activities::ActivitiesView;
use crate::ApiError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Focus {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

/// The real date on this device.
pub fn real_today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

impl Focus {
    /// One day.
    pub fn day(day: NaiveDate) -> Self {
        Self {
            start: day,
            end: day,
        }
    }

    /// A range. The two dates can come in any order.
    pub fn range(a: NaiveDate, b: NaiveDate) -> Self {
        Self {
            start: a.min(b),
            end: a.max(b),
        }
    }

    /// The real today.
    pub fn today() -> Self {
        Self::day(real_today())
    }

    /// Read the focus from two optional `YYYY-MM-DD` dates. `to` is the last
    /// day and defaults to the real today. `from` defaults to `to`. With
    /// neither date, the result is `None`: the caller reads the live day.
    pub fn parse(from: Option<&str>, to: Option<&str>) -> Result<Option<Self>, ApiError> {
        if from.is_none() && to.is_none() {
            return Ok(None);
        }
        let end = to.map(parse_day).transpose()?.unwrap_or_else(real_today);
        let start = from.map(parse_day).transpose()?.unwrap_or(end);
        Ok(Some(Self::range(start, end)))
    }

    /// True when the focus is the single day of the real today.
    pub fn is_live(&self) -> bool {
        self.start == self.end && self.end == real_today()
    }

    /// The focus after a day rollover. A focus that follows "today" moves to
    /// the real today. Any other focus stays.
    pub fn rolled(self, follows_today: bool, today: NaiveDate) -> Self {
        if follows_today {
            Self::day(today)
        } else {
            self
        }
    }

    /// The number of days in the range, with both ends. A single day is 1.
    pub fn days(&self) -> i64 {
        (self.end - self.start).num_days() + 1
    }

    pub fn contains(&self, date: NaiveDate) -> bool {
        self.start <= date && date <= self.end
    }

    /// The text for the calendar and the headers: "Today", "Tue 14 Oct" or
    /// "14 - 20 Oct 2026".
    pub fn label(&self) -> String {
        label_for(*self, real_today())
    }

    /// A daily series that ends on the end of the focus, as dated points. A
    /// range keeps only the days inside it. `values` is the series, oldest
    /// first.
    pub fn dated(&self, values: &[Option<f64>]) -> Vec<(NaiveDate, Option<f64>)> {
        let n = values.len() as i64;
        values
            .iter()
            .enumerate()
            .map(|(i, v)| (self.end - Duration::days(n - 1 - i as i64), *v))
            .filter(|(d, _)| self.days() <= 1 || self.contains(*d))
            .collect()
    }
}

fn parse_day(text: &str) -> Result<NaiveDate, ApiError> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .map_err(|_| ApiError::BadArguments("a date must be YYYY-MM-DD".into()))
}

fn label_for(f: Focus, today: NaiveDate) -> String {
    if f.start == f.end {
        if f.end == today {
            return "Today".into();
        }
        return f.end.format("%a %-d %b").to_string();
    }
    let (s, e) = (f.start, f.end);
    if s.year() != e.year() {
        return format!("{} - {}", s.format("%-d %b %Y"), e.format("%-d %b %Y"));
    }
    if s.month() != e.month() {
        return format!("{} - {}", s.format("%-d %b"), e.format("%-d %b %Y"));
    }
    format!("{} - {}", s.day(), e.format("%-d %b %Y"))
}

/// Drop the day groups outside a range. A single day keeps every group up to
/// that day, because the read already hides the later days.
pub fn clip_activities(view: &mut ActivitiesView, focus: Focus) {
    if focus.days() <= 1 {
        return;
    }
    view.groups.retain(|g| {
        NaiveDate::parse_from_str(&g.date, "%Y-%m-%d").is_ok_and(|d| focus.contains(d))
    });
    view.rows.retain(|r| {
        NaiveDate::parse_from_str(&r.date, "%Y-%m-%d").is_ok_and(|d| focus.contains(d))
    });
    view.empty = view.groups.is_empty();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn the_label_follows_the_range() {
        let today = d(2026, 10, 20);
        assert_eq!(label_for(Focus::day(today), today), "Today");
        assert_eq!(label_for(Focus::day(d(2026, 10, 14)), today), "Wed 14 Oct");
        let week = Focus::range(d(2026, 10, 20), d(2026, 10, 14));
        assert_eq!(label_for(week, today), "14 - 20 Oct 2026");
        let months = Focus::range(d(2026, 9, 28), d(2026, 10, 4));
        assert_eq!(label_for(months, today), "28 Sep - 4 Oct 2026");
        let years = Focus::range(d(2026, 12, 30), d(2027, 1, 5));
        assert_eq!(label_for(years, today), "30 Dec 2026 - 5 Jan 2027");
    }

    #[test]
    fn a_live_focus_follows_the_day_rollover() {
        let tomorrow = d(2026, 10, 21);
        let yesterday = Focus::day(d(2026, 10, 20));
        assert_eq!(yesterday.rolled(true, tomorrow), Focus::day(tomorrow));
        assert_eq!(yesterday.rolled(false, tomorrow), yesterday);
    }

    #[test]
    fn days_and_contains_count_both_ends() {
        let f = Focus::range(d(2026, 10, 14), d(2026, 10, 20));
        assert_eq!(f.days(), 7);
        assert!(f.contains(d(2026, 10, 14)) && f.contains(d(2026, 10, 20)));
        assert!(!f.contains(d(2026, 10, 21)));
        assert_eq!(Focus::day(d(2026, 10, 14)).days(), 1);
        assert!(Focus::today().is_live());
        assert!(!f.is_live());
    }

    #[test]
    fn a_series_ends_on_the_end_and_a_range_clips_it() {
        let values = [Some(1.0), None, Some(3.0), Some(4.0)];
        let day = Focus::day(d(2026, 10, 20)).dated(&values);
        assert_eq!(day.len(), 4);
        assert_eq!(day[3], (d(2026, 10, 20), Some(4.0)));
        let week = Focus::range(d(2026, 10, 19), d(2026, 10, 20)).dated(&values);
        assert_eq!(
            week,
            [(d(2026, 10, 19), Some(3.0)), (d(2026, 10, 20), Some(4.0))]
        );
    }

    #[test]
    fn a_range_drops_the_activity_days_outside_it() {
        let mut view: ActivitiesView =
            serde_json::from_str(include_str!("../fixtures/activities.sample.json")).unwrap();
        let first = view.groups[0].date.clone();
        let day = NaiveDate::parse_from_str(&first, "%Y-%m-%d").unwrap();
        let all = view.groups.len();
        assert!(all > 1, "the sample needs more than one day");
        // One day keeps every group.
        clip_activities(&mut view, Focus::day(day));
        assert_eq!(view.groups.len(), all);
        clip_activities(&mut view, Focus::range(day, day + Duration::days(1)));
        assert_eq!(view.groups.len(), 1);
        assert_eq!(view.groups[0].date, first);
        clip_activities(
            &mut view,
            Focus::range(day + Duration::days(2), day + Duration::days(3)),
        );
        assert!(view.groups.is_empty() && view.empty);
    }

    #[test]
    fn two_dates_make_a_focus_and_a_bad_date_is_refused() {
        assert_eq!(Focus::parse(None, None).unwrap(), None);
        let f = Focus::parse(Some("2026-10-20"), Some("2026-10-14")).unwrap();
        assert_eq!(f, Some(Focus::range(d(2026, 10, 14), d(2026, 10, 20))));
        let one = Focus::parse(None, Some("2026-10-14")).unwrap();
        assert_eq!(one, Some(Focus::day(d(2026, 10, 14))));
        assert!(matches!(
            Focus::parse(Some("soon"), None),
            Err(ApiError::BadArguments(_))
        ));
    }
}
