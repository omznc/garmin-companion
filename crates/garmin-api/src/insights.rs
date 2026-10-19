//! The Insights screen as one view model.
//!
//! The deep findings come from `garmin_core::findings`. This module adds the
//! training load and the risk flags, and it decides what each user sees. A
//! finding about running shows only when the profile shows runs. Findings
//! about sleep, recovery and the shape of the week need no sport.

use crate::body::EmptyNote;
use crate::derive::{
    daily_series, date_key, days_since, easy_hard_split, is_run, MIN_RUNS_FOR_NOTES,
};
use crate::text::{num, short_date};
use crate::{activity_profile, ApiError};
use chrono::{Datelike, Duration, NaiveDate};
use garmin_core::findings::{self, edwards_trimp, Finding, Section, Tone};
use garmin_core::sport::is_exercise;
use garmin_core::{CachedActivity, Db, Family};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const YEAR: i64 = 365;
/// The profile window. A run older than this does not count as running now.
const RUN_WINDOW_DAYS: i64 = 56;
/// The findings that read runs only.
const RUN_ONLY: [&str; 4] = [
    "fitness-at-fixed-hr",
    "cadence-lever",
    "easy-share-trend",
    "block-ended",
];

/// A label with a figure. The load, the report and the fitness screens use it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Figure {
    pub label: String,
    pub value: String,
    pub accent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightSeries {
    pub name: String,
    /// A gap in the data is `None`.
    pub values: Vec<Option<f64>>,
    /// Drawn as the comparison line.
    pub muted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightRow {
    pub label: String,
    pub value: String,
    pub note: Option<String>,
    pub accent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightItem {
    pub id: String,
    pub tone: Tone,
    /// The statement. One sentence, set large.
    pub claim: String,
    pub detail: String,
    /// What was counted. A finding always has it.
    pub basis: Option<String>,
    pub figures: Vec<Figure>,
    pub series: Vec<InsightSeries>,
    /// The first and the last label under the chart. Empty for no labels.
    pub ends: Vec<String>,
    pub rows: Vec<InsightRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightSection {
    pub key: String,
    pub title: String,
    pub items: Vec<InsightItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightsView {
    pub date: String,
    pub eyebrow: String,
    pub lede: String,
    pub empty: Option<EmptyNote>,
    pub sections: Vec<InsightSection>,
}

/// True when the user runs now. Run-only copy needs this.
pub fn runs_now(activities: &[CachedActivity], today: NaiveDate) -> bool {
    let profile = activity_profile(activities, 0, today);
    let recent_runs = activities
        .iter()
        .filter(|a| {
            is_run(&a.type_key)
                && days_since(a, today).is_some_and(|d| (0..RUN_WINDOW_DAYS).contains(&d))
        })
        .count();
    // The audience plan: gate every run note on 3 or more runs in the window.
    profile.active.contains(&Family::Endurance) && recent_runs >= MIN_RUNS_FOR_NOTES
}

fn ends(labels: &[String]) -> Vec<String> {
    match (labels.first(), labels.last()) {
        (Some(a), Some(b)) if labels.len() > 1 => vec![a.clone(), b.clone()],
        _ => Vec::new(),
    }
}

fn finding_item(f: &Finding) -> InsightItem {
    InsightItem {
        id: f.kind.clone(),
        tone: f.tone,
        claim: f.claim.clone(),
        detail: f.detail.clone(),
        basis: Some(f.basis.clone()),
        figures: Vec::new(),
        series: f
            .series
            .iter()
            .map(|s| InsightSeries {
                name: s.name.clone(),
                values: s.values.clone(),
                muted: s.muted,
            })
            .collect(),
        ends: ends(&f.labels),
        rows: f
            .rows
            .iter()
            .map(|r| InsightRow {
                label: r.label.clone(),
                value: r.value.clone(),
                note: r.note.clone(),
                accent: r.accent,
            })
            .collect(),
    }
}

fn section_title(s: Section) -> (&'static str, &'static str) {
    match s {
        Section::Fitness => ("fitness", "Fitness"),
        Section::Recovery => ("recovery", "Recovery"),
        Section::Patterns => ("patterns", "Patterns"),
    }
}

/* ------------------------------------------------------------------- load --- */

fn date_of(a: &CachedActivity) -> Option<NaiveDate> {
    let text = a
        .local_date
        .as_deref()
        .or(a.start_time_local.as_deref())?
        .get(..10)?;
    NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()
}

/// Hours of training on the `days` days that end today.
fn hours_in(activities: &[CachedActivity], today: NaiveDate, days: i64) -> f64 {
    let from = today - Duration::days(days - 1);
    activities
        .iter()
        .filter(|a| date_of(a).is_some_and(|d| (from..=today).contains(&d)))
        .fold(0.0, |t, a| t + a.duration_s.unwrap_or(0.0))
        / 3600.0
}

#[derive(Debug, Clone, PartialEq)]
pub struct Load {
    pub acute: f64,
    pub chronic: f64,
    pub ratio: f64,
}

/// Days of history that the 28-day average needs. A new user has less, so a
/// first week would look like a spike against a month that never happened.
const LOAD_HISTORY_DAYS: i64 = 28;

/// The last 7 days against the weekly average of the last 28. `None` until
/// the oldest synced day is 28 days old.
pub fn load(
    activities: &[CachedActivity],
    oldest_day: Option<NaiveDate>,
    today: NaiveDate,
) -> Option<Load> {
    if oldest_day.is_none_or(|d| (today - d).num_days() < LOAD_HISTORY_DAYS) {
        return None;
    }
    let acute = hours_in(activities, today, 7);
    let chronic = hours_in(activities, today, 28) / 4.0;
    (chronic > 0.0).then(|| Load {
        acute,
        chronic,
        ratio: acute / chronic,
    })
}

fn load_status(ratio: f64) -> &'static str {
    if ratio > 1.5 {
        "spiking"
    } else if ratio > 1.15 {
        "building"
    } else if ratio > 0.85 {
        "steady"
    } else {
        "easing"
    }
}

fn ratio_sentence(ratio: f64) -> &'static str {
    match load_status(ratio) {
        "spiking" => "That is a sharp step up. In this range the risk of injury climbs.",
        "building" => "That is a step up from your recent norm.",
        "easing" => "You train less than your recent norm, as in a down week.",
        _ => "That is a steady ratio.",
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WeekLoad {
    pub start: NaiveDate,
    pub trimp: f64,
    pub monotony: Option<f64>,
}

fn monday(d: NaiveDate) -> NaiveDate {
    d - Duration::days(d.weekday().num_days_from_monday() as i64)
}

fn monotony(days: &[f64; 7]) -> Option<f64> {
    let mean = days.iter().sum::<f64>() / 7.0;
    let sd = (days.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / 7.0).sqrt();
    (sd > 0.0).then(|| mean / sd)
}

/// The weekly training impulse of the last `weeks` weeks, oldest first.
/// Fewer than four weeks of data gives `None`.
pub fn load_shape(activities: &[CachedActivity], weeks: usize) -> Option<Vec<WeekLoad>> {
    let mut buckets: BTreeMap<NaiveDate, [f64; 7]> = BTreeMap::new();
    for a in activities {
        let Some(d) = date_of(a) else { continue };
        let days = buckets.entry(monday(d)).or_insert([0.0; 7]);
        days[d.weekday().num_days_from_monday() as usize] += edwards_trimp(a);
    }
    if buckets.len() < 4 {
        return None;
    }
    let first = *buckets.keys().next()?;
    let last = *buckets.keys().next_back()?;
    // A week with no session stays in the series as a zero, so a break shows.
    let all = (0..=(last - first).num_weeks()).map(|w| first + Duration::weeks(w));
    let mut out: Vec<WeekLoad> = all
        .map(|start| {
            let days = buckets.get(&start).copied().unwrap_or([0.0; 7]);
            WeekLoad {
                start,
                trimp: days.iter().fold(0.0, |t, v| t + v),
                monotony: monotony(&days),
            }
        })
        .collect();
    out.drain(..out.len().saturating_sub(weeks));
    Some(out)
}

fn monotony_sentence(shape: &[WeekLoad]) -> String {
    let latest = shape.last().and_then(|w| w.monotony);
    let high = shape
        .iter()
        .filter(|w| w.monotony.is_some_and(|m| m >= 2.0))
        .count();
    let mut out = match latest {
        None => "Monotony is undefined this week, because the load was the same on every day.".into(),
        Some(m) if m >= 2.0 => format!("At {m:.2}, the week had no real recovery day. Weeks like this grind more than they build."),
        Some(m) => format!("At {m:.2}, monotony is under the usual limit of 2.0. Your hard days and easy days differ."),
    };
    if high > 0 {
        out.push_str(&format!(
            " {high} of the last {} weeks passed 2.0.",
            shape.len()
        ));
    }
    out
}

fn load_item(l: &Load, shape: Option<&[WeekLoad]>) -> InsightItem {
    let status = load_status(l.ratio);
    let mut figures = vec![Figure {
        label: "Acute / chronic".into(),
        value: format!("{:.2}", l.ratio),
        accent: l.ratio > 1.5,
    }];
    let latest = shape.and_then(|s| s.last()).and_then(|w| w.monotony);
    if let Some(m) = latest {
        figures.push(Figure {
            label: "Monotony".into(),
            value: format!("{m:.2}"),
            accent: m >= 2.0,
        });
    }
    let mut basis = String::from(
        "Weekly load is Edwards' training impulse: the minutes in each heart-rate zone, weighted one to five. A session with no heart rate counts as easy.",
    );
    if let Some(s) = shape {
        basis.push(' ');
        basis.push_str(&monotony_sentence(s));
    }
    InsightItem {
        id: "load".into(),
        tone: if l.ratio > 1.5 { Tone::Watch } else { Tone::Note },
        claim: format!("Your training load is {status}."),
        detail: format!(
            "You trained {:.1} h in the last seven days. Your weekly average over 28 days is {:.1} h. {}",
            l.acute,
            l.chronic,
            ratio_sentence(l.ratio)
        ),
        basis: Some(basis),
        figures,
        series: shape.map_or_else(Vec::new, |s| {
            vec![InsightSeries {
                name: "Weekly load".into(),
                values: s.iter().map(|w| Some(w.trimp.round())).collect(),
                muted: false,
            }]
        }),
        ends: shape.map_or_else(Vec::new, |s| {
            let label = |w: &WeekLoad| short_date(&date_key(w.start)).unwrap_or_default();
            vec![label(&s[0]), label(&s[s.len() - 1])]
        }),
        rows: Vec::new(),
    }
}

/* ------------------------------------------------------------------ flags --- */

fn flag(id: &str, accent: bool, claim: &str, detail: String) -> InsightItem {
    InsightItem {
        id: id.into(),
        tone: if accent { Tone::Watch } else { Tone::Note },
        claim: claim.into(),
        detail,
        basis: None,
        figures: Vec::new(),
        series: Vec::new(),
        ends: Vec::new(),
        rows: Vec::new(),
    }
}

/// The notes about the last ten runs. Call it only when the user runs now.
fn run_flags(activities: &[CachedActivity]) -> Vec<InsightItem> {
    let recent: Vec<&CachedActivity> = activities
        .iter()
        .filter(|a| is_run(&a.type_key))
        .take(10)
        .collect();
    let mut out = Vec::new();
    if let Some(s) = easy_hard_split(&recent).filter(|s| s.hard_pct > 50.0) {
        out.push(flag(
            "hard-runs",
            true,
            "Most of your recent running was hard.",
            format!("Across your last {} runs with heart-rate data, {:.0}% of the time was above Z2. Short and hard runs are fine as a choice. They are not a choice when every run is like that.", s.counted, s.hard_pct),
        ));
    }
    let short = recent
        .iter()
        .filter(|a| a.duration_s.unwrap_or(0.0) < 900.0)
        .count();
    if recent.len() >= 4 && short + 1 >= recent.len() {
        out.push(flag(
            "short-runs",
            true,
            "Your recent runs were short.",
            format!("{short} of your last {} runs were under fifteen minutes. Duration builds the aerobic base. One longer easy run each week is the lever.", recent.len()),
        ));
    }
    let cadences: Vec<f64> = recent.iter().filter_map(|a| a.avg_cadence).collect();
    let avg = cadences.iter().sum::<f64>() / cadences.len().max(1) as f64;
    if cadences.len() >= 3 && avg < 160.0 {
        out.push(flag(
            "low-cadence",
            false,
            "Your cadence is on the low side.",
            format!("Average cadence over your recent runs is {} spm. A cadence nearer 170 means shorter and lighter steps.", num(avg)),
        ));
    }
    out
}

/* ------------------------------------------------------------------- view --- */

fn empty_note() -> EmptyNote {
    EmptyNote {
        title: "Not enough history yet.".into(),
        body: "Insights compare your daily health data with your activity. After a few more weeks of synced data, the first ones appear. Nothing shows until the data supports it.".into(),
    }
}

fn sections(
    found: Vec<Finding>,
    extra: Vec<InsightItem>,
    flags: Vec<InsightItem>,
) -> Vec<InsightSection> {
    let mut out: Vec<InsightSection> = Vec::new();
    for f in &found {
        let (key, title) = section_title(f.section);
        match out.iter_mut().find(|s| s.key == key) {
            Some(s) => s.items.push(finding_item(f)),
            None => out.push(InsightSection {
                key: key.into(),
                title: title.into(),
                items: vec![finding_item(f)],
            }),
        }
    }
    let tail = [
        ("load", "Training load", extra),
        ("watch", "Worth a look", flags),
    ];
    out.extend(
        tail.into_iter()
            .filter(|(_, _, items)| !items.is_empty())
            .map(|(key, title, items)| InsightSection {
                key: key.into(),
                title: title.into(),
                items,
            }),
    );
    out
}

/// Build the Insights view for `today`.
pub fn insights_view(db: &Db, today: NaiveDate) -> Result<InsightsView, ApiError> {
    let from = date_key(today - Duration::days(YEAR - 1));
    let daily = db.daily_since(&from)?;
    let oldest_day = daily
        .iter()
        .filter_map(|d| NaiveDate::parse_from_str(&d.date, "%Y-%m-%d").ok())
        .min();
    let exercise: Vec<CachedActivity> = db
        .activities_since(&from)?
        .into_iter()
        .filter(|a| is_exercise(a.type_key.as_deref()))
        .collect();
    let runs = runs_now(&exercise, today);

    // The daily rows go in oldest first, as the findings expect.
    let rows = daily_series(daily, YEAR as u32, today);
    let mut found = findings::all(&rows, &exercise, today);
    found.retain(|f| runs || !RUN_ONLY.contains(&f.kind.as_str()));

    let shape = load_shape(&exercise, 12);
    let extra: Vec<InsightItem> = load(&exercise, oldest_day, today)
        .map(|l| load_item(&l, shape.as_deref()))
        .into_iter()
        .collect();
    let flags = if runs {
        run_flags(&exercise)
    } else {
        Vec::new()
    };
    let sections = sections(found, extra, flags);
    Ok(InsightsView {
        date: date_key(today),
        eyebrow: "Last 365 days".into(),
        lede: "What a year of your own data says. Each insight shows the sample behind it, so you can judge how far to trust it. These are patterns, not causes.".into(),
        empty: sections.is_empty().then(empty_note),
        sections,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn act(date: &str, key: &str, mins: f64) -> CachedActivity {
        CachedActivity {
            activity_id: 1,
            name: None,
            type_key: Some(key.into()),
            start_time_local: None,
            local_date: Some(date.into()),
            distance_m: None,
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

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 3, 11).unwrap()
    }

    #[test]
    fn load_compares_a_week_with_the_monthly_average() {
        // 60 minutes in the last week, 240 minutes over 28 days.
        let acts = [
            act("2026-03-10", "golf", 60.0),
            act("2026-02-20", "golf", 180.0),
            act("2026-02-01", "golf", 30.0),
        ];
        let old = NaiveDate::from_ymd_opt(2026, 2, 1);
        let l = load(&acts, old, today()).unwrap();
        assert!((l.acute - 1.0).abs() < 1e-9 && (l.chronic - 1.0).abs() < 1e-9);
        assert_eq!(load_status(l.ratio), "steady");
        assert!(load(&[], old, today()).is_none());
    }

    #[test]
    fn a_new_user_gets_no_load_note() {
        // Two sessions in the first week: the ratio would read as a spike.
        let acts = [
            act("2026-03-10", "running", 60.0),
            act("2026-03-05", "running", 60.0),
        ];
        let first_week = NaiveDate::from_ymd_opt(2026, 3, 5);
        assert!(load(&acts, first_week, today()).is_none());
        assert!(load(&acts, None, today()).is_none());
        // A user with a month of health data and a first session is a real rise.
        let month = NaiveDate::from_ymd_opt(2026, 2, 5);
        assert!(load(&acts, month, today()).is_some());
    }

    #[test]
    fn monotony_is_undefined_only_for_a_flat_week() {
        assert_eq!(
            monotony(&[60.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]).map(|m| m > 0.0),
            Some(true)
        );
        assert!(monotony(&[0.0; 7]).is_none());
        assert!(monotony(&[10.0; 7]).is_none());
    }

    #[test]
    fn load_shape_needs_four_weeks() {
        let weekly: Vec<CachedActivity> = (0..3)
            .map(|w| act(&date_key(today() - Duration::days(w * 7)), "golf", 60.0))
            .collect();
        assert!(load_shape(&weekly, 12).is_none());
        let more: Vec<CachedActivity> = (0..5)
            .map(|w| act(&date_key(today() - Duration::days(w * 7)), "golf", 60.0))
            .collect();
        assert_eq!(load_shape(&more, 12).unwrap().len(), 5);
        let gap =
            [0, 7, 14, 21, 77].map(|d| act(&date_key(today() - Duration::days(d)), "golf", 60.0));
        assert_eq!(load_shape(&gap, 12).unwrap().len(), 12);
    }

    #[test]
    fn running_counts_only_for_a_user_who_runs_now() {
        let golf = [
            act("2026-03-10", "golf", 60.0),
            act("2026-03-02", "golf", 60.0),
        ];
        assert!(!runs_now(&golf, today()));
        let stopped = [
            act("2025-11-01", "running", 30.0),
            act("2025-11-03", "running", 30.0),
        ];
        assert!(!runs_now(&stopped, today()));
        let one_run = [act("2026-03-10", "running", 30.0)];
        assert!(!runs_now(&one_run, today()));
        let runner = [
            act("2026-03-10", "running", 30.0),
            act("2026-03-08", "running", 30.0),
            act("2026-03-06", "running", 30.0),
        ];
        assert!(runs_now(&runner, today()));
    }
}
