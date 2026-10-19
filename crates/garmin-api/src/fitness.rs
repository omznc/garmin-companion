//! The Fitness screen as one view model.
//!
//! Everything here is Garmin's own arithmetic: the training status, the load
//! balance, the race predictions and the records. The core adds the words. A
//! number about running shows only when the user runs now. The load and the
//! step records suit every sport.

use crate::body::EmptyNote;
use crate::derive::date_key;
use crate::insights::{runs_now, Figure};
use crate::text::{num, short_date};
use crate::ApiError;
use chrono::NaiveDate;
use garmin_core::records::RecordUnit;
use garmin_core::sport::is_exercise;
use garmin_core::{query, CachedActivity, Db, PersonalRecord, TrainingStatus};
use serde::{Deserialize, Serialize};

const FITNESS_DAYS: u32 = 90;
/// Garmin record ids for runs, and for rides. See `garmin_core::records`.
const RUN_RECORDS: std::ops::RangeInclusive<i64> = 1..=7;
const RIDE_RECORDS: std::ops::RangeInclusive<i64> = 8..=9;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadBar {
    pub label: String,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    /// `over`, `under` or `in`, against the range that Garmin wants.
    pub state: String,
    /// For example `473 of 200 to 400`.
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Balance {
    pub intro: String,
    pub bars: Vec<LoadBar>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Predictions {
    pub intro: String,
    pub rows: Vec<Figure>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordRow {
    pub label: String,
    pub value: String,
    /// The short date, or empty when the cache has none.
    pub date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FitnessView {
    pub date: String,
    pub empty: Option<EmptyNote>,
    pub lede: Option<String>,
    pub figures: Vec<Figure>,
    pub notes: Vec<String>,
    pub balance: Option<Balance>,
    pub predictions: Option<Predictions>,
    pub records: Vec<RecordRow>,
}

/// `ANAEROBIC_FOCUS` is `anaerobic focus`. A trailing number goes.
fn phrase(key: &str) -> String {
    let words = key.to_lowercase().replace('_', " ");
    let trimmed = words.trim_end_matches(|c: char| c.is_ascii_digit() || c == ' ');
    trimmed.to_string()
}

fn hms(secs: f64) -> String {
    let total = secs.round() as i64;
    let (h, m, s) = (total / 3600, total % 3600 / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn record_value(r: &PersonalRecord) -> String {
    match r.unit {
        Some(RecordUnit::Seconds) => hms(r.value),
        Some(RecordUnit::Metres) => format!("{:.1} km", r.value / 1000.0),
        Some(RecordUnit::Days) => format!("{} days", r.value.round() as i64),
        _ => num(r.value),
    }
}

fn rides_now(activities: &[CachedActivity]) -> bool {
    activities.iter().any(|a| {
        let k = a.type_key.as_deref().unwrap_or("");
        ["cycling", "biking", "ride"].iter().any(|p| k.contains(p))
    })
}

fn shown_record(r: &PersonalRecord, runs: bool, rides: bool) -> bool {
    r.label.is_some()
        && (!RUN_RECORDS.contains(&r.type_id) || runs)
        && (!RIDE_RECORDS.contains(&r.type_id) || rides)
}

fn load_bar(
    label: &str,
    value: Option<f64>,
    min: Option<f64>,
    max: Option<f64>,
) -> Option<LoadBar> {
    let (value, min, max) = (value?, min?, max?);
    let state = if value > max {
        "over"
    } else if value < min {
        "under"
    } else {
        "in"
    };
    Some(LoadBar {
        label: label.into(),
        value,
        min,
        max,
        state: state.into(),
        text: format!("{} of {} to {}", num(value), num(min), num(max)),
    })
}

fn balance(s: &TrainingStatus) -> Option<Balance> {
    let bars: Vec<LoadBar> = [
        load_bar(
            "Low aerobic: easy work",
            s.aerobic_low,
            s.aerobic_low_target_min,
            s.aerobic_low_target_max,
        ),
        load_bar(
            "High aerobic: tempo and threshold",
            s.aerobic_high,
            s.aerobic_high_target_min,
            s.aerobic_high_target_max,
        ),
        load_bar(
            "Anaerobic: hard and short",
            s.anaerobic,
            s.anaerobic_target_min,
            s.anaerobic_target_max,
        ),
    ]
    .into_iter()
    .flatten()
    .collect();
    if bars.is_empty() {
        return None;
    }
    let verdict = s
        .balance_phrase
        .as_deref()
        .map(|p| format!(" Its verdict: {}.", phrase(p)))
        .unwrap_or_default();
    Some(Balance {
        intro: format!("Garmin splits the work of the last month into three kinds. Each has a range that Garmin wants.{verdict}"),
        bars,
    })
}

fn figure(label: &str, value: Option<String>) -> Option<Figure> {
    value.map(|value| Figure {
        label: label.into(),
        value,
        accent: false,
    })
}

fn figures(s: &TrainingStatus, runs: bool) -> Vec<Figure> {
    let ratio_label = match &s.acwr_status {
        Some(p) => format!("Acute : chronic ({})", phrase(p)),
        None => "Acute : chronic".into(),
    };
    [
        figure("Acute load", s.acute_load.map(num)),
        figure("Chronic load", s.chronic_load.map(num)),
        figure(&ratio_label, s.acwr.map(|v| format!("{v:.2}"))),
        // VO2 max comes from outdoor runs, so a user who does not run has none.
        figure("VO2 max", s.vo2max.filter(|_| runs).map(num)),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn predictions(p: &garmin_core::RacePredictions) -> Option<Predictions> {
    let rows: Vec<Figure> = [
        ("5K", p.time_5k_s),
        ("10K", p.time_10k_s),
        ("Half", p.time_half_s),
        ("Marathon", p.time_marathon_s),
    ]
    .into_iter()
    .filter_map(|(label, secs)| figure(label, secs.filter(|s| *s > 0.0).map(hms)))
    .collect();
    (!rows.is_empty()).then(|| Predictions {
        intro: "Garmin estimates these from your heart rate and pace. Read them as a direction, not as a time to plan around.".into(),
        rows,
    })
}

fn empty_note() -> EmptyNote {
    EmptyNote {
        title: "Nothing synced yet.".into(),
        body: "Training status, load balance and records arrive with a sync. Garmin works them out on the watch, so a watch that records activity is needed.".into(),
    }
}

/// Build the Fitness view for `today`.
pub fn fitness_view(db: &Db, today: NaiveDate) -> Result<FitnessView, ApiError> {
    // Not `query::fitness`: it counts back from the real date, not from `today`.
    let days = db.fitness_since(&date_key(
        today - chrono::Duration::days(FITNESS_DAYS as i64 - 1),
    ))?;
    let from = date_key(today - chrono::Duration::days(55));
    let recent: Vec<CachedActivity> = db
        .activities_since(&from)?
        .into_iter()
        .filter(|a| is_exercise(a.type_key.as_deref()))
        .collect();
    let (runs, rides) = (runs_now(&recent, today), rides_now(&recent));

    let records: Vec<RecordRow> = query::personal_records(db)?
        .iter()
        .filter(|r| shown_record(r, runs, rides))
        .map(|r| RecordRow {
            label: r.label.clone().unwrap_or_default(),
            value: record_value(r),
            date: r.set_on.as_deref().and_then(short_date).unwrap_or_default(),
        })
        .collect();

    let latest = days.first();
    let status = latest.map(|d| &d.status);
    let mut notes = Vec::new();
    if runs && status.is_some_and(|s| s.vo2max.is_none()) {
        notes.push("VO2 max is blank. Garmin works it out only from outdoor GPS runs. One easy outdoor run starts it.".to_string());
    }
    Ok(FitnessView {
        date: date_key(today),
        empty: (latest.is_none() && records.is_empty()).then(empty_note),
        lede: status
            .and_then(|s| s.status_phrase.as_deref())
            .map(|p| format!("Garmin rates your training status as {}.", phrase(p))),
        figures: status.map_or_else(Vec::new, |s| figures(s, runs)),
        notes,
        balance: status.and_then(balance),
        predictions: latest
            .filter(|_| runs)
            .and_then(|d| predictions(&d.predictions)),
        records,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garmin_phrases_become_words() {
        assert_eq!(phrase("ANAEROBIC_FOCUS"), "anaerobic focus");
        assert_eq!(phrase("PRODUCTIVE_1"), "productive");
    }

    #[test]
    fn times_read_as_clock_values() {
        assert_eq!(hms(1_475.0), "24:35");
        assert_eq!(hms(3_725.0), "1:02:05");
    }

    #[test]
    fn a_load_bar_says_where_the_value_sits() {
        let over = load_bar("x", Some(473.0), Some(200.0), Some(400.0)).unwrap();
        assert_eq!(
            (over.state.as_str(), over.text.as_str()),
            ("over", "473 of 200 to 400")
        );
        assert_eq!(
            load_bar("x", Some(100.0), Some(200.0), Some(400.0))
                .unwrap()
                .state,
            "under"
        );
        assert_eq!(
            load_bar("x", Some(300.0), Some(200.0), Some(400.0))
                .unwrap()
                .state,
            "in"
        );
        assert!(load_bar("x", Some(1.0), None, Some(2.0)).is_none());
    }

    #[test]
    fn run_records_need_a_runner() {
        let r = |type_id| PersonalRecord {
            record_id: 1,
            type_id,
            label: Some("x".into()),
            unit: None,
            value: 1.0,
            activity_id: None,
            activity_name: None,
            activity_type: None,
            set_on: None,
        };
        assert!(!shown_record(&r(3), false, false));
        assert!(shown_record(&r(3), true, false));
        assert!(shown_record(&r(12), false, false));
        assert!(!shown_record(&r(8), true, false));
    }
}
