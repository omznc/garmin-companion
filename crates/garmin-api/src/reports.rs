//! The weekly report as one view model.
//!
//! A week runs from Monday to Sunday. The report sets it against the week
//! before. Distance and the heart-rate split show only when the week has them,
//! so a golfer or a gym user reads a report about time and sessions.

use crate::body::EmptyNote;
use crate::derive::{
    daily_series, date_key, easy_hard_split, fuel, hydration, is_run, mean, Fuel, Hydration,
    MIN_RUNS_FOR_NOTES,
};
use crate::insights::Figure;
use crate::text::{hours_minutes, num, plural};
use crate::ApiError;
use chrono::{Datelike, Duration, NaiveDate};
use garmin_core::sport::{is_exercise, sport_label};
use garmin_core::{CachedActivity, DailyMetrics, Db};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const HISTORY_DAYS: i64 = 120;
const DAY_NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DayBar {
    pub label: String,
    pub value: f64,
    /// For example `5.2 km` or `45 min`.
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportSession {
    pub activity_id: i64,
    pub name: String,
    /// The weekday and date, such as `Mon 09 Mar`.
    pub day: String,
    /// The sport, or empty when the name already says it.
    pub sport: String,
    /// Empty for a session with no distance.
    pub distance: String,
    pub duration: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportView {
    pub date: String,
    pub empty: Option<EmptyNote>,
    pub lede: String,
    /// The week shown. 0 is the newest week with a session.
    pub offset: usize,
    pub weeks: usize,
    pub has_earlier: bool,
    pub has_later: bool,
    /// For example `No. 6 · 02 Mar - 08 Mar`.
    pub eyebrow: String,
    pub headline: String,
    pub paragraphs: Vec<String>,
    pub figures: Vec<Figure>,
    pub daily_title: String,
    pub daily: Vec<DayBar>,
    pub sessions: Vec<ReportSession>,
}

struct Week<'a> {
    start: NaiveDate,
    activities: Vec<&'a CachedActivity>,
    distance_m: f64,
    duration_s: f64,
}

fn date_of(a: &CachedActivity) -> Option<NaiveDate> {
    let text = a
        .start_time_local
        .as_deref()
        .or(a.local_date.as_deref())?
        .get(..10)?;
    NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()
}

/// Weeks with at least one session, newest first.
fn by_week(activities: &[CachedActivity]) -> Vec<Week<'_>> {
    let mut buckets: BTreeMap<NaiveDate, Vec<&CachedActivity>> = BTreeMap::new();
    for a in activities {
        if let Some(d) = date_of(a) {
            let start = d - Duration::days(d.weekday().num_days_from_monday() as i64);
            buckets.entry(start).or_default().push(a);
        }
    }
    buckets
        .into_iter()
        .rev()
        .map(|(start, activities)| Week {
            start,
            distance_m: total(activities.iter().filter_map(|a| a.distance_m)),
            duration_s: total(activities.iter().filter_map(|a| a.duration_s)),
            activities,
        })
        .collect()
}

impl Week<'_> {
    fn end(&self) -> NaiveDate {
        self.start + Duration::days(6)
    }

    fn runs(&self) -> Vec<&CachedActivity> {
        self.activities
            .iter()
            .copied()
            .filter(|a| is_run(&a.type_key))
            .collect()
    }
}

/// The sum of the values. An empty list gives `0.0`, not `-0.0` as `Iterator::sum` does.
fn total(values: impl Iterator<Item = f64>) -> f64 {
    values.fold(0.0, |a, b| a + b)
}

fn short(d: NaiveDate) -> String {
    d.format("%d %b").to_string()
}

/// `39 min`, or `1h 05m`.
fn duration(secs: f64) -> String {
    if secs < 3600.0 {
        format!("{} min", (secs / 60.0).round() as i64)
    } else {
        hours_minutes(secs)
    }
}

/// The share of heart-rate time above Z2 in the runs of the week.
fn hard_share(week: &Week) -> Option<(f64, f64, usize)> {
    easy_hard_split(&week.runs())
        .filter(|s| s.counted >= MIN_RUNS_FOR_NOTES)
        .map(|s| (s.hard_pct, s.easy_pct, s.counted))
}

/// What the week is set against.
enum Compare<'a> {
    /// No earlier session exists in the cache.
    First,
    /// The week is not over, so a whole week is no fair match.
    Partial,
    /// The week before had no session.
    Gap,
    Week(&'a Week<'a>),
}

fn headline(week: &Week, compare: &Compare) -> String {
    let prior = match compare {
        Compare::First => return "Your first week in the cache".into(),
        Compare::Week(p) => Some(*p),
        _ => None,
    };
    let share = hard_share(week);
    if share.is_some_and(|(hard, ..)| hard <= 25.0) {
        "A week close to the 80/20 split".into()
    } else if prior.is_some_and(|p| p.duration_s > 0.0 && week.duration_s > p.duration_s * 1.25) {
        "A bigger week than the one before".into()
    } else if share.is_some_and(|(hard, ..)| hard > 60.0) {
        "A hard week, front to back".into()
    } else {
        "A steady week".into()
    }
}

fn body(week: &Week, compare: &Compare) -> String {
    let mut parts = vec![plural(week.activities.len(), "session", "sessions")];
    if week.distance_m > 0.0 {
        parts.push(format!("{:.1} km", week.distance_m / 1000.0));
    }
    parts.push(format!(
        "{} of recorded time",
        hours_minutes(week.duration_s)
    ));
    let mut out = format!("{}.", parts.join(", "));
    match compare {
        Compare::Week(p) if p.duration_s > 0.0 => {
            let change = (week.duration_s - p.duration_s) / p.duration_s * 100.0;
            out.push_str(&format!(
                " That is {:.0}% {} time than the week before.",
                change.abs(),
                if change >= 0.0 { "more" } else { "less" }
            ));
        }
        Compare::Gap => out.push_str(" The week before had no sessions."),
        Compare::Partial => out.push_str(" The week is not over yet, so it has no comparison."),
        _ => {}
    }
    let longest = week.activities.iter().max_by(|a, b| {
        a.duration_s
            .unwrap_or(0.0)
            .total_cmp(&b.duration_s.unwrap_or(0.0))
    });
    if let Some(a) = longest {
        out.push_str(&format!(
            " The longest was {} at {}.",
            a.name.as_deref().unwrap_or("an untitled session"),
            duration(a.duration_s.unwrap_or(0.0))
        ));
    }
    out
}

fn food_sentence(f: &Fuel) -> Option<String> {
    if f.logged >= 3 && f.avg_balance.is_some() {
        let avg = f.avg_balance.unwrap_or(0.0).round();
        return Some(format!(
            "Food was logged on {} of {} days. The average was {} {} kcal, against {} kcal burned.",
            f.logged,
            f.window,
            if avg > 0.0 {
                "a surplus of"
            } else {
                "a deficit of"
            },
            num(avg.abs()),
            num(f.avg_burn.unwrap_or(0.0))
        ));
    }
    if f.logged > 0 {
        return Some(format!(
            "Food was logged on {} of {}. That is too little to average.",
            f.logged,
            plural(f.window, "day", "days")
        ));
    }
    f.avg_burn.map(|b| {
        format!(
            "No food was logged, against {} kcal burned each day.",
            num(b)
        )
    })
}

fn water_sentence(w: &Hydration) -> String {
    let litres = |ml: f64| format!("{:.2} L", ml / 1000.0);
    let days = plural(w.logged, "logged day", "logged days");
    match w.goal_ml {
        Some(goal) => format!(
            "Water averaged {} a day over {days}, against a goal of {}.",
            litres(w.avg_ml),
            litres(goal)
        ),
        None => format!("Water averaged {} a day over {days}.", litres(w.avg_ml)),
    }
}

fn recovery(week: &Week, rows: &[DailyMetrics]) -> String {
    let inside: Vec<DailyMetrics> = rows
        .iter()
        .filter(|r| r.date >= date_key(week.start) && r.date <= date_key(week.end()))
        .cloned()
        .collect();
    let mut parts = Vec::new();
    if let Some(s) = mean(inside.iter().map(|d| d.sleep_secs)) {
        parts.push(format!("Sleep averaged {}.", hours_minutes(s)));
    }
    if let Some(r) = mean(inside.iter().map(|d| d.resting_hr)) {
        parts.push(format!("Resting heart rate averaged {} bpm.", num(r)));
    }
    let days = inside.len().max(1);
    parts.extend(food_sentence(&fuel(&inside, days)));
    parts.extend(hydration(&inside, days).as_ref().map(water_sentence));
    if let Some((hard, easy, counted)) = hard_share(week) {
        let sessions = plural(counted, "run", "runs");
        parts.push(if hard > 30.0 {
            format!("{hard:.0}% of the heart-rate time was above Z2, over {sessions} that recorded it. The 80/20 model wants this nearer 20%.")
        } else {
            format!("{easy:.0}% of the heart-rate time was in Z1 and Z2. A week that builds a base sits there.")
        });
    }
    if parts.is_empty() {
        "No recovery data was recorded for this week.".into()
    } else {
        parts.join(" ")
    }
}

fn figures(week: &Week, rows: &[DailyMetrics]) -> Vec<Figure> {
    let plain = |label: &str, value: String| Figure {
        label: label.into(),
        value,
        accent: false,
    };
    let mut out = Vec::new();
    if week.distance_m > 0.0 {
        out.push(plain(
            "Distance",
            format!("{:.1} km", week.distance_m / 1000.0),
        ));
    }
    out.push(plain("Time", hours_minutes(week.duration_s)));
    out.push(plain("Sessions", num(week.activities.len() as f64)));
    let sleep = mean(
        rows.iter()
            .filter(|r| r.date >= date_key(week.start) && r.date <= date_key(week.end()))
            .map(|d| d.sleep_secs),
    );
    if let Some(s) = sleep {
        out.push(plain("Avg sleep", hours_minutes(s)));
    }
    if let Some((hard, ..)) = hard_share(week) {
        out.push(Figure {
            label: "Time above Z2".into(),
            value: format!("{hard:.0}%"),
            accent: hard > 30.0,
        });
    }
    out
}

fn daily(week: &Week) -> (String, Vec<DayBar>) {
    let by_distance = week.distance_m > 0.0;
    let bars = DAY_NAMES
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let key = date_key(week.start + Duration::days(i as i64));
            let value = total(
                week.activities
                    .iter()
                    .filter(|a| a.local_date.as_deref() == Some(key.as_str()))
                    .map(|a| {
                        if by_distance {
                            a.distance_m.unwrap_or(0.0) / 1000.0
                        } else {
                            a.duration_s.unwrap_or(0.0) / 60.0
                        }
                    }),
            );
            DayBar {
                label: (*name).into(),
                value,
                text: if by_distance {
                    format!("{value:.1} km")
                } else {
                    format!("{value:.0} min")
                },
            }
        })
        .collect();
    let title = if by_distance {
        "Daily distance"
    } else {
        "Daily active time"
    };
    (title.into(), bars)
}

fn session_rows(week: &Week) -> Vec<ReportSession> {
    week.activities
        .iter()
        .map(|a| {
            let name = a.name.clone().unwrap_or_else(|| "Untitled".into());
            let sport = sport_label(a.type_key.as_deref());
            ReportSession {
                activity_id: a.activity_id,
                day: date_of(a).map_or_else(String::new, |d| d.format("%a %d %b").to_string()),
                sport: if sport.to_lowercase().starts_with(&name.to_lowercase()) {
                    String::new()
                } else {
                    sport
                },
                name,
                distance: a
                    .distance_m
                    .filter(|m| *m > 0.0)
                    .map_or_else(String::new, |m| format!("{:.1} km", m / 1000.0)),
                duration: duration(a.duration_s.unwrap_or(0.0)),
            }
        })
        .collect()
}

fn empty_view(today: NaiveDate) -> ReportView {
    ReportView {
        date: date_key(today),
        empty: Some(EmptyNote {
            title: "No weeks to report on.".into(),
            body: "The weekly report is built from synced activities. Sync some history and the report writes itself.".into(),
        }),
        lede: "Monday to Sunday, written up and set against the week before.".into(),
        offset: 0,
        weeks: 0,
        has_earlier: false,
        has_later: false,
        eyebrow: "No weeks yet".into(),
        headline: String::new(),
        paragraphs: Vec::new(),
        figures: Vec::new(),
        daily_title: String::new(),
        daily: Vec::new(),
        sessions: Vec::new(),
    }
}

fn from_day(key: &str) -> NaiveDate {
    NaiveDate::parse_from_str(key, "%Y-%m-%d").unwrap_or(NaiveDate::MIN)
}

fn compare_for<'a>(
    week: &Week,
    older: Option<&'a Week<'a>>,
    today: NaiveDate,
    window_start: NaiveDate,
) -> Compare<'a> {
    if week.end() >= today {
        return Compare::Partial;
    }
    match older {
        Some(o) if o.start == week.start - Duration::days(7) => Compare::Week(o),
        Some(_) => Compare::Gap,
        // The window is 120 days. A week at its edge may have an earlier week outside it.
        None if week.start > window_start + Duration::days(7) => Compare::First,
        None => Compare::Gap,
    }
}

/// Build the report for the week at `offset`. 0 is the newest week with a
/// session. An offset past the oldest week gives the oldest week.
pub fn report_view(db: &Db, today: NaiveDate, offset: usize) -> Result<ReportView, ApiError> {
    let from = date_key(today - Duration::days(HISTORY_DAYS - 1));
    let activities: Vec<CachedActivity> = db
        .activities_since(&from)?
        .into_iter()
        .filter(|a| is_exercise(a.type_key.as_deref()))
        .collect();
    let weeks = by_week(&activities);
    if weeks.is_empty() {
        return Ok(empty_view(today));
    }
    let rows = daily_series(db.daily_since(&from)?, HISTORY_DAYS as u32, today);
    let offset = offset.min(weeks.len() - 1);
    let week = &weeks[offset];
    let compare = compare_for(week, weeks.get(offset + 1), today, from_day(&from));
    let (daily_title, daily) = daily(week);
    Ok(ReportView {
        date: date_key(today),
        empty: None,
        lede: "Monday to Sunday, written up and set against the week before.".into(),
        offset,
        weeks: weeks.len(),
        has_earlier: offset + 1 < weeks.len(),
        has_later: offset > 0,
        eyebrow: format!(
            "No. {} · {} - {}",
            weeks.len() - offset,
            short(week.start),
            short(week.end())
        ),
        headline: headline(week, &compare),
        paragraphs: vec![body(week, &compare), recovery(week, &rows)],
        figures: figures(week, &rows),
        daily_title,
        daily,
        sessions: session_rows(week),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn act(id: i64, date: &str, key: &str, mins: f64, km: f64) -> CachedActivity {
        CachedActivity {
            activity_id: id,
            name: Some(format!("Session {id}")),
            type_key: Some(key.into()),
            start_time_local: Some(format!("{date} 09:00:00")),
            local_date: Some(date.into()),
            distance_m: (km > 0.0).then_some(km * 1000.0),
            duration_s: Some(mins * 60.0),
            moving_duration_s: None,
            avg_hr: None,
            max_hr: None,
            avg_cadence: None,
            calories: None,
            elevation_gain: None,
            steps: None,
            aerobic_te: None,
            anaerobic_te: None,
            zone_secs: [0.0; 5],
        }
    }

    #[test]
    fn weeks_start_on_monday_and_skip_empty_ones() {
        // 2026-03-09 is a Monday. 2026-03-01 is a Sunday, so it is the week before.
        let acts = [
            act(1, "2026-03-11", "golf", 60.0, 0.0),
            act(2, "2026-03-09", "golf", 30.0, 0.0),
            act(3, "2026-03-01", "golf", 90.0, 0.0),
        ];
        let weeks = by_week(&acts);
        assert_eq!(weeks.len(), 2);
        assert_eq!(date_key(weeks[0].start), "2026-03-09");
        assert_eq!(weeks[0].activities.len(), 2);
        assert_eq!(date_key(weeks[1].start), "2026-02-23");
    }

    #[test]
    fn a_week_without_distance_charts_time_and_hides_distance() {
        let acts = [act(1, "2026-03-10", "strength_training", 45.0, 0.0)];
        let weeks = by_week(&acts);
        let (title, bars) = daily(&weeks[0]);
        assert_eq!(title, "Daily active time");
        assert_eq!(bars[1].text, "45 min");
        assert!(figures(&weeks[0], &[])
            .iter()
            .all(|f| f.label != "Distance"));
        assert!(figures(&weeks[0], &[])
            .iter()
            .all(|f| f.label != "Time above Z2"));
    }

    #[test]
    fn run_notes_need_three_runs_with_heart_rate_data() {
        let mut runs: Vec<CachedActivity> = (1..=3)
            .map(|i| {
                let mut a = act(i, "2026-03-10", "running", 30.0, 5.0);
                a.zone_secs = [900.0, 900.0, 0.0, 0.0, 0.0];
                a
            })
            .collect();
        assert!(hard_share(&by_week(&runs[..1])[0]).is_none());
        assert!(hard_share(&by_week(&runs[..2])[0]).is_none());
        assert!(hard_share(&by_week(&runs)[0]).is_some());
        runs.truncate(2);
        let week = &by_week(&runs)[0];
        assert_eq!(headline(week, &Compare::Gap), "A steady week");
    }

    #[test]
    fn the_body_says_one_session_in_the_singular() {
        let acts = [act(1, "2026-03-10", "golf", 60.0, 0.0)];
        let weeks = by_week(&acts);
        let text = body(&weeks[0], &Compare::First);
        assert!(
            text.starts_with("1 session, 1h 00m of recorded time."),
            "{text}"
        );
    }

    #[test]
    fn the_week_before_is_the_calendar_week() {
        let acts = [
            act(1, "2026-03-03", "golf", 60.0, 0.0),
            act(2, "2026-02-10", "golf", 60.0, 0.0),
        ];
        let weeks = by_week(&acts);
        let day = |d: &str| NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap();
        let start = day("2025-11-01");
        let today = day("2026-03-20");
        let gap = compare_for(&weeks[0], weeks.get(1), today, start);
        assert!(matches!(gap, Compare::Gap));
        let first = compare_for(&weeks[1], None, today, start);
        assert!(matches!(first, Compare::First));
        let partial = compare_for(&weeks[0], weeks.get(1), day("2026-03-05"), start);
        assert!(matches!(partial, Compare::Partial));
    }
}
