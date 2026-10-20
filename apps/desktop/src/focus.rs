//! The day or the range of days that all pages and the coach look at.
//!
//! A single day has `start == end`. A garmin-api view takes `today`: the
//! pages read it with `end`.

use chrono::{Datelike, NaiveDate};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Focus {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

/// The real date on this computer.
fn real_today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

#[allow(dead_code)] // The calendar and the screens use these later.
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

    /// True when the focus is the single day of the real today.
    pub fn is_live(&self) -> bool {
        self.start == self.end && self.end == real_today()
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
    fn days_and_contains_count_both_ends() {
        let f = Focus::range(d(2026, 10, 14), d(2026, 10, 20));
        assert_eq!(f.days(), 7);
        assert!(f.contains(d(2026, 10, 14)) && f.contains(d(2026, 10, 20)));
        assert!(!f.contains(d(2026, 10, 21)));
        assert_eq!(Focus::day(d(2026, 10, 14)).days(), 1);
        assert!(Focus::today().is_live());
        assert!(!f.is_live());
    }
}
