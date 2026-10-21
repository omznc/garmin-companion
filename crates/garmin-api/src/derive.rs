//! Numbers computed from cache rows, not read out of them.
//!
//! Port of the Today parts of `app/src/lib/derive.ts`. All of it is plain
//! arithmetic over real rows. Where the data cannot support a claim, a
//! function returns `None` and the UI shows an empty state.

use chrono::{Duration, NaiveDate};
use garmin_core::{days_between, CachedActivity, DailyMetrics};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Garmin's minimum sleep before resting heart rate is the overnight figure.
/// Mirrors `SLEEP_FOR_RHR_SECS` in `garmin-core::signal`.
const SLEEP_FOR_RHR_SECS: f64 = 4.0 * 3600.0;

pub fn date_key(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

/// Runs that a user needs before any run note shows. Every screen uses it.
pub const MIN_RUNS_FOR_NOTES: usize = 3;

pub fn is_run(type_key: &Option<String>) -> bool {
    type_key.as_deref().is_some_and(|k| k.contains("running"))
}

pub fn mean(values: impl IntoIterator<Item = Option<f64>>) -> Option<f64> {
    let present: Vec<f64> = values
        .into_iter()
        .flatten()
        .filter(|v| v.is_finite())
        .collect();
    if present.is_empty() {
        None
    } else {
        Some(present.iter().sum::<f64>() / present.len() as f64)
    }
}

/// An empty row for a day with no data. The chart needs one slot per day.
fn blank_day(date: String) -> DailyMetrics {
    DailyMetrics {
        date,
        ..DailyMetrics::default()
    }
}

/// Daily rows padded to a continuous run of `days` that ends on `today`.
/// Oldest first. A gap in the data becomes a blank row, not a missing one.
pub fn daily_series(rows: Vec<DailyMetrics>, days: u32, today: NaiveDate) -> Vec<DailyMetrics> {
    let mut by_date: HashMap<String, DailyMetrics> =
        rows.into_iter().map(|r| (r.date.clone(), r)).collect();
    (0..days)
        .rev()
        .map(|i| {
            let key = date_key(today - Duration::days(i as i64));
            by_date.remove(&key).unwrap_or_else(|| blank_day(key))
        })
        .collect()
}

/// Whether a day's resting heart rate is the overnight measurement.
fn resting_hr_is_overnight(d: &DailyMetrics) -> bool {
    d.sleep_secs.is_some_and(|s| s >= SLEEP_FOR_RHR_SECS)
}

/// The window with daytime-estimate resting heart rates blanked out.
///
/// Blanked, not removed. The day keeps its place on the axis, and a gap in the
/// line shows a night that the watch spent off the wrist.
pub fn comparable_resting_hr(rows: &[DailyMetrics]) -> Vec<DailyMetrics> {
    rows.iter()
        .map(|r| {
            let mut r = r.clone();
            if !resting_hr_is_overnight(&r) {
                r.resting_hr = None;
            }
            r
        })
        .collect()
}

/// The most recent day that has a value, with its date.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Latest {
    pub value: f64,
    pub date: String,
}

pub fn latest(rows: &[DailyMetrics], get: impl Fn(&DailyMetrics) -> Option<f64>) -> Option<Latest> {
    rows.iter().rev().find_map(|r| {
        get(r).filter(|v| v.is_finite()).map(|value| Latest {
            value,
            date: r.date.clone(),
        })
    })
}

/* ------------------------------------------------------------------ fuel --- */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fuel {
    /// The most recent day with a food log, or `None` if the window has none.
    pub day: Option<DailyMetrics>,
    /// 0 when that day is today, 1 for yesterday, and so on.
    pub age: Option<usize>,
    /// Days in the window, and how many of them have a food log.
    pub window: usize,
    pub logged: usize,
    /// Averaged over logged days only. A blank day is not a day of no food.
    pub avg_balance: Option<f64>,
    /// Burn comes from the device, so it exists on days with no food log.
    pub avg_burn: Option<f64>,
    /// Eaten minus burned for `day`. `None` unless both sides exist.
    pub balance: Option<f64>,
    /// True for a deficit of 500 kcal or more.
    pub accent: bool,
}

/// The food side of the last `days` of `rows`.
pub fn fuel(rows: &[DailyMetrics], days: usize) -> Fuel {
    let window = &rows[rows.len().saturating_sub(days)..];
    let logged: Vec<&DailyMetrics> = window
        .iter()
        .filter(|d| d.consumed_kcal.is_some())
        .collect();
    let last_idx = window.iter().rposition(|d| d.consumed_kcal.is_some());

    let day = last_idx.map(|i| window[i].clone());
    let balance = day.as_ref().and_then(DailyMetrics::calorie_balance);
    Fuel {
        balance,
        accent: balance.is_some_and(|b| b <= -500.0),
        day,
        age: last_idx.map(|i| window.len() - 1 - i),
        window: window.len(),
        logged: logged.len(),
        avg_balance: mean(logged.iter().map(|d| d.calorie_balance())),
        avg_burn: mean(window.iter().map(|d| d.total_burn_kcal)),
    }
}

/* ------------------------------------------------------------- hydration --- */

/// Hydration in ml, where 0 means "not logged". Garmin writes 0, not null.
fn hydration_ml(d: &DailyMetrics) -> Option<f64> {
    d.hydration_ml.filter(|v| *v > 0.0)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HydrationLatest {
    pub ml: f64,
    pub age: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hydration {
    pub logged: usize,
    pub window: usize,
    pub avg_ml: f64,
    pub latest: Option<HydrationLatest>,
    pub goal_ml: Option<f64>,
}

/// The hydration side of the last `days`, or `None` when the account does not
/// track it. That is the common case.
pub fn hydration(rows: &[DailyMetrics], days: usize) -> Option<Hydration> {
    let window = &rows[rows.len().saturating_sub(days)..];
    let values: Vec<Option<f64>> = window.iter().map(hydration_ml).collect();
    let real: Vec<f64> = values.iter().flatten().copied().collect();
    if real.is_empty() {
        return None;
    }
    let last_idx = values.iter().rposition(Option::is_some);
    Some(Hydration {
        logged: real.len(),
        window: window.len(),
        avg_ml: real.iter().sum::<f64>() / real.len() as f64,
        latest: last_idx.map(|i| HydrationLatest {
            ml: values[i].unwrap_or(0.0),
            age: values.len() - 1 - i,
        }),
        goal_ml: window
            .iter()
            .filter_map(|d| d.hydration_goal_ml)
            .rfind(|v| *v > 0.0),
    })
}

/* ----------------------------------------------------------------- zones --- */

fn zone_total(a: &CachedActivity) -> f64 {
    a.zone_secs.iter().sum()
}

/// True when the session recorded HR at all. Strength work often did not.
pub fn has_zone_data(a: &CachedActivity) -> bool {
    zone_total(a) > 0.0
}

/// Share of time above Z2, in percent. 0 when the session has no HR.
pub fn above_z2_pct(a: &CachedActivity) -> f64 {
    let total = zone_total(a);
    if total <= 0.0 {
        return 0.0;
    }
    (a.zone_secs[2] + a.zone_secs[3] + a.zone_secs[4]) / total * 100.0
}

#[derive(Debug, Clone, PartialEq)]
pub struct EasyHard {
    pub easy_pct: f64,
    pub hard_pct: f64,
    pub counted: usize,
}

/// Time-weighted easy (Z1-Z2) against hard (Z3-Z5) split.
/// Sessions with no HR are left out, not counted as zero.
pub fn easy_hard_split(activities: &[&CachedActivity]) -> Option<EasyHard> {
    let tracked: Vec<_> = activities.iter().filter(|a| has_zone_data(a)).collect();
    let easy: f64 = tracked
        .iter()
        .map(|a| a.zone_secs[0] + a.zone_secs[1])
        .sum();
    let hard: f64 = tracked
        .iter()
        .map(|a| a.zone_secs[2..].iter().sum::<f64>())
        .sum();
    let total = easy + hard;
    if tracked.is_empty() || total <= 0.0 {
        return None;
    }
    Some(EasyHard {
        easy_pct: easy / total * 100.0,
        hard_pct: hard / total * 100.0,
        counted: tracked.len(),
    })
}

/* ----------------------------------------------------------------- weeks --- */

/// Distance in km per day over the last `days`, oldest first.
/// Zero on a rest day. A rest day is data, not a gap.
pub fn daily_distance(activities: &[CachedActivity], days: u32, today: NaiveDate) -> Vec<f64> {
    daily_sum(activities, days, today, |a| {
        a.distance_m.unwrap_or(0.0) / 1000.0
    })
}

/// Active minutes per day over the last `days`, oldest first.
pub fn daily_minutes(activities: &[CachedActivity], days: u32, today: NaiveDate) -> Vec<f64> {
    daily_sum(activities, days, today, |a| {
        a.duration_s.unwrap_or(0.0) / 60.0
    })
}

fn daily_sum(
    activities: &[CachedActivity],
    days: u32,
    today: NaiveDate,
    get: impl Fn(&CachedActivity) -> f64,
) -> Vec<f64> {
    let mut totals: HashMap<&str, f64> = HashMap::new();
    for a in activities {
        if let Some(d) = a.local_date.as_deref() {
            *totals.entry(d).or_default() += get(a);
        }
    }
    (0..days)
        .rev()
        .map(|i| {
            let key = date_key(today - Duration::days(i as i64));
            totals.get(key.as_str()).copied().unwrap_or(0.0)
        })
        .collect()
}

/// Activities in the last `days`, counting today.
pub fn count_sessions(activities: &[CachedActivity], days: i64, today: NaiveDate) -> usize {
    let cutoff = date_key(today - Duration::days(days - 1));
    activities
        .iter()
        .filter(|a| a.local_date.as_deref().unwrap_or("") >= cutoff.as_str())
        .count()
}

/* ---------------------------------------------------------- observations --- */

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub label: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Observation {
    /// Accent items are the ones worth acting on today.
    pub accent: bool,
    pub text: String,
    /// Optional 14-point series for an inline sparkline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spark: Option<Vec<Option<f64>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spark_unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<Link>,
    /// The id of the Insights item that tells more about this note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insight: Option<String>,
}

fn note(accent: bool, text: String) -> Observation {
    Observation {
        accent,
        text,
        spark: None,
        spark_unit: None,
        link: None,
        insight: None,
    }
}

fn with_insight(mut o: Observation, id: &str) -> Observation {
    o.insight = Some(id.into());
    o
}

fn with_link(mut o: Observation, label: &str, to: &str) -> Observation {
    o.link = Some(Link {
        label: label.into(),
        to: to.into(),
    });
    o
}

fn with_spark(mut o: Observation, series: &[Option<f64>], unit: &str) -> Observation {
    o.spark = Some(series[series.len().saturating_sub(14)..].to_vec());
    o.spark_unit = Some(unit.into());
    o
}

/// The "Attention" list. Each entry is a threshold crossed by real numbers.
pub fn attention(
    daily: &[DailyMetrics],
    activities: &[CachedActivity],
    today: NaiveDate,
) -> Vec<Observation> {
    let runs: Vec<&CachedActivity> = activities
        .iter()
        .filter(|a| is_run(&a.type_key))
        .take(5)
        .collect();
    [
        resting_hr_note(daily),
        hrv_note(daily),
        zone_drift_note(&runs),
        cadence_note(&runs),
        fuel_note(daily),
        gps_note(activities, today),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Resting HR against its own baseline. The baseline leaves out the recent
/// window, so a sustained rise cannot lift the baseline it is measured against.
fn resting_hr_note(daily: &[DailyMetrics]) -> Option<Observation> {
    let rhr: Vec<Option<f64>> = daily.iter().map(|d| d.resting_hr).collect();
    let split = rhr.len().saturating_sub(3);
    let recent: Vec<f64> = rhr[split..].iter().flatten().copied().collect();
    let baseline = mean(rhr[..split].iter().copied())?;
    if recent.len() < 2 {
        return None;
    }
    let delta = recent.iter().sum::<f64>() / recent.len() as f64 - baseline;
    if delta < 3.0 {
        return None;
    }
    let text = format!(
        "Resting heart rate is {delta:.0} bpm above your {}-day baseline across the last {} mornings.",
        rhr.iter().flatten().count(),
        recent.len()
    );
    let o = with_spark(note(true, text), &rhr, "bpm");
    Some(with_insight(
        with_link(o, "Ask about this", "/ask"),
        "recovery-drivers",
    ))
}

/// HRV falling over the last five nights against the fortnight before.
fn hrv_note(daily: &[DailyMetrics]) -> Option<Observation> {
    let hrv: Vec<Option<f64>> = daily.iter().map(|d| d.hrv_last_night).collect();
    let n = hrv.len();
    let recent = mean(hrv[n.saturating_sub(5)..].iter().copied())?;
    let prior = mean(
        hrv[n.saturating_sub(20)..n.saturating_sub(5)]
            .iter()
            .copied(),
    )?;
    if recent >= prior * 0.92 {
        return None;
    }
    let text = format!(
        "HRV has averaged {recent:.0} ms over the last five nights, down from {prior:.0} ms in the fortnight before."
    );
    Some(with_insight(
        with_spark(note(true, text), &hrv, "ms"),
        "recovery-drivers",
    ))
}

/// Zone drift: too much of the recent running was above Z2.
fn zone_drift_note(runs: &[&CachedActivity]) -> Option<Observation> {
    let split = easy_hard_split(runs).filter(|s| s.hard_pct > 50.0 && s.counted >= 3)?;
    let text = format!(
        "{:.0}% of your last {} runs with heart-rate data was above Z2. The 80/20 target is 20%.",
        split.hard_pct, split.counted
    );
    Some(with_insight(
        with_link(note(true, text), "Insights", "/insights"),
        "hard-runs",
    ))
}

fn cadence_note(runs: &[&CachedActivity]) -> Option<Observation> {
    let low: Vec<Option<f64>> = runs
        .iter()
        .filter(|a| a.avg_cadence.is_some_and(|c| c < 155.0))
        .map(|a| a.avg_cadence)
        .collect();
    if low.len() < 3 {
        return None;
    }
    let avg = mean(low)?;
    Some(with_insight(
        note(
            false,
            format!("Cadence is averaging {avg:.0} spm across your recent runs. Quicker, lighter steps nearer 170 cut joint load."),
        ),
        "low-cadence",
    ))
}

/// Fuelling, only where the log is dense enough to mean something.
fn fuel_note(daily: &[DailyMetrics]) -> Option<Observation> {
    let f = fuel(daily, 7);
    let balance = f.avg_balance.filter(|_| f.logged >= 3)?;
    let over = if f.logged == f.window {
        String::new()
    } else {
        format!(" across the {} of 7 days you logged", f.logged)
    };
    if balance <= -500.0 {
        let text = format!(
            "You're averaging a {:.0} kcal daily deficit{over}. That's a big gap to be active on — under-fuelling shows up as low energy, slow recovery and a resting heart rate that won't settle.",
            balance.abs()
        );
        Some(with_link(note(true, text), "Food", "/food"))
    } else if balance >= 700.0 {
        let text = format!(
            "You're averaging a {balance:.0} kcal daily surplus{over}. Worth knowing rather than worth fixing — at {:.0} kcal burned a day it's a choice, not a slip.",
            f.avg_burn.unwrap_or(0.0)
        );
        Some(with_link(note(false, text), "Food", "/food"))
    } else {
        None
    }
}

/// Garmin computes VO2 max only from outdoor GPS runs.
fn gps_note(activities: &[CachedActivity], today: NaiveDate) -> Option<Observation> {
    let cutoff = date_key(today - Duration::days(42));
    let outdoor = activities.iter().any(|a| {
        is_run(&a.type_key)
            && a.type_key.as_deref() != Some("treadmill_running")
            && a.local_date.as_deref().unwrap_or("") >= cutoff.as_str()
    });
    let any_run = activities.iter().any(|a| is_run(&a.type_key));
    if outdoor || !any_run {
        return None;
    }
    Some(note(
        false,
        "No outdoor GPS run in the last six weeks, so VO2 max still isn't calculating. One easy outdoor run starts it tracking.".into(),
    ))
}

/// Days since an activity, or `None` if the date will not parse.
pub fn days_since(a: &CachedActivity, today: NaiveDate) -> Option<i64> {
    days_between(
        a.start_time_local.as_deref().or(a.local_date.as_deref())?,
        today,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn d(date: &str) -> NaiveDate {
        NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap()
    }

    fn day(date: &str) -> DailyMetrics {
        blank_day(date.into())
    }

    fn run(date: &str, zones: [f64; 5]) -> CachedActivity {
        CachedActivity {
            activity_id: 1,
            name: None,
            type_key: Some("running".into()),
            start_time_local: None,
            local_date: Some(date.into()),
            distance_m: Some(5000.0),
            duration_s: Some(1800.0),
            moving_duration_s: None,
            avg_hr: None,
            max_hr: None,
            avg_cadence: None,
            calories: None,
            elevation_gain: None,
            steps: None,
            aerobic_te: None,
            anaerobic_te: None,
            zone_secs: zones,
            training_load: None,
        }
    }

    #[test]
    fn series_pads_gaps_oldest_first() {
        let mut a = day("2026-03-10");
        a.steps = Some(100);
        let out = daily_series(vec![a], 3, d("2026-03-11"));
        let dates: Vec<_> = out.iter().map(|r| r.date.as_str()).collect();
        assert_eq!(dates, ["2026-03-09", "2026-03-10", "2026-03-11"]);
        assert_eq!(out[1].steps, Some(100));
        assert_eq!(out[0].steps, None);
    }

    #[test]
    fn daytime_resting_hr_is_blanked() {
        let mut worn = day("2026-03-10");
        worn.sleep_secs = Some(7.0 * 3600.0);
        worn.resting_hr = Some(50.0);
        let mut off = day("2026-03-11");
        off.sleep_secs = Some(1000.0);
        off.resting_hr = Some(60.0);
        let out = comparable_resting_hr(&[worn, off]);
        assert_eq!(out[0].resting_hr, Some(50.0));
        assert_eq!(out[1].resting_hr, None);
    }

    #[test]
    fn fuel_averages_logged_days_only() {
        let mut rows: Vec<_> = (1..=7).map(|i| day(&format!("2026-03-0{i}"))).collect();
        for i in [1, 3] {
            rows[i].consumed_kcal = Some(2000.0);
            rows[i].total_burn_kcal = Some(2500.0);
        }
        rows[5].total_burn_kcal = Some(3000.0);
        let f = fuel(&rows, 7);
        assert_eq!(f.logged, 2);
        assert_eq!(f.age, Some(3));
        assert_eq!(f.avg_balance, Some(-500.0));
        assert_eq!(f.avg_burn, Some((2500.0 * 2.0 + 3000.0) / 3.0));
        assert_eq!(f.balance, Some(-500.0));
        assert!(f.accent);
    }

    #[test]
    fn fuel_without_log_has_no_day() {
        let f = fuel(&[day("2026-03-01")], 7);
        assert!(f.day.is_none() && f.avg_balance.is_none());
    }

    #[test]
    fn hydration_zero_means_not_logged() {
        let mut rows = vec![day("2026-03-01"), day("2026-03-02")];
        rows[0].hydration_ml = Some(0.0);
        assert!(hydration(&rows, 7).is_none());
        rows[0].hydration_ml = Some(1500.0);
        rows[0].hydration_goal_ml = Some(2500.0);
        let h = hydration(&rows, 7).unwrap();
        assert_eq!(h.latest, Some(HydrationLatest { ml: 1500.0, age: 1 }));
        assert_eq!(h.goal_ml, Some(2500.0));
    }

    #[test]
    fn easy_hard_skips_sessions_without_hr() {
        let a = run("2026-03-01", [600.0, 600.0, 400.0, 200.0, 0.0]);
        let b = run("2026-03-02", [0.0; 5]);
        let s = easy_hard_split(&[&a, &b]).unwrap();
        assert_eq!(s.counted, 1);
        assert!((s.hard_pct - 33.333).abs() < 0.01);
        assert!(easy_hard_split(&[&b]).is_none());
    }

    #[test]
    fn distance_is_zero_on_rest_days() {
        let acts = vec![run("2026-03-11", [0.0; 5]), run("2026-03-11", [0.0; 5])];
        let out = daily_distance(&acts, 3, d("2026-03-11"));
        assert_eq!(out, [0.0, 0.0, 10.0]);
        assert_eq!(daily_minutes(&acts, 2, d("2026-03-11")), [0.0, 60.0]);
        assert_eq!(count_sessions(&acts, 2, d("2026-03-11")), 2);
        assert_eq!(count_sessions(&acts, 1, d("2026-03-12")), 0);
    }

    #[test]
    fn resting_hr_note_needs_a_rise_of_three() {
        let mut rows: Vec<_> = (10..=20).map(|i| day(&format!("2026-03-{i}"))).collect();
        for r in &mut rows {
            r.resting_hr = Some(50.0);
        }
        assert!(resting_hr_note(&rows).is_none());
        for r in rows.iter_mut().rev().take(3) {
            r.resting_hr = Some(55.0);
        }
        let n = resting_hr_note(&rows).unwrap();
        assert!(n.text.starts_with("Resting heart rate is 5 bpm"));
        assert_eq!(n.spark.unwrap().len(), 11);
    }

    #[test]
    fn hrv_note_flags_a_fall() {
        let mut rows: Vec<_> = (1..=20).map(|i| day(&format!("2026-03-{i:02}"))).collect();
        for (i, r) in rows.iter_mut().enumerate() {
            r.hrv_last_night = Some(if i >= 15 { 40.0 } else { 60.0 });
        }
        assert!(hrv_note(&rows).unwrap().text.contains("40 ms"));
    }

    #[test]
    fn fuel_note_needs_three_logged_days() {
        let mut rows: Vec<_> = (1..=7).map(|i| day(&format!("2026-03-0{i}"))).collect();
        for r in rows.iter_mut().take(2) {
            r.consumed_kcal = Some(1500.0);
            r.total_burn_kcal = Some(2500.0);
        }
        assert!(fuel_note(&rows).is_none());
        rows[2].consumed_kcal = Some(1500.0);
        rows[2].total_burn_kcal = Some(2500.0);
        let n = fuel_note(&rows).unwrap();
        assert!(n.accent && n.text.contains("across the 3 of 7 days"));
    }

    #[test]
    fn gps_note_only_for_treadmill_only_runners() {
        let mut t = run("2026-03-10", [0.0; 5]);
        t.type_key = Some("treadmill_running".into());
        assert!(gps_note(&[t.clone()], d("2026-03-11")).is_some());
        assert!(gps_note(&[], d("2026-03-11")).is_none());
        let o = run("2026-03-10", [0.0; 5]);
        assert!(gps_note(&[t, o], d("2026-03-11")).is_none());
    }
}
