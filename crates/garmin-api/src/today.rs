//! The Today screen as one view model.
//!
//! The old app made six calls and derived the rest in TypeScript. This builds
//! the same numbers and sentences in one pass over the cache. The UI only
//! lays them out.
//!
//! Left out on purpose: the written opening (needs a model), the coach panel,
//! and the greeting name (needs the Garmin profile, which is a network call).

use crate::derive::{
    above_z2_pct, attention, comparable_resting_hr, count_sessions, daily_distance, daily_minutes,
    daily_series, date_key, days_since, fuel, has_zone_data, hydration, is_run, latest, mean, Fuel,
    Hydration, Latest, Observation,
};
use crate::headline::{headline, Headline, HeadlineInput, HrvRead};
use crate::insights::{load, runs_now};
use crate::profile::{activity_profile, ActivityProfile, WINDOW_DAYS};
use crate::text::{fuel_sentence, hydration_sentence, narrative, week_sentence, weight_sentence};
use crate::ApiError;
use chrono::NaiveDate;
use garmin_core::analysis::{discipline, Discipline};
use garmin_core::sport::{family, is_exercise, shows_zone_verdict, sport_label};
use garmin_core::{query, CachedActivity, Db, Family};
use serde::{Deserialize, Serialize};

/// Days of daily rows and activities that the screen reads.
const DAILY_DAYS: u32 = 45;
/// Enough rows to cover the 56-day activity profile.
const ACTIVITY_LIMIT: u32 = 300;
/// Days of resting heart rate that make a baseline for the headline.
const MIN_BASELINE_DAYS: usize = 4;
/// Runs in the zone drift strip. A phone UI can show fewer from the end.
const DRIFT_RUNS: usize = 8;
/// The 80/20 model asks for 20% of time above Z2.
const DRIFT_TARGET_PCT: f64 = 20.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TodayView {
    /// The day the view was built for, `YYYY-MM-DD`.
    pub date: String,
    /// True when the cache holds no activity and no daily row.
    pub empty: bool,
    /// The local opening paragraph. A UI shows it when no model text exists.
    pub lede: String,
    pub sleep: Option<Latest>,
    pub body_battery: Option<Latest>,
    pub readiness: Option<Latest>,
    pub hrv: Option<Latest>,
    pub steps: Option<Latest>,
    pub stress: Option<Latest>,
    pub resting_hr: Option<RestingHr>,
    pub fuel: Fuel,
    pub fuel_sentence: String,
    pub hydration: Option<Hydration>,
    pub hydration_sentence: Option<String>,
    pub weight: Option<WeightGlance>,
    pub attention: Vec<Observation>,
    pub last_session: Option<LastSession>,
    pub zone_drift: ZoneDrift,
    pub week: Week,
    /// What the user does, from the last 56 days of exercise sessions.
    pub profile: ActivityProfile,
    /// Section keys in drawing order. A UI draws these and skips a key that it
    /// does not know. Keys: `health_tiles`, `attention`, `fuel`, `weight`,
    /// `last_session`, `week`, `zone_drift`.
    #[cfg_attr(
        feature = "ts-rs",
        ts(
            type = "Array<\"health_tiles\" | \"attention\" | \"fuel\" | \"weight\" | \"last_session\" | \"week\" | \"zone_drift\">"
        )
    )]
    pub sections: Vec<String>,
    /// The verdict and the sentence under it. Built from readiness, resting
    /// heart rate, HRV, sleep and load.
    pub headline: Headline,
}

/// This morning's overnight resting HR against the 30-day mean.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct RestingHr {
    pub value: f64,
    pub baseline: f64,
    pub delta: f64,
    /// True for a rise of 3 bpm or more. A drop is good news and has no accent.
    pub accent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct WeightGlance {
    pub trend_kg: Option<f64>,
    pub days_since_latest: Option<i64>,
    pub rate_kg_per_week: Option<f64>,
    pub count: usize,
    pub sentence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct LastSession {
    pub activity: CachedActivity,
    pub has_zone_data: bool,
    pub above_z2_pct: f64,
    pub discipline: Discipline,
    pub family: Family,
    /// True only for paced and endurance sport. A zone verdict is false for
    /// strength, golf and other sport.
    pub show_zone_verdict: bool,
    /// A readable sport name, for example "Treadmill running".
    pub sport_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct DriftRun {
    pub activity_id: i64,
    pub name: Option<String>,
    pub type_key: Option<String>,
    pub sport_label: String,
    pub date: Option<String>,
    pub above_z2_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ZoneDrift {
    /// What the strip counts, for the heading. Always "runs" today, because
    /// the strip holds runs only.
    #[serde(default = "drift_noun")]
    pub noun: String,
    pub target_pct: f64,
    /// Mean of the newest three runs. `None` with no run.
    pub recent_avg_pct: Option<f64>,
    /// Runs with HR data, oldest first. The UI shows the strip at two or more.
    pub runs: Vec<DriftRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Week {
    /// Distance in km for each of the last seven days, oldest first.
    pub daily_km: Vec<f64>,
    pub total_km: f64,
    pub prior_km: f64,
    /// Active minutes for each of the last seven days, oldest first.
    pub daily_minutes: Vec<f64>,
    pub total_minutes: f64,
    pub prior_minutes: f64,
    pub sessions: usize,
    pub sentence: String,
}

/// Build the Today view for `today`.
pub fn today_view(db: &Db, today: NaiveDate) -> Result<TodayView, ApiError> {
    let from = date_key(today - chrono::Duration::days(DAILY_DAYS as i64 - 1));
    let stored = db.daily_since(&from)?;
    // Non-exercise rows (a motorcycle ride) stay out of every count below.
    let activities: Vec<CachedActivity> = db
        .recent_activities(ACTIVITY_LIMIT, None)?
        .into_iter()
        .filter(|a| is_exercise(a.type_key.as_deref()))
        .collect();
    // A weight failure must not hide the rest of the screen.
    let weight = query::weight(db, 180).ok();

    let empty = stored.is_empty() && activities.is_empty();
    let data_days = stored.len();
    let oldest_day = stored
        .iter()
        .filter_map(|d| NaiveDate::parse_from_str(&d.date, "%Y-%m-%d").ok())
        .min();
    let rows = daily_series(stored, DAILY_DAYS, today);
    let comparable = comparable_resting_hr(&rows);
    let food = fuel(&rows, 7);
    let water = hydration(&rows, 7);

    let profile = activity_profile(&activities, data_days, today);
    let visible = run_visible(&activities, today);

    let mut view = TodayView {
        date: date_key(today),
        empty,
        lede: narrative(&rows, &visible, today),
        sleep: latest(&rows, |d| d.sleep_secs),
        body_battery: latest(&rows, |d| d.body_battery_high),
        readiness: latest(&rows, |d| d.training_readiness),
        hrv: latest(&rows, |d| d.hrv_last_night),
        steps: latest(&rows, |d| d.steps.map(|s| s as f64)),
        stress: latest(&rows, |d| d.stress_avg),
        resting_hr: resting_hr(&comparable),
        fuel_sentence: fuel_sentence(&food),
        fuel: food,
        hydration_sentence: water.as_ref().map(hydration_sentence),
        hydration: water,
        weight: weight.filter(|w| w.count > 0).map(|weight| WeightGlance {
            trend_kg: weight.trend_kg,
            days_since_latest: weight.days_since_latest,
            rate_kg_per_week: weight.rate_kg_per_week,
            count: weight.count,
            sentence: weight_sentence(
                weight.trend_kg,
                weight.days_since_latest,
                weight.rate_kg_per_week,
            ),
        }),
        attention: attention(&rows, &visible, today),
        last_session: activities.first().map(last_session),
        zone_drift: zone_drift(&visible),
        week: week(&activities, today),
        profile,
        sections: Vec::new(),
        headline: headline(&headline_input(
            &rows,
            &comparable,
            &activities,
            oldest_day,
            today,
        )),
    };
    view.sections = sections(&view);
    Ok(view)
}

/// A reading counts for the headline when it is from today or yesterday. An
/// older one says nothing about this morning.
fn fresh(l: Option<Latest>, today: NaiveDate) -> Option<Latest> {
    l.filter(|l| {
        NaiveDate::parse_from_str(&l.date, "%Y-%m-%d").is_ok_and(|d| (today - d).num_days() <= 1)
    })
}

/// A session counts as hard when much of it was above Z2, or when Garmin
/// rates its aerobic effect as high.
fn is_hard(a: &CachedActivity) -> bool {
    (has_zone_data(a) && above_z2_pct(a) >= 40.0) || a.aerobic_te.is_some_and(|te| te >= 4.0)
}

fn headline_input(
    rows: &[garmin_core::DailyMetrics],
    comparable: &[garmin_core::DailyMetrics],
    activities: &[CachedActivity],
    oldest_day: Option<NaiveDate>,
    today: NaiveDate,
) -> HeadlineInput {
    let hrv = fresh(latest(rows, |d| d.hrv_last_night), today).map(|l| {
        let row = rows.iter().find(|d| d.date == l.date);
        HrvRead {
            value: l.value,
            balanced_low: row.and_then(|d| d.hrv_balanced_low),
            balanced_high: row.and_then(|d| d.hrv_balanced_upper),
        }
    });
    let yesterday = date_key(today - chrono::Duration::days(1));
    HeadlineInput {
        readiness: fresh(latest(rows, |d| d.training_readiness), today).map(|l| l.value),
        // A new user has no baseline yet: the delta would be 0 and read as "recovered".
        resting_delta: fresh(latest(comparable, |d| d.resting_hr), today)
            .zip(resting_baseline(comparable))
            .map(|(l, baseline)| l.value - baseline),
        hrv,
        sleep_secs: fresh(latest(rows, |d| d.sleep_secs), today).map(|l| l.value),
        load_ratio: load(activities, oldest_day, today).map(|l| l.ratio),
        hard_yesterday: activities
            .iter()
            .find(|a| a.local_date.as_deref() == Some(yesterday.as_str()) && is_hard(a))
            .map(|a| sport_label(a.type_key.as_deref())),
    }
}

/// The sessions that run-only copy may read. A run counts only when the user
/// runs now (the same test as Insights) and the run is inside the window, so a
/// user with one run, or one who stopped running, gets no run advice.
fn run_visible(activities: &[CachedActivity], today: NaiveDate) -> Vec<CachedActivity> {
    let runs_count = runs_now(activities, today);
    activities
        .iter()
        .filter(|a| {
            !is_run(&a.type_key)
                || (runs_count && days_since(a, today).is_some_and(|d| d < WINDOW_DAYS))
        })
        .cloned()
        .collect()
}

/// The sections to draw, in order. Health first. A sport section appears only
/// when the data shows that sport. Zone drift is last and needs 2 or more runs.
fn sections(v: &TodayView) -> Vec<String> {
    let health = [
        &v.sleep,
        &v.body_battery,
        &v.readiness,
        &v.hrv,
        &v.steps,
        &v.stress,
    ];
    let order = [
        (
            "health_tiles",
            health.iter().any(|t| t.is_some()) || v.resting_hr.is_some(),
        ),
        ("attention", !v.attention.is_empty()),
        ("fuel", v.fuel.logged > 0 || v.hydration.is_some()),
        ("weight", v.weight.is_some()),
        ("last_session", v.last_session.is_some()),
        ("week", v.profile.tracks_activity || v.week.sessions > 0),
        ("zone_drift", v.zone_drift.runs.len() >= 2),
    ];
    order
        .into_iter()
        .filter(|(_, show)| *show)
        .map(|(key, _)| key.to_string())
        .collect()
}

/// The mean resting heart rate, once there are enough days to be a baseline.
fn resting_baseline(comparable: &[garmin_core::DailyMetrics]) -> Option<f64> {
    let days = comparable.iter().filter(|d| d.resting_hr.is_some()).count();
    (days >= MIN_BASELINE_DAYS)
        .then(|| mean(comparable.iter().map(|d| d.resting_hr)))
        .flatten()
}

fn resting_hr(comparable: &[garmin_core::DailyMetrics]) -> Option<RestingHr> {
    let value = latest(comparable, |d| d.resting_hr)?.value;
    let baseline = resting_baseline(comparable)?;
    let delta = value - baseline;
    Some(RestingHr {
        value,
        baseline,
        delta,
        accent: delta >= 3.0,
    })
}

fn last_session(a: &CachedActivity) -> LastSession {
    let key = a.type_key.as_deref();
    LastSession {
        activity: a.clone(),
        has_zone_data: has_zone_data(a),
        above_z2_pct: above_z2_pct(a),
        discipline: discipline(key),
        family: family(key),
        show_zone_verdict: shows_zone_verdict(key),
        sport_label: sport_label(key),
    }
}

fn drift_noun() -> String {
    "runs".to_string()
}

fn zone_drift(activities: &[CachedActivity]) -> ZoneDrift {
    let mut runs: Vec<DriftRun> = activities
        .iter()
        .filter(|a| is_run(&a.type_key) && has_zone_data(a))
        .take(DRIFT_RUNS)
        .map(|a| DriftRun {
            activity_id: a.activity_id,
            name: a.name.clone(),
            type_key: a.type_key.clone(),
            sport_label: sport_label(a.type_key.as_deref()),
            date: a.start_time_local.clone().or_else(|| a.local_date.clone()),
            above_z2_pct: above_z2_pct(a),
        })
        .collect();
    runs.reverse();
    let recent = &runs[runs.len().saturating_sub(3)..];
    ZoneDrift {
        noun: drift_noun(),
        target_pct: DRIFT_TARGET_PCT,
        recent_avg_pct: mean(recent.iter().map(|r| Some(r.above_z2_pct))),
        runs,
    }
}

fn week(activities: &[CachedActivity], today: NaiveDate) -> Week {
    let daily_km = daily_distance(activities, 7, today);
    let total_km: f64 = daily_km.iter().sum();
    let prior_km: f64 = daily_distance(activities, 14, today)[..7].iter().sum();
    let daily_minutes_7 = daily_minutes(activities, 7, today);
    let total_minutes: f64 = daily_minutes_7.iter().sum();
    let prior_minutes: f64 = daily_minutes(activities, 14, today)[..7].iter().sum();
    let sessions = count_sessions(activities, 7, today);
    Week {
        sentence: week_sentence(total_minutes, prior_minutes, total_km, sessions),
        daily_minutes: daily_minutes_7,
        total_minutes,
        prior_minutes,
        daily_km,
        total_km,
        prior_km,
        sessions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use garmin_core::DailyMetrics;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 3, 11).unwrap()
    }

    fn db() -> Db {
        let dir = std::env::temp_dir().join(format!(
            "garmin-api-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        Db::open(&dir.join("cache.sqlite3")).unwrap()
    }

    #[test]
    fn empty_cache_is_flagged() {
        let v = today_view(&db(), today()).unwrap();
        assert!(v.empty && v.weight.is_none() && v.last_session.is_none());
        assert_eq!(v.week.daily_km, vec![0.0; 7]);
        assert!(v.zone_drift.runs.is_empty());
    }

    #[test]
    fn seeded_cache_fills_the_view() {
        let db = db();
        for (date, rhr, sleep) in [
            ("2026-03-07", 50.0, 7.0 * 3600.0),
            ("2026-03-08", 50.0, 7.0 * 3600.0),
            ("2026-03-09", 51.0, 7.0 * 3600.0),
            ("2026-03-10", 56.0, 7.0 * 3600.0),
            ("2026-03-11", 56.0, 1000.0),
        ] {
            db.upsert_daily(&DailyMetrics {
                date: date.into(),
                resting_hr: Some(rhr),
                sleep_secs: Some(sleep),
                ..DailyMetrics::default()
            })
            .unwrap();
        }
        let v = today_view(&db, today()).unwrap();
        assert!(!v.empty);
        assert_eq!(v.sleep.as_ref().unwrap().date, "2026-03-11");
        // Today's resting HR is a daytime estimate, so the overnight one leads.
        let rhr = v.resting_hr.unwrap();
        assert_eq!(rhr.value, 56.0);
        assert!(rhr.accent);
        assert!(v.lede.starts_with("You slept"));
    }

    #[test]
    fn a_new_user_is_not_told_that_they_are_recovered() {
        let db = db();
        for date in ["2026-03-09", "2026-03-10", "2026-03-11"] {
            db.upsert_daily(&DailyMetrics {
                date: date.into(),
                resting_hr: Some(50.0),
                // Under 4 h of sleep clears the resting HR as not comparable.
                sleep_secs: Some(5.0 * 3600.0),
                ..DailyMetrics::default()
            })
            .unwrap();
        }
        let v = today_view(&db, today()).unwrap();
        assert!(
            !v.headline.detail.contains("resting heart rate"),
            "{:?}",
            v.headline
        );
        assert!(v.resting_hr.is_none());
    }

    #[test]
    fn run_copy_needs_three_runs_like_insights() {
        let run = |id: i64, date: &str, key: &str| CachedActivity {
            activity_id: id,
            name: None,
            type_key: Some(key.into()),
            start_time_local: None,
            local_date: Some(date.into()),
            distance_m: None,
            duration_s: None,
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
            training_load: None,
        };
        let one = [
            run(1, "2026-03-10", "running"),
            run(2, "2026-03-10", "golf"),
        ];
        let kept = run_visible(&one, today());
        assert_eq!(kept.iter().map(|a| a.activity_id).collect::<Vec<_>>(), [2]);
        let three: Vec<_> = (1..=3).map(|i| run(i, "2026-03-10", "running")).collect();
        assert_eq!(run_visible(&three, today()).len(), 3);
    }

    #[test]
    fn zone_drift_is_oldest_first_with_recent_mean() {
        let mk = |id: i64, hard: f64| CachedActivity {
            activity_id: id,
            name: None,
            type_key: Some("running".into()),
            start_time_local: None,
            local_date: Some("2026-03-10".into()),
            distance_m: None,
            duration_s: None,
            moving_duration_s: None,
            avg_hr: None,
            max_hr: None,
            avg_cadence: None,
            calories: None,
            elevation_gain: None,
            steps: None,
            aerobic_te: None,
            anaerobic_te: None,
            zone_secs: [100.0 - hard, 0.0, hard, 0.0, 0.0],
            training_load: None,
        };
        // Newest first, as the database returns them.
        let d = zone_drift(&[mk(3, 60.0), mk(2, 30.0), mk(1, 0.0), mk(0, 0.0)]);
        assert_eq!(
            d.runs.iter().map(|r| r.activity_id).collect::<Vec<_>>(),
            [0, 1, 2, 3]
        );
        assert_eq!(d.recent_avg_pct, Some(30.0));
    }

    #[test]
    fn week_compares_with_the_prior_seven_days() {
        let mk = |date: &str| CachedActivity {
            activity_id: 1,
            name: None,
            type_key: Some("running".into()),
            start_time_local: None,
            local_date: Some(date.into()),
            distance_m: Some(10_000.0),
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
            zone_secs: [0.0; 5],
            training_load: None,
        };
        let w = week(
            &[mk("2026-03-11"), mk("2026-03-10"), mk("2026-03-02")],
            today(),
        );
        assert_eq!((w.total_km, w.prior_km, w.sessions), (20.0, 10.0, 2));
        assert_eq!((w.total_minutes, w.prior_minutes), (60.0, 30.0));
        assert!(w.sentence.contains("up 100%"));
    }
}
