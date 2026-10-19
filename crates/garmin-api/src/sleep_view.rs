//! The Sleep screen as one view model.
//!
//! It needs no activity. Last night, the stage split and the 7 and 30 night
//! windows come from the sleep nights. A night that has only the daily sleep
//! time and score (a cache from before nights were kept) still draws, without
//! stages. The insights come from `garmin_core::sleep`.

use crate::derive::{daily_series, date_key};
use crate::text::{hours_minutes, short_date};
use crate::ApiError;
use chrono::{Datelike, NaiveDate};
use garmin_core::sleep::{insights, SleepInsight, SleepNight};
use garmin_core::sport::is_exercise;
use garmin_core::{DailyMetrics, Db};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const MONTH: u32 = 30;
const WEEK: u32 = 7;
/// The shortest night that is not a short night.
const SHORT_NIGHT_SECS: f64 = 7.0 * 3600.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageShare {
    /// `deep`, `light`, `rem` or `awake`.
    pub key: String,
    pub label: String,
    pub secs: f64,
    /// Share of the four stages together. The bar fills to 100.
    pub pct: f64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NightView {
    pub date: String,
    /// `Last night`, or `Woke 03 Mar` for an older night.
    pub title: String,
    pub score: Option<f64>,
    /// Garmin's word for the score, in lower case: `good`.
    pub qualifier: Option<String>,
    pub duration_secs: f64,
    pub duration_text: String,
    /// Clock time that sleep began and ended, `HH:MM`.
    pub start: Option<String>,
    pub end: Option<String>,
    /// Empty when the cache holds no stage detail for the night.
    pub stages: Vec<StageShare>,
    pub sentence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NightBar {
    pub date: String,
    /// Weekday, `Mon`.
    pub label: String,
    pub hours: Option<f64>,
    pub score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SleepWindow {
    pub days: u32,
    /// One bar for each day, oldest first. A night with no data is a gap.
    pub bars: Vec<NightBar>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SleepView {
    pub date: String,
    /// True when the last 30 days hold no sleep.
    pub empty: bool,
    pub lede: String,
    /// Said under the night when its stage detail is not cached yet.
    pub note: Option<String>,
    pub last_night: Option<NightView>,
    pub week: SleepWindow,
    pub month: SleepWindow,
    pub insights: Vec<SleepInsight>,
}

/// The figures of one night, from the detailed night or the daily row.
struct Basic<'a> {
    date: &'a str,
    secs: f64,
    score: Option<f64>,
    night: Option<&'a SleepNight>,
}

fn clock(local: &Option<String>) -> Option<String> {
    local.as_deref()?.get(11..16).map(str::to_string)
}

fn stages(n: &SleepNight) -> Vec<StageShare> {
    let parts = [
        ("deep", "Deep", n.deep_secs),
        ("light", "Light", n.light_secs),
        ("rem", "REM", n.rem_secs),
        ("awake", "Awake", n.awake_secs),
    ];
    let total: f64 = parts.iter().filter_map(|p| p.2).sum();
    if total <= 0.0 {
        return Vec::new();
    }
    parts
        .into_iter()
        .filter_map(|(key, label, secs)| {
            let secs = secs.filter(|s| *s > 0.0)?;
            let pct = secs / total * 100.0;
            Some(StageShare {
                key: key.into(),
                label: label.into(),
                secs,
                pct,
                text: format!("{} · {pct:.0}%", hours_minutes(secs)),
            })
        })
        .collect()
}

fn night_sentence(
    duration: &str,
    start: &Option<String>,
    end: &Option<String>,
    n: Option<&SleepNight>,
    score: Option<f64>,
) -> String {
    let span = match (start, end) {
        (Some(s), Some(e)) => format!(", from {s} to {e}"),
        _ => String::new(),
    };
    let mut out = format!("Slept {duration}{span}.");
    if let Some(score) = score {
        let word = n
            .and_then(|n| n.score_qualifier.as_deref())
            .map(|q| format!(", {}", q.to_lowercase().replace('_', " ")))
            .unwrap_or_default();
        out.push_str(&format!(" Garmin scored it {score:.0}{word}."));
    }
    out
}

fn night_view(b: &Basic, today: NaiveDate) -> NightView {
    let duration = hours_minutes(b.secs);
    let (start, end) = b.night.map_or((None, None), |n| {
        (clock(&n.start_local), clock(&n.end_local))
    });
    let age = NaiveDate::parse_from_str(b.date, "%Y-%m-%d")
        .map(|d| (today - d).num_days())
        .unwrap_or(0);
    NightView {
        date: b.date.into(),
        title: if age <= 1 {
            "Last night".into()
        } else {
            format!(
                "Woke {}",
                short_date(b.date).unwrap_or_else(|| b.date.into())
            )
        },
        score: b.score,
        qualifier: b
            .night
            .and_then(|n| n.score_qualifier.as_deref())
            .map(|q| q.to_lowercase().replace('_', " ")),
        sentence: night_sentence(&duration, &start, &end, b.night, b.score),
        duration_secs: b.secs,
        duration_text: duration,
        stages: b.night.map(stages).unwrap_or_default(),
        start,
        end,
    }
}

fn window_summary(bars: &[NightBar], days: u32) -> String {
    let hours: Vec<f64> = bars.iter().filter_map(|b| b.hours).collect();
    if hours.is_empty() {
        return format!("No sleep recorded in the last {days} nights.");
    }
    let n = hours.len();
    let avg = hours.iter().sum::<f64>() / n as f64;
    let mut out = format!(
        "Averaged {} a night over {n} of the last {days} nights.",
        hours_minutes(avg * 3600.0)
    );
    let scores: Vec<f64> = bars.iter().filter_map(|b| b.score).collect();
    if !scores.is_empty() {
        let mean = scores.iter().sum::<f64>() / scores.len() as f64;
        out.push_str(&format!(" The average score was {mean:.0}."));
    }
    let short = hours
        .iter()
        .filter(|h| **h * 3600.0 < SHORT_NIGHT_SECS)
        .count();
    if short > 0 {
        out.push_str(&format!(
            " {short} {} under 7 hours.",
            if short == 1 {
                "night was"
            } else {
                "nights were"
            }
        ));
    }
    out
}

fn window(by_date: &HashMap<&str, Basic>, days: u32, end: NaiveDate) -> SleepWindow {
    let bars: Vec<NightBar> = (0..days)
        .rev()
        .map(|age| {
            let day = end - chrono::Duration::days(age as i64);
            let found = by_date.get(date_key(day).as_str());
            NightBar {
                date: date_key(day),
                label: day.weekday().to_string(),
                hours: found.map(|b| b.secs / 3600.0),
                score: found.and_then(|b| b.score),
            }
        })
        .collect();
    SleepWindow {
        days,
        summary: window_summary(&bars, days),
        bars,
    }
}

/// A night, or a daily row with a sleep time, keyed by the day of waking. A
/// detailed night wins over the daily row of the same day.
fn merge<'a>(nights: &'a [SleepNight], daily: &'a [DailyMetrics]) -> HashMap<&'a str, Basic<'a>> {
    let mut map: HashMap<&str, Basic> = daily
        .iter()
        .filter_map(|d| {
            let secs = d.sleep_secs.filter(|s| *s > 0.0)?;
            Some((
                d.date.as_str(),
                Basic {
                    date: &d.date,
                    secs,
                    score: d.sleep_score,
                    night: None,
                },
            ))
        })
        .collect();
    for n in nights {
        if let Some(secs) = n.total_secs.filter(|s| *s > 0.0) {
            map.insert(
                &n.date,
                Basic {
                    date: &n.date,
                    secs,
                    score: n.score,
                    night: Some(n),
                },
            );
        }
    }
    map
}

/// Build the Sleep view for `today`.
pub fn sleep_view(db: &Db, today: NaiveDate) -> Result<SleepView, ApiError> {
    // One day more than the chart: before the morning sync the window ends yesterday.
    let from = date_key(today - chrono::Duration::days(MONTH as i64));
    let nights = db.sleep_nights_since(&from)?;
    let daily = daily_series(db.daily_since(&from)?, MONTH + 1, today);
    // A motorcycle ride is not an evening session or a hard day.
    let exercise: Vec<_> = db
        .activities_since(&from)?
        .into_iter()
        .filter(|a| is_exercise(a.type_key.as_deref()))
        .collect();
    let by_date = merge(&nights, &daily);
    let last = by_date.values().max_by_key(|b| b.date);
    let last_night = last.map(|b| night_view(b, today));
    let missing_detail = last.is_some_and(|b| b.night.is_none());
    let empty = last.is_none();
    // Before the watch syncs this morning, the newest night is last night's
    // wake date. The windows end there, so they hold no empty slot for today.
    let end = last
        .and_then(|b| NaiveDate::parse_from_str(b.date, "%Y-%m-%d").ok())
        .filter(|d| (today - *d).num_days() == 1)
        .unwrap_or(today);
    Ok(SleepView {
        date: date_key(today),
        empty,
        lede: last_night.as_ref().map_or_else(
            || "No sleep is recorded yet. Wear the watch to bed, then sync to see your nights here.".into(),
            |n| n.sentence.clone(),
        ),
        note: missing_detail
            .then(|| "The stage detail for this night arrives with the next sync.".to_string()),
        last_night,
        week: window(&by_date, WEEK, end),
        month: window(&by_date, MONTH, end),
        insights: insights(&nights, &daily, &exercise),
    })
}

/// Seed made-up nights for a persona, to match what `personas::seed` writes
/// for the daily rows. A golfer's watch has no SpO2 or respiration sensor
/// data, so those nights carry none.
pub fn seed_sleep(
    db: &Db,
    persona: crate::personas::Persona,
    today: NaiveDate,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::personas::Persona;
    let days = if persona == Persona::NewUser { 3 } else { 40 };
    let sensors = persona != Persona::Golfer;
    for age in 0..days {
        let wobble = (age * 7 % 5) as f64;
        let date = today - chrono::Duration::days(age);
        let total = 25_200.0 + wobble * 900.0;
        let bed = 22 * 60 + 15 + age * 13 % 50;
        let wake_total = bed + ((total + 1_800.0) / 60.0) as i64;
        let local = |day: NaiveDate, mins: i64| {
            format!(
                "{}T{:02}:{:02}:00",
                date_key(day),
                (mins / 60) % 24,
                mins % 60
            )
        };
        db.upsert_sleep_night(&SleepNight {
            date: date_key(date),
            score: Some(70.0 + wobble * 3.0),
            score_qualifier: Some(if wobble > 2.0 { "GOOD" } else { "FAIR" }.into()),
            total_secs: Some(total),
            deep_secs: Some(total * 0.18),
            rem_secs: Some(total * 0.22),
            light_secs: Some(total * 0.60),
            awake_secs: Some(1_800.0 + wobble * 120.0),
            start_local: Some(local(date - chrono::Duration::days(1), bed)),
            end_local: Some(local(date, wake_total)),
            avg_spo2: sensors.then_some(95.0 + wobble / 2.0),
            avg_respiration: sensors.then_some(14.0 + wobble / 4.0),
            ..SleepNight::default()
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::personas::{sample_day, seed, Persona};

    fn db(tag: &str) -> Db {
        let dir =
            std::env::temp_dir().join(format!("garmin-api-sleep-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Db::open(&dir.join("cache.sqlite3")).unwrap()
    }

    #[test]
    fn empty_cache_says_so() {
        let v = sleep_view(&db("empty"), sample_day()).unwrap();
        assert!(v.empty && v.last_night.is_none() && v.insights.is_empty());
        assert!(v.lede.starts_with("No sleep is recorded yet"));
        assert_eq!(v.week.bars.len(), 7);
        assert!(v.week.bars.iter().all(|b| b.hours.is_none()));
        assert_eq!(v.week.summary, "No sleep recorded in the last 7 nights.");
    }

    #[test]
    fn the_windows_end_on_last_night_before_the_morning_sync() {
        let db = db("before-sync");
        let day = sample_day();
        seed(&db, Persona::HealthOnly, day).unwrap();
        seed_sleep(&db, Persona::HealthOnly, day).unwrap();
        // The next calendar day: the watch has not synced a night for it yet.
        let v = sleep_view(&db, day + chrono::Duration::days(1)).unwrap();
        let last = v.week.bars.last().unwrap();
        assert_eq!(last.date, date_key(day));
        assert!(v.week.bars.iter().all(|b| b.hours.is_some()));
        assert_eq!(v.month.bars.last().unwrap().date, date_key(day));
        // The first bar of the 30 nights has data too, not an empty slot.
        assert!(v.month.bars.iter().all(|b| b.hours.is_some()));
    }

    #[test]
    fn a_motorcycle_ride_is_not_an_evening_session() {
        let db = db("moto");
        let day = sample_day();
        seed(&db, Persona::HealthOnly, day).unwrap();
        seed_sleep(&db, Persona::HealthOnly, day).unwrap();
        for age in (0..20).step_by(2) {
            let date = date_key(day - chrono::Duration::days(age));
            let ride = serde_json::json!({
                "activityId": 9000 + age,
                "startTimeLocal": format!("{date} 20:30:00"),
                "duration": 3600.0,
                "activityType": { "typeKey": "motorcycling_v2" },
                "hrTimeInZone_1": 0.0,
                "hrTimeInZone_2": 0.0,
                "hrTimeInZone_3": 0.0,
                "hrTimeInZone_4": 600.0,
                "hrTimeInZone_5": 600.0,
            });
            db.upsert_activity(&serde_json::from_value(ride).unwrap())
                .unwrap();
        }
        let v = sleep_view(&db, day).unwrap();
        assert!(v
            .insights
            .iter()
            .all(|i| i.id != "late-session" && i.id != "hard-day"));
    }

    #[test]
    fn a_sleep_only_user_gets_a_full_screen() {
        let db = db("sleep-only");
        let day = sample_day();
        seed(&db, Persona::HealthOnly, day).unwrap();
        seed_sleep(&db, Persona::HealthOnly, day).unwrap();
        let v = sleep_view(&db, day).unwrap();
        let night = v.last_night.unwrap();
        assert_eq!(night.title, "Last night");
        assert_eq!(night.stages.len(), 4);
        let pct: f64 = night.stages.iter().map(|s| s.pct).sum();
        assert!((pct - 100.0).abs() < 0.01);
        assert!(night.start.is_some() && night.end.is_some());
        assert!(night.sentence.starts_with("Slept 7h"));
        assert!(v.note.is_none());
        assert_eq!(v.month.bars.len(), 30);
        assert!(v.week.summary.starts_with("Averaged"));
    }

    #[test]
    fn a_cache_without_nights_falls_back_to_the_daily_row() {
        let db = db("backfill");
        let day = sample_day();
        seed(&db, Persona::NewUser, day).unwrap();
        let v = sleep_view(&db, day).unwrap();
        let night = v.last_night.unwrap();
        assert!(night.stages.is_empty() && night.start.is_none());
        assert!(night.sentence.starts_with("Slept "));
        assert!(v.note.unwrap().contains("next sync"));
        assert!(!v.empty);
    }

    #[test]
    fn an_old_night_is_named_by_its_date() {
        let db = db("old");
        let day = sample_day();
        db.upsert_daily(&DailyMetrics {
            date: "2026-03-05".into(),
            sleep_secs: Some(21_600.0),
            ..DailyMetrics::default()
        })
        .unwrap();
        let v = sleep_view(&db, day).unwrap();
        let night = v.last_night.unwrap();
        assert_eq!(night.title, "Woke 05 Mar");
        assert_eq!(night.sentence, "Slept 6h 00m.");
        assert!(v.week.summary.contains("1 night was under 7 hours"));
    }

    /// The committed Health and Sleep files must match what the code writes now.
    #[test]
    fn committed_fixtures_are_current() {
        for p in Persona::ALL {
            let db = db(p.name());
            let day = sample_day();
            seed(&db, p, day).unwrap();
            seed_sleep(&db, p, day).unwrap();
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
            let pairs = [
                (
                    "health",
                    serde_json::to_string_pretty(&crate::health_view(&db, day).unwrap()),
                ),
                (
                    "sleep",
                    serde_json::to_string_pretty(&sleep_view(&db, day).unwrap()),
                ),
            ];
            for (kind, want) in pairs {
                let path = dir.join(format!("{kind}.{}.json", p.name()));
                let have = std::fs::read_to_string(&path)
                    .unwrap_or_else(|_| panic!("missing {}", path.display()));
                assert_eq!(
                    have,
                    want.unwrap() + "\n",
                    "stale {kind} for {}: run the fixture bin with --personas",
                    p.name()
                );
            }
        }
    }
}
