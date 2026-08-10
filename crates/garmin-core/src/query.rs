//! Read-only analyses over the local cache.
//!
//! This is the one implementation of "what does the data say". Both the MCP
//! server and the desktop app's chat tools call it, so an answer given in Claude
//! Desktop and the same answer given in the app can't drift apart.
//!
//! Types here are plain serde. The MCP server mirrors them into its own
//! schemars-annotated shapes rather than this crate taking a dependency on the
//! MCP SDK's schemars version.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::db::{CachedActivity, Db};

/// Anything Garmin classifies as a run. Matched as a substring so
/// `treadmill_running`, `indoor_running` and `trail_running` all count.
const RUN_TYPES: &[&str] = &["running"];

pub fn is_run(a: &CachedActivity) -> bool {
    let key = a.type_key.as_deref().unwrap_or("");
    RUN_TYPES.iter().any(|t| key.contains(t))
}

pub fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Over-fetch and filter in memory: the SQL sport filter is a substring match,
/// and a strength session shouldn't dilute a running report.
fn recent_runs(db: &Db, count: u32) -> Result<Vec<CachedActivity>> {
    Ok(db
        .recent_activities(200, None)?
        .into_iter()
        .filter(is_run)
        .take(count as usize)
        .collect())
}

/* ------------------------------------------------------------------ shapes --- */

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneSplit {
    pub zone: u8,
    pub minutes: f64,
    pub percent: f64,
}

pub fn zone_splits(a: &CachedActivity) -> Vec<ZoneSplit> {
    let pcts = a.zone_percentages();
    a.zone_secs
        .iter()
        .enumerate()
        .map(|(i, secs)| ZoneSplit {
            zone: (i + 1) as u8,
            minutes: round1(secs / 60.0),
            percent: round1(pcts[i]),
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityView {
    pub activity_id: i64,
    pub name: Option<String>,
    pub sport: Option<String>,
    pub start: Option<String>,
    pub distance_km: Option<f64>,
    pub duration_min: Option<f64>,
    pub pace_min_per_km: Option<f64>,
    pub avg_hr: Option<f64>,
    pub max_hr: Option<f64>,
    pub avg_cadence: Option<f64>,
    pub aerobic_training_effect: Option<f64>,
    pub anaerobic_training_effect: Option<f64>,
    pub zones: Vec<ZoneSplit>,
    /// Share of tracked HR time in Z1+Z2. The number the 80/20 model turns on.
    pub easy_percent: f64,
    /// False when the session recorded no HR at all, in which case every zone
    /// reads zero — which is not the same as a session spent entirely in Z1.
    pub has_hr_data: bool,
    /// How far the zone split above can be trusted, and why not. The numbers
    /// are still the numbers — this says what they rest on.
    pub hr_confidence: crate::signal::HrConfidence,
    /// True for treadmill and other indoor work, where distance and pace come
    /// off the arm accelerometer rather than GPS or the belt. Heart rate and
    /// cadence indoors are measurements; these two are estimates, and
    /// comparing them across sessions compares two estimates.
    pub pace_estimated: bool,
    /// Pace over moving time rather than elapsed. Diverges from
    /// `pace_min_per_km` on a run/walk session, where it is the more useful of
    /// the two — the other averages the walk breaks in.
    pub moving_pace_min_per_km: Option<f64>,
}

impl From<&CachedActivity> for ActivityView {
    fn from(a: &CachedActivity) -> Self {
        let pcts = a.zone_percentages();
        Self {
            activity_id: a.activity_id,
            name: a.name.clone(),
            sport: a.type_key.clone(),
            start: a.start_time_local.clone(),
            distance_km: a.distance_m.map(|d| round2(d / 1000.0)),
            duration_min: a.duration_s.map(|s| round1(s / 60.0)),
            pace_min_per_km: a.pace_min_per_km().map(round1),
            avg_hr: a.avg_hr,
            max_hr: a.max_hr,
            avg_cadence: a.avg_cadence.map(round1),
            aerobic_training_effect: a.aerobic_te,
            anaerobic_training_effect: a.anaerobic_te,
            zones: zone_splits(a),
            easy_percent: round1(pcts[0] + pcts[1]),
            has_hr_data: a.zone_total_secs() > 0.0,
            // Averages only here. `ActivityView` is built for lists, where the
            // per-sample trace isn't loaded and loading fifty of them to
            // annotate a summary would be absurd; the session's own analysis
            // runs the stronger check.
            hr_confidence: crate::signal::hr_confidence(
                a.type_key.as_deref(),
                a.duration_s,
                a.avg_hr,
                a.avg_cadence,
                a.zone_total_secs() > 0.0,
                None,
            ),
            pace_estimated: crate::signal::is_indoor(a.type_key.as_deref()),
            moving_pace_min_per_km: a.moving_pace_min_per_km().map(round1),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftPoint {
    pub activity_id: i64,
    pub date: Option<String>,
    pub duration_min: Option<f64>,
    pub avg_hr: Option<f64>,
    pub z5_percent: f64,
    pub hard_percent: f64,
    pub easy_percent: f64,
    pub has_hr_data: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftReport {
    pub runs: Vec<DriftPoint>,
    /// Z1+Z2 share across the window, weighted by time — the honest 80/20
    /// number, as opposed to averaging per-run percentages.
    pub overall_easy_percent: f64,
    pub overall_hard_percent: f64,
    pub total_run_minutes: f64,
    pub longest_run_minutes: f64,
    /// How many of the runs examined actually recorded HR. The overall split is
    /// computed from these only.
    pub runs_with_hr: usize,
    pub runs_examined: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CadencePoint {
    pub activity_id: i64,
    pub date: Option<String>,
    pub avg_cadence: Option<f64>,
    pub pace_min_per_km: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CadenceReport {
    pub runs: Vec<CadencePoint>,
    pub average_cadence: Option<f64>,
    /// Runs with cadence data, out of the runs examined. A treadmill run
    /// without a footpod reports nothing.
    pub runs_with_cadence: usize,
    pub runs_examined: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryDay {
    pub date: String,
    pub resting_hr: Option<f64>,
    /// Which of Garmin's two resting-heart-rate measurements this is. Only
    /// `overnight` readings belong on one trend line with each other; a
    /// daytime estimate is a different measurement under the same name, and
    /// mixing them makes a trend out of whether the watch was worn to bed.
    pub resting_hr_source: crate::signal::RestingHrSource,
    pub hrv_last_night: Option<f64>,
    pub hrv_weekly_avg: Option<f64>,
    pub hrv_status: Option<String>,
    pub training_readiness: Option<f64>,
    pub sleep_hours: Option<f64>,
    pub sleep_score: Option<f64>,
    pub steps: Option<i64>,
    pub stress_avg: Option<f64>,
    pub body_battery_high: Option<f64>,
    pub body_battery_low: Option<f64>,
}

impl From<crate::DailyMetrics> for RecoveryDay {
    fn from(d: crate::DailyMetrics) -> Self {
        Self {
            resting_hr_source: crate::signal::resting_hr_source(d.sleep_secs),
            date: d.date,
            resting_hr: d.resting_hr,
            hrv_last_night: d.hrv_last_night,
            hrv_weekly_avg: d.hrv_weekly_avg,
            hrv_status: d.hrv_status,
            training_readiness: d.training_readiness,
            sleep_hours: d.sleep_secs.map(|s| round1(s / 3600.0)),
            sleep_score: d.sleep_score,
            steps: d.steps,
            stress_avg: d.stress_avg,
            body_battery_high: d.body_battery_high,
            body_battery_low: d.body_battery_low,
        }
    }
}

/// What's in the cache, and — separately — how current it is.
///
/// Those are two questions and this used to answer only the first. `last_sync`
/// says when the app last asked Garmin, which is a fact about the app; a sync
/// against a watch nobody has worn succeeds, writes nothing, and moves it to
/// now. The `newest_*` fields say when the data itself stops, which is a fact
/// about the athlete, and is the one that tells you whether an answer built on
/// this cache describes this week or a week last spring.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStatus {
    pub activities_cached: i64,
    pub last_sync: Option<String>,
    pub database_path: Option<String>,
    pub connected_to_garmin: bool,
    /// Local date of the newest cached activity, and how many days back it is.
    pub newest_activity_date: Option<String>,
    pub days_since_activity: Option<i64>,
    /// Same for wellness data — resting HR, HRV, sleep, readiness.
    pub newest_daily_date: Option<String>,
    pub days_since_daily: Option<i64>,
    /// Set when the cache has nothing from the last few days. The number is
    /// there to be reasoned about; this is here so the check is hard to skip.
    pub stale: bool,
}

/* ---------------------------------------------------------------- analyses --- */

pub fn recent_activities(db: &Db, limit: u32, sport: Option<&str>) -> Result<Vec<ActivityView>> {
    Ok(db
        .recent_activities(limit, sport)?
        .iter()
        .map(ActivityView::from)
        .collect())
}

/// One activity, or the most recent when `activity_id` is None.
pub fn activity_zones(db: &Db, activity_id: Option<i64>) -> Result<Option<ActivityView>> {
    let found = match activity_id {
        Some(id) => db.activity(id)?,
        None => db.recent_activities(1, None)?.into_iter().next(),
    };
    Ok(found.as_ref().map(ActivityView::from))
}

/// Hard-effort drift across recent runs.
///
/// The overall split counts only runs that recorded HR. Including a strapless
/// session would add zero to both buckets and silently drag the ratio toward
/// whatever the other runs happened to say.
pub fn zone_drift(db: &Db, count: u32) -> Result<DriftReport> {
    let runs = recent_runs(db, count)?;

    let mut easy_secs = 0.0;
    let mut hard_secs = 0.0;
    let mut total_secs = 0.0;
    let mut longest = 0.0f64;
    let mut with_hr = 0usize;

    let points: Vec<DriftPoint> = runs
        .iter()
        .map(|a| {
            let pcts = a.zone_percentages();
            let z = a.zone_secs;
            let has_hr = a.zone_total_secs() > 0.0;
            if has_hr {
                with_hr += 1;
                easy_secs += z[0] + z[1];
                hard_secs += z[2] + z[3] + z[4];
            }
            total_secs += a.duration_s.unwrap_or(0.0);
            longest = longest.max(a.duration_s.unwrap_or(0.0));
            DriftPoint {
                activity_id: a.activity_id,
                date: a.local_date.clone(),
                duration_min: a.duration_s.map(|s| round1(s / 60.0)),
                avg_hr: a.avg_hr,
                z5_percent: round1(pcts[4]),
                hard_percent: round1(pcts[2] + pcts[3] + pcts[4]),
                easy_percent: round1(pcts[0] + pcts[1]),
                has_hr_data: has_hr,
            }
        })
        .collect();

    let tracked = easy_secs + hard_secs;
    let (easy_pct, hard_pct) = if tracked > 0.0 {
        (easy_secs / tracked * 100.0, hard_secs / tracked * 100.0)
    } else {
        (0.0, 0.0)
    };

    Ok(DriftReport {
        runs_examined: points.len(),
        runs: points,
        overall_easy_percent: round1(easy_pct),
        overall_hard_percent: round1(hard_pct),
        total_run_minutes: round1(total_secs / 60.0),
        longest_run_minutes: round1(longest / 60.0),
        runs_with_hr: with_hr,
    })
}

pub fn cadence_trend(db: &Db, count: u32) -> Result<CadenceReport> {
    let runs = recent_runs(db, count)?;
    let with_cadence: Vec<f64> = runs.iter().filter_map(|a| a.avg_cadence).collect();
    let average = if with_cadence.is_empty() {
        None
    } else {
        Some(round1(
            with_cadence.iter().sum::<f64>() / with_cadence.len() as f64,
        ))
    };

    Ok(CadenceReport {
        runs_with_cadence: with_cadence.len(),
        runs_examined: runs.len(),
        average_cadence: average,
        runs: runs
            .iter()
            .map(|a| CadencePoint {
                activity_id: a.activity_id,
                date: a.local_date.clone(),
                avg_cadence: a.avg_cadence.map(round1),
                pace_min_per_km: a.pace_min_per_km().map(round1),
            })
            .collect(),
    })
}

pub fn recovery(db: &Db, days: u32) -> Result<Vec<RecoveryDay>> {
    Ok(db
        .daily_since(&crate::days_ago(days))?
        .into_iter()
        .map(RecoveryDay::from)
        .collect())
}

/* --------------------------------------------------------------- nutrition --- */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NutritionDay {
    pub date: String,
    pub consumed_kcal: Option<f64>,
    pub total_burn_kcal: Option<f64>,
    pub active_kcal: Option<f64>,
    pub bmr_kcal: Option<f64>,
    pub net_calorie_goal: Option<f64>,
    /// Eaten minus burned; negative is a deficit.
    pub balance_kcal: Option<f64>,
    pub hydration_ml: Option<f64>,
    pub hydration_goal_ml: Option<f64>,
    pub sweat_loss_ml: Option<f64>,
    /// Whether a food log existed at all that day.
    pub logged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NutritionReport {
    pub days: Vec<NutritionDay>,
    /// How many of the returned days carry a food log.
    pub days_logged: usize,
    /// Averages over logged days only — averaging an unlogged day as zero
    /// would invent a starvation week out of a missing integration.
    pub avg_consumed_kcal: Option<f64>,
    pub avg_burn_kcal: Option<f64>,
    pub avg_balance_kcal: Option<f64>,
}

pub fn nutrition(db: &Db, days: u32) -> Result<NutritionReport> {
    let rows: Vec<NutritionDay> = db
        .daily_since(&crate::days_ago(days))?
        .into_iter()
        .map(|d| NutritionDay {
            balance_kcal: d.calorie_balance().map(round1),
            logged: d.consumed_kcal.is_some(),
            date: d.date,
            consumed_kcal: d.consumed_kcal,
            total_burn_kcal: d.total_burn_kcal,
            active_kcal: d.active_kcal,
            bmr_kcal: d.bmr_kcal,
            net_calorie_goal: d.net_calorie_goal,
            hydration_ml: d.hydration_ml,
            hydration_goal_ml: d.hydration_goal_ml,
            sweat_loss_ml: d.sweat_loss_ml,
        })
        .collect();

    let logged: Vec<&NutritionDay> = rows.iter().filter(|d| d.logged).collect();
    let mean = |vals: Vec<f64>| {
        (!vals.is_empty()).then(|| round1(vals.iter().sum::<f64>() / vals.len() as f64))
    };

    Ok(NutritionReport {
        days_logged: logged.len(),
        avg_consumed_kcal: mean(logged.iter().filter_map(|d| d.consumed_kcal).collect()),
        avg_burn_kcal: mean(logged.iter().filter_map(|d| d.total_burn_kcal).collect()),
        avg_balance_kcal: mean(logged.iter().filter_map(|d| d.balance_kcal).collect()),
        days: rows,
    })
}

/* ------------------------------------------------------------------ weight --- */

/// Kilocalories per kilogram of body mass. The textbook 7,700 — an
/// approximation that assumes the tissue lost is mostly fat, which is why
/// nothing here presents a number derived from it as a measurement.
const KCAL_PER_KG: f64 = 7700.0;

/// Half-life of the trend line, in days.
///
/// Ten days is the usual choice for scale smoothing: long enough to flatten a
/// salty meal or a bad night, short enough that a real change shows within a
/// fortnight rather than a month.
const TREND_HALF_LIFE_DAYS: f64 = 10.0;

/// The smallest jump that can count as a mis-entry, in kilograms.
///
/// Below this it's plausible biology — a big meal and a dehydrated morning are
/// two kilos apart on the same body. Above it, and only when the readings
/// either side agree with each other, it's a typo.
const SPIKE_FLOOR_KG: f64 = 3.0;

/// How much genuine day-to-day movement to allow before calling a point a
/// spike, per day of gap. Real weight can move fast over a fortnight, so the
/// threshold has to widen with the gap or every post-holiday weigh-in gets
/// flagged.
const SPIKE_PER_DAY_KG: f64 = 0.2;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeightPoint {
    pub date: String,
    pub kg: f64,
    /// The smoothed trend at this point — what the scale is "really" saying,
    /// with the day-to-day water noise taken out.
    pub trend_kg: Option<f64>,
    /// A reading that disagrees with both its neighbours by more than a body
    /// can move. Kept and shown, never silently dropped: it's the athlete's
    /// data, and a wrong entry is worth seeing so it can be corrected in
    /// Garmin. Excluded from the trend, the rate and the averages.
    pub outlier: bool,
    /// `MFP`, `MANUAL`, `USER_SETTING`, or a scale's name.
    pub source: Option<String>,
}

/// How the calories logged compare with what the scale did over the same span.
///
/// The two disagreeing is the normal case, not an error — the point of showing
/// them together is that the size and direction of the disagreement is
/// informative, and that a thin food log explains most of it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnergyCheck {
    /// Days between the first and last weigh-in used.
    pub span_days: i64,
    /// Of those, how many carry a food log. Everything below is computed from
    /// these days alone — never from the span, which would treat an unlogged
    /// day as a day of perfect maintenance.
    pub logged_days: usize,
    /// `logged_days` as a share of the span. The number that decides how much
    /// of this section to believe.
    pub coverage_pct: f64,
    /// Sum of (eaten − burned) across the logged days.
    pub balance_kcal: f64,
    /// That balance converted to kilograms. Deliberately not scaled up to cover
    /// the unlogged days: inventing the missing four fifths of a month would
    /// make this a prediction about data that doesn't exist.
    pub predicted_change_kg: f64,
    /// What the trend line actually did across the same span.
    pub actual_change_kg: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeightGoal {
    pub target_kg: f64,
    /// Signed: negative means there is weight to lose.
    pub delta_kg: f64,
    /// Projected arrival, from the current rate. `None` when the rate is flat
    /// or pointing away from the target — an honest "not on this trajectory"
    /// rather than a date centuries out.
    pub eta_date: Option<String>,
    pub eta_days: Option<i64>,
}

/// The same series, cut to a recent slice.
///
/// The report's own window is half a year, which is the right span for a chart
/// and the wrong one for a sentence: "you're down 2 kg since February" is true
/// on the day you gained a kilo. These are what a reader actually wants first —
/// the last week and the last month — and they carry their own `count` so a
/// window with one weigh-in in it can be described as the thin thing it is
/// rather than as a trend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeightWindow {
    /// Days the window covers, counting back from today.
    pub days: u32,
    /// Clean weigh-ins inside it. Outliers are excluded here, unlike the
    /// report's own `count` — a window of two where one is a mis-entry has one
    /// reading, and saying two would invite a change to be read off it.
    pub count: usize,
    /// The trend line at each end of the window, and the distance between them.
    /// All three are `None` below two clean readings, because a single point
    /// has no direction and a window is not a trend.
    pub trend_start_kg: Option<f64>,
    pub trend_end_kg: Option<f64>,
    pub change_kg: Option<f64>,
    /// Lightest and heaviest clean readings inside the window. Present from one
    /// reading, since neither claims a direction.
    pub low_kg: Option<f64>,
    pub high_kg: Option<f64>,
}

/// The recent slices, newest span first.
///
/// Computed from `points`, which are already thinned to one per day and carry
/// their trend, so this is a filter rather than a second pass over the rows.
fn windows(points: &[WeightPoint], today: chrono::NaiveDate) -> Vec<WeightWindow> {
    [7_u32, 30]
        .into_iter()
        .map(|days| {
            let from = day_number(
                &(today - chrono::Duration::days(days.saturating_sub(1) as i64))
                    .format("%Y-%m-%d")
                    .to_string(),
            );
            let inside: Vec<&WeightPoint> = points
                .iter()
                .filter(|p| !p.outlier && day_number(&p.date) >= from)
                .collect();

            let kgs: Vec<f64> = inside.iter().map(|p| p.kg).collect();
            let start = inside.first().and_then(|p| p.trend_kg);
            let end = inside.last().and_then(|p| p.trend_kg);
            WeightWindow {
                days,
                count: inside.len(),
                trend_start_kg: start.filter(|_| inside.len() >= 2),
                trend_end_kg: end.filter(|_| inside.len() >= 2),
                change_kg: match (start, end) {
                    (Some(a), Some(b)) if inside.len() >= 2 => Some(round1(b - a)),
                    _ => None,
                },
                low_kg: kgs
                    .iter()
                    .cloned()
                    .fold(None, |m: Option<f64>, k| Some(m.map_or(k, |m| m.min(k)))),
                high_kg: kgs
                    .iter()
                    .cloned()
                    .fold(None, |m: Option<f64>, k| Some(m.map_or(k, |m| m.max(k)))),
            }
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeightReport {
    /// Oldest first, for drawing.
    pub points: Vec<WeightPoint>,
    /// The last week and the last month, in that order. The report's own
    /// figures describe the whole window and answer a different question.
    pub windows: Vec<WeightWindow>,
    /// Weigh-ins in the window, outliers included.
    pub count: usize,
    /// The most recent weigh-in on the account, even if it predates the window
    /// — so a screen can say "you last weighed in in March" rather than going
    /// blank because the last 90 days happen to be empty.
    pub latest_kg: Option<f64>,
    pub latest_date: Option<String>,
    pub days_since_latest: Option<i64>,
    /// Current value of the trend line, which is the number to quote as "your
    /// weight" — a single reading is noise around this.
    pub trend_kg: Option<f64>,
    /// Change in the trend line across the window.
    pub change_kg: Option<f64>,
    /// Least-squares slope over the window's clean readings. `None` until there
    /// are enough points spread over enough days to mean anything.
    pub rate_kg_per_week: Option<f64>,
    /// Days between the first and last weigh-in in the window.
    pub span_days: Option<i64>,
    pub bmi: Option<f64>,
    pub height_cm: Option<f64>,
    /// Whether any reading carries body composition. False on every account
    /// without a smart scale, which is what hides that section entirely.
    pub has_body_composition: bool,
    pub energy: Option<EnergyCheck>,
    pub goal: Option<WeightGoal>,
}

/// One weigh-in per day, outliers marked, trend attached.
///
/// Two entries on the same day means one corrected the other, so the later
/// `sample_pk` wins rather than the two being averaged into a weight the
/// athlete never saw.
fn daily_points(rows: Vec<crate::db::WeighIn>) -> Vec<WeightPoint> {
    let mut kept: Vec<crate::db::WeighIn> = Vec::new();
    for r in rows {
        match kept.last_mut() {
            Some(prev) if prev.calendar_date == r.calendar_date => *prev = r,
            _ => kept.push(r),
        }
    }

    let days: Vec<i64> = kept.iter().map(|r| day_number(&r.calendar_date)).collect();
    let kgs: Vec<f64> = kept.iter().map(|r| r.weight_g / 1000.0).collect();

    // A reading is a spike only if *both* neighbours disagree with it in the
    // same direction. A step that the next reading confirms is a real change,
    // however abrupt — that's a body, not a typo.
    let outliers: Vec<bool> = (0..kept.len())
        .map(|i| {
            let (Some(&prev), Some(&next)) = (kgs.get(i.wrapping_sub(1)), kgs.get(i + 1)) else {
                return false;
            };
            let gap = (days[i + 1] - days[i - 1]).max(1) as f64;
            let allow = SPIKE_FLOOR_KG.max(SPIKE_PER_DAY_KG * gap);
            let (dp, dn) = (kgs[i] - prev, kgs[i] - next);
            dp.signum() == dn.signum() && dp.abs() > allow && dn.abs() > allow
        })
        .collect();

    // Time-aware exponential smoothing: the weight given to history decays with
    // the *days* since the last reading, not with how many readings ago it was.
    // A plain moving average would treat a gap of one day and a gap of three
    // months as the same step and drag a stale figure across the gap.
    let mut trend: Option<(f64, i64)> = None;
    let mut out = Vec::with_capacity(kept.len());
    for (i, r) in kept.into_iter().enumerate() {
        let kg = kgs[i];
        let trend_kg = if outliers[i] {
            // Not fed in, but the line is still drawn through this x so the
            // chart doesn't break where a bad reading sits.
            trend.map(|(v, _)| round1(v))
        } else {
            let next = match trend {
                None => (kg, days[i]),
                Some((prev, prev_day)) => {
                    let elapsed = (days[i] - prev_day).max(0) as f64;
                    let alpha = 1.0 - 0.5_f64.powf(elapsed / TREND_HALF_LIFE_DAYS);
                    (prev + alpha * (kg - prev), days[i])
                }
            };
            trend = Some(next);
            Some(round1(next.0))
        };

        out.push(WeightPoint {
            date: r.calendar_date,
            kg: round1(kg),
            trend_kg,
            outlier: outliers[i],
            source: r.source_type,
        });
    }
    out
}

/// Days since an arbitrary fixed epoch, for spacing arithmetic. Only
/// differences between two of these are ever used, so the epoch is immaterial.
/// Parsing failure yields 0, which only happens if the cache holds a malformed
/// date.
fn day_number(date: &str) -> i64 {
    let epoch = chrono::NaiveDate::from_ymd_opt(2000, 1, 1).expect("2000-01-01 is a date");
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map(|d| (d - epoch).num_days())
        .unwrap_or(0)
}

/// Least-squares slope in kg/day over the clean readings.
///
/// Regression rather than "first to last divided by the days": weigh-ins are
/// irregular, and two endpoints that happen to be a heavy morning and a light
/// one produce a rate that no amount of data in between can correct.
fn slope_kg_per_day(points: &[WeightPoint]) -> Option<f64> {
    let clean: Vec<(f64, f64)> = points
        .iter()
        .filter(|p| !p.outlier)
        .map(|p| (day_number(&p.date) as f64, p.kg))
        .collect();

    // Three readings over a fortnight is the floor for saying anything about a
    // direction. Below that the slope is a line through noise.
    if clean.len() < 3 {
        return None;
    }
    let span = clean.last()?.0 - clean.first()?.0;
    if span < 14.0 {
        return None;
    }

    let n = clean.len() as f64;
    let mean_x = clean.iter().map(|p| p.0).sum::<f64>() / n;
    let mean_y = clean.iter().map(|p| p.1).sum::<f64>() / n;
    let num: f64 = clean.iter().map(|p| (p.0 - mean_x) * (p.1 - mean_y)).sum();
    let den: f64 = clean.iter().map(|p| (p.0 - mean_x).powi(2)).sum();
    (den > 0.0).then(|| num / den)
}

/// Weight, its trend, and what the food log says should have happened.
pub fn weight(db: &Db, days: u32) -> Result<WeightReport> {
    let today = chrono::Utc::now().date_naive();
    let from = today - chrono::Duration::days(days.saturating_sub(1) as i64);
    let rows = db.weigh_ins_since(&from.format("%Y-%m-%d").to_string())?;

    // Asked of the raw rows, before they're thinned to one per day: whether a
    // smart scale ever wrote to this account is a property of the data, and one
    // entry with a body-fat figure is enough to make the section worth showing.
    let has_body_composition = rows
        .iter()
        .any(|r| r.body_fat.is_some() || r.muscle_mass.is_some() || r.body_water.is_some());

    let points = daily_points(rows);

    let latest = db.latest_weigh_in()?;
    let latest_kg = latest.as_ref().map(|w| round1(w.weight_g / 1000.0));
    let days_since_latest = latest
        .as_ref()
        .and_then(|w| chrono::NaiveDate::parse_from_str(&w.calendar_date, "%Y-%m-%d").ok())
        .map(|d| (today - d).num_days());

    let clean: Vec<&WeightPoint> = points.iter().filter(|p| !p.outlier).collect();
    let trend_kg = points.iter().rev().find_map(|p| p.trend_kg);
    let first_trend = points.iter().find_map(|p| p.trend_kg);
    let change_kg = match (first_trend, trend_kg) {
        (Some(a), Some(b)) if clean.len() >= 2 => Some(round1(b - a)),
        _ => None,
    };

    let span_days = match (clean.first(), clean.last()) {
        (Some(a), Some(b)) => Some(day_number(&b.date) - day_number(&a.date)),
        _ => None,
    };

    let rate = slope_kg_per_day(&points);
    let height_cm = db
        .sync_state("height_cm")?
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|c| *c > 50.0);
    let bmi = match (trend_kg.or(latest_kg), height_cm) {
        (Some(kg), Some(cm)) => Some(round1(kg / (cm / 100.0).powi(2))),
        _ => None,
    };

    Ok(WeightReport {
        count: points.len(),
        has_body_composition,
        energy: energy_check(db, &points, span_days)?,
        goal: goal_check(db, trend_kg.or(latest_kg), rate, today)?,
        rate_kg_per_week: rate.map(|r| round2(r * 7.0)),
        span_days,
        trend_kg,
        change_kg,
        latest_kg,
        latest_date: latest.map(|w| w.calendar_date),
        days_since_latest,
        bmi,
        height_cm,
        windows: windows(&points, today),
        points,
    })
}

/// The calorie log against the scale, over the span the weigh-ins cover.
fn energy_check(
    db: &Db,
    points: &[WeightPoint],
    span_days: Option<i64>,
) -> Result<Option<EnergyCheck>> {
    let (Some(span), Some(first), Some(last)) = (span_days, points.first(), points.last()) else {
        return Ok(None);
    };
    if span < 7 {
        return Ok(None);
    }

    // The span the weigh-ins actually cover, which is what the calorie balance
    // has to be summed over — not a window ending today. The lower bound goes
    // to SQL and the upper is applied here.
    let daily = db.daily_since(&first.date)?;
    let logged: Vec<f64> = daily
        .iter()
        .filter(|d| d.date.as_str() <= last.date.as_str())
        .filter_map(|d| d.calorie_balance())
        .collect();

    if logged.is_empty() {
        return Ok(None);
    }

    let balance: f64 = logged.iter().sum();
    let actual = match (first.trend_kg, last.trend_kg) {
        (Some(a), Some(b)) => Some(round1(b - a)),
        _ => None,
    };

    Ok(Some(EnergyCheck {
        span_days: span,
        logged_days: logged.len(),
        coverage_pct: round1(logged.len() as f64 / (span as f64 + 1.0) * 100.0),
        balance_kcal: balance.round(),
        predicted_change_kg: round2(balance / KCAL_PER_KG),
        actual_change_kg: actual,
    }))
}

/// The locally-set goal, if there is one, and when the current rate reaches it.
///
/// Garmin exposes no weight goal on this account — `/weight-service/weight/goal`
/// is not a readable endpoint and the goal service returns nothing — so this one
/// is the app's own, and the UI says so. Weigh-ins themselves always come from
/// Garmin; only the target is local.
fn goal_check(
    db: &Db,
    current: Option<f64>,
    rate_per_day: Option<f64>,
    today: chrono::NaiveDate,
) -> Result<Option<WeightGoal>> {
    let Some(target) = db
        .sync_state("weight_goal_kg")?
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|k| *k > 20.0 && *k < 500.0)
    else {
        return Ok(None);
    };
    let Some(current) = current else {
        return Ok(Some(WeightGoal {
            target_kg: target,
            delta_kg: 0.0,
            eta_date: None,
            eta_days: None,
        }));
    };

    let delta = round1(target - current);
    // Only project when the trend actually points at the target. A rate of
    // roughly zero, or one heading the other way, has no arrival date — saying
    // so is more use than a date in the next century.
    let eta_days = rate_per_day
        .filter(|r| r.abs() > 0.001 && r.signum() == delta.signum())
        .map(|r| (delta / r).ceil() as i64)
        .filter(|d| *d > 0 && *d < 3650);

    Ok(Some(WeightGoal {
        target_kg: target,
        delta_kg: delta,
        eta_date: eta_days.map(|d| {
            (today + chrono::Duration::days(d))
                .format("%Y-%m-%d")
                .to_string()
        }),
        eta_days,
    }))
}

/* ------------------------------------------------------------------ routes --- */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteOuting {
    pub activity_id: i64,
    pub name: Option<String>,
    pub local_date: Option<String>,
    pub distance_m: Option<f64>,
    pub duration_s: Option<f64>,
    pub points: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    /// Borrowed from the most recent outing on this route.
    pub name: Option<String>,
    pub type_key: Option<String>,
    pub times: usize,
    pub avg_distance_m: Option<f64>,
    pub outings: Vec<RouteOuting>,
}

/// Metres per degree of latitude, near enough at any latitude. Longitude is
/// scaled by cos(lat) at the comparison point.
const M_PER_DEG: f64 = 111_320.0;

fn metres_apart(a: (f64, f64), b: (f64, f64)) -> f64 {
    let dlat = (a.0 - b.0) * M_PER_DEG;
    let dlon = (a.1 - b.1) * M_PER_DEG * a.0.to_radians().cos();
    (dlat * dlat + dlon * dlon).sqrt()
}

/// Two outings count as the same route when they start and finish within
/// `RADIUS_M` of each other and cover a similar distance. That is deliberately
/// crude — it groups "the same loop from home" without pretending to be
/// trace-shape matching, which this downsampled data can't support.
const RADIUS_M: f64 = 250.0;
const DISTANCE_TOLERANCE: f64 = 0.25;

/// How many routes get their coordinates loaded.
///
/// Shipping every trace was handing the webview tens of thousands of points
/// and a few hundred inline SVG paths in one render, which took the web
/// process down on the way in. Only the first this-many routes in the
/// requested order get a trace; the rest still appear in the list and the
/// counts, just without a drawn thumbnail.
const TRACED_ROUTES: usize = 40;

/// How the routes list is ordered.
///
/// The screen offers this as a control, but the ordering is decided here
/// rather than in the webview: `TRACED_ROUTES` means the choice of order also
/// decides which routes get coordinates, so sorting after the fact would leave
/// the top of the list without traces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RouteSort {
    /// Most recent outing first.
    #[default]
    Recent,
    /// Most-repeated first, most recent among equals.
    Repeats,
    /// Longest average distance first.
    Distance,
}

/// Routes with their traces, for the screen that draws them.
pub fn routes(db: &Db, sort: RouteSort) -> Result<Vec<Route>> {
    let mut routes = route_groups(db, sort)?;

    // Coordinates are read only now, and only for the one outing per route the
    // screen actually draws. Fetching all of them up front was shipping tens of
    // thousands of unused points across the IPC boundary.
    for route in routes.iter_mut().take(TRACED_ROUTES) {
        route.outings[0].points = db.track_points(route.outings[0].activity_id)?;
    }

    Ok(routes)
}

/// The grouping on its own, with every `points` left empty.
fn route_groups(db: &Db, sort: RouteSort) -> Result<Vec<Route>> {
    // Headers only: grouping reads endpoints and distance, never coordinates.
    let tracks: Vec<_> = db
        .track_headers()?
        .into_iter()
        .filter(|t| t.point_count > 1)
        .collect();

    let mut groups: Vec<Vec<crate::db::ActivityTrack>> = Vec::new();

    'next: for t in tracks {
        let (Some(slat), Some(slon), Some(elat), Some(elon)) =
            (t.start_lat, t.start_lon, t.end_lat, t.end_lon)
        else {
            continue;
        };

        for g in groups.iter_mut() {
            let h = &g[0];
            let (Some(hs), Some(hsl), Some(he), Some(hel)) =
                (h.start_lat, h.start_lon, h.end_lat, h.end_lon)
            else {
                continue;
            };
            let near_start = metres_apart((slat, slon), (hs, hsl)) < RADIUS_M;
            let near_end = metres_apart((elat, elon), (he, hel)) < RADIUS_M;
            let similar = match (t.distance_m, h.distance_m) {
                (Some(a), Some(b)) if b > 0.0 => ((a - b) / b).abs() < DISTANCE_TOLERANCE,
                // Without distance on both sides, position alone decides.
                _ => true,
            };
            if near_start && near_end && similar {
                g.push(t);
                continue 'next;
            }
        }
        groups.push(vec![t]);
    }

    let mut routes: Vec<Route> = groups
        .into_iter()
        .map(|g| {
            let dists: Vec<f64> = g.iter().filter_map(|t| t.distance_m).collect();
            Route {
                name: g[0].name.clone(),
                type_key: g[0].type_key.clone(),
                times: g.len(),
                avg_distance_m: (!dists.is_empty())
                    .then(|| dists.iter().sum::<f64>() / dists.len() as f64),
                outings: g
                    .into_iter()
                    .map(|t| RouteOuting {
                        activity_id: t.activity_id,
                        name: t.name,
                        local_date: t.local_date,
                        distance_m: t.distance_m,
                        duration_s: t.duration_s,
                        points: t.points,
                    })
                    .collect(),
            }
        })
        .collect();

    // Outings arrive newest-first from the query, so `outings[0]` is the last
    // time this route was covered — the date every order below falls back to.
    let latest = |r: &Route| r.outings[0].local_date.clone();
    match sort {
        RouteSort::Recent => routes.sort_by_key(|r| std::cmp::Reverse(latest(r))),
        RouteSort::Repeats => routes.sort_by(|a, b| {
            b.times
                .cmp(&a.times)
                .then_with(|| latest(b).cmp(&latest(a)))
        }),
        RouteSort::Distance => routes.sort_by(|a, b| {
            // `None` sorts last either way: `Option` orders `None` below
            // `Some`, and this comparison is reversed.
            b.avg_distance_m
                .partial_cmp(&a.avg_distance_m)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| latest(b).cmp(&latest(a)))
        }),
    }
    Ok(routes)
}

/// A route without its trace. The screens draw the coordinates; a language
/// model has no use for four hundred of them, so the tool surfaces get this.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteSummary {
    pub name: Option<String>,
    pub sport: Option<String>,
    pub times: usize,
    pub avg_distance_km: Option<f64>,
    /// One entry per outing, newest first.
    pub dates: Vec<String>,
}

pub fn route_summaries(db: &Db) -> Result<Vec<RouteSummary>> {
    // The point-free form: a model has no use for coordinates.
    // Most-repeated first for the model: a route covered nine times is the one
    // worth talking about, and there is no scroll position to preserve here.
    Ok(route_groups(db, RouteSort::Repeats)?
        .into_iter()
        .map(|r| RouteSummary {
            name: r.name,
            sport: r.type_key,
            times: r.times,
            avg_distance_km: r.avg_distance_m.map(|m| round2(m / 1000.0)),
            dates: r.outings.into_iter().filter_map(|o| o.local_date).collect(),
        })
        .collect())
}

/* ----------------------------------------------------------------- strength --- */

/// Recent strength sessions, each summarised from its sets.
///
/// Read the field docs on [`crate::StrengthSession`] before building anything on
/// this: the watch records reps, durations and order, and does not record load.
pub fn strength_sessions(db: &Db, limit: u32) -> Result<Vec<crate::StrengthSession>> {
    let mut out = Vec::new();
    for a in db.strength_activities(limit)? {
        let sets = db.exercise_sets(a.activity_id)?;
        out.push(session_of(&a, &sets));
    }
    Ok(out)
}

/// One session, with its sets in order. `None` when the activity has no cached
/// sets — which includes every strength session recorded before this feature
/// existed, until the next sync fetches them.
pub fn strength_session(
    db: &Db,
    activity_id: i64,
) -> Result<Option<(crate::StrengthSession, Vec<crate::ExerciseSet>)>> {
    let Some(a) = db.activity(activity_id)? else {
        return Ok(None);
    };
    let sets = db.exercise_sets(activity_id)?;
    if sets.is_empty() {
        return Ok(None);
    }
    Ok(Some((session_of(&a, &sets), sets)))
}

/// Attach the activity's own summary fields to a set-derived summary.
fn session_of(a: &CachedActivity, sets: &[crate::ExerciseSet]) -> crate::StrengthSession {
    crate::StrengthSession {
        activity_id: a.activity_id,
        name: a.name.clone(),
        date: a.local_date.clone(),
        duration_min: a.duration_s.map(|s| round1(s / 60.0)),
        avg_hr: a.avg_hr,
        max_hr: a.max_hr,
        calories: a.calories,
        ..crate::strength::summarise(sets)
    }
}

/// How a run of strength sessions compares, oldest last.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrengthReport {
    pub sessions: Vec<crate::StrengthSession>,
    pub sessions_examined: usize,
    pub avg_work_sets: Option<f64>,
    pub avg_reps: Option<f64>,
    /// Median of the per-session median rests. The number that says whether the
    /// sessions are paced as strength work or as a circuit.
    pub median_rest_s: Option<f64>,
    /// How many work sets across the window carried no usable exercise guess.
    /// Shown so the exercise breakdown is read with the right scepticism.
    pub unlabelled_sets: usize,
    pub labelled_sets: usize,
    /// True when no session in the window carried a load figure — which is the
    /// expected state, and the reason there is no volume anywhere here.
    pub no_weights_recorded: bool,
}

pub fn strength_trend(db: &Db, limit: u32) -> Result<StrengthReport> {
    let sessions = strength_sessions(db, limit)?;
    let n = sessions.len();

    let mean =
        |xs: Vec<f64>| (!xs.is_empty()).then(|| round1(xs.iter().sum::<f64>() / xs.len() as f64));

    let mut rests: Vec<f64> = sessions.iter().filter_map(|s| s.median_rest_s).collect();
    rests.sort_by(f64::total_cmp);

    let labelled: usize = sessions
        .iter()
        .map(|s| s.guessed_exercises.iter().map(|e| e.sets).sum::<usize>())
        .sum();

    Ok(StrengthReport {
        avg_work_sets: mean(sessions.iter().map(|s| s.work_sets as f64).collect()),
        avg_reps: mean(sessions.iter().map(|s| s.total_reps as f64).collect()),
        median_rest_s: (!rests.is_empty()).then(|| round1(rests[rests.len() / 2])),
        unlabelled_sets: sessions.iter().map(|s| s.unlabelled_sets).sum(),
        labelled_sets: labelled,
        no_weights_recorded: true,
        sessions_examined: n,
        sessions,
    })
}

/* ------------------------------------------------------------------ records --- */

/// Every personal record, best-known first.
///
/// Records whose `type_id` this build doesn't recognise are kept but carry no
/// label — a caller showing these to a human should skip them rather than
/// print a number with no idea what it measures.
pub fn personal_records(db: &Db) -> Result<Vec<crate::PersonalRecord>> {
    db.personal_records()
}

/* ------------------------------------------------------------------ fitness --- */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FitnessReport {
    /// Garmin's most recent verdict, or `None` if it has never been synced.
    pub latest: Option<crate::db::FitnessDay>,
    /// The window, newest first, for drawing the acute/chronic curve.
    pub days: Vec<crate::db::FitnessDay>,
    /// True when the account has no VO2 max — which for a treadmill-only runner
    /// is expected, not a fault, and is the cue to suggest an outdoor GPS run.
    pub vo2max_missing: bool,
    /// Set when the month's anaerobic load has gone past the top of Garmin's
    /// own target range.
    pub anaerobic_over_target: bool,
}

pub fn fitness(db: &Db, days: u32) -> Result<FitnessReport> {
    let rows = db.fitness_since(&crate::days_ago(days))?;
    let latest = rows.first().cloned();
    Ok(FitnessReport {
        vo2max_missing: latest.as_ref().is_none_or(|d| d.status.vo2max.is_none()),
        anaerobic_over_target: latest
            .as_ref()
            .is_some_and(|d| d.status.anaerobic_over_target()),
        latest,
        days: rows,
    })
}

/* -------------------------------------------------------------------- sleep --- */

/// Last night in full, the window behind it, and what the two say.
///
/// A thin pass-through to [`crate::sleep::report`], here so that every screen
/// and every tool reaches analysis the same way — through `query` — rather than
/// some of them knowing which module a thing happens to live in.
pub fn sleep(db: &Db, days: u32) -> Result<crate::sleep::SleepReport> {
    crate::sleep::report(db, days)
}

/// Days without a wellness row past which the cache is called stale.
///
/// A worn watch writes one every day, so a gap this size is the watch being off
/// the wrist rather than a quiet week. Activities are deliberately not part of
/// this test: not training for three days is a rest block, not a broken sync.
const STALE_DAYS: i64 = 3;

pub fn cache_status(db: &Db) -> Result<CacheStatus> {
    let today = crate::today();
    let newest_activity_date = db.newest_activity_date()?;
    let newest_daily_date = db.newest_daily_date()?;

    let age = |d: &Option<String>| {
        d.as_deref()
            .and_then(|s| crate::days_between(s, today))
            .map(|n| n.max(0))
    };
    let days_since_daily = age(&newest_daily_date);

    Ok(CacheStatus {
        activities_cached: db.activity_count()?,
        last_sync: db.sync_state("last_sync")?,
        database_path: crate::db::default_path().map(|p| p.display().to_string()),
        connected_to_garmin: crate::store::load_tokens()?.is_some(),
        days_since_activity: age(&newest_activity_date),
        stale: days_since_daily.is_none_or(|n| n > STALE_DAYS),
        newest_activity_date,
        newest_daily_date,
        days_since_daily,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::WeighIn;

    fn w(pk: i64, date: &str, kg: f64) -> WeighIn {
        WeighIn {
            sample_pk: pk,
            calendar_date: date.into(),
            weight_g: kg * 1000.0,
            bmi: None,
            body_fat: None,
            body_water: None,
            bone_mass: None,
            muscle_mass: None,
            source_type: Some("MFP".into()),
        }
    }

    /// The real shape this account's cache holds: a 97.0 typed in the day after
    /// an 86.6, with the run of readings either side agreeing with each other.
    #[test]
    fn a_lone_impossible_reading_is_flagged_but_kept() {
        let pts = daily_points(vec![
            w(1, "2026-01-18", 87.6),
            w(2, "2026-01-22", 86.6),
            w(3, "2026-01-23", 97.0),
            w(4, "2026-02-03", 86.3),
            w(5, "2026-02-04", 85.9),
        ]);

        assert_eq!(pts.len(), 5, "the bad reading is kept, not dropped");
        let flagged: Vec<&str> = pts
            .iter()
            .filter(|p| p.outlier)
            .map(|p| p.date.as_str())
            .collect();
        assert_eq!(flagged, ["2026-01-23"]);
    }

    fn on(date: &str) -> chrono::NaiveDate {
        chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap()
    }

    /// This account's actual August: two weigh-ins two days apart, then nothing
    /// back to June. The half-year figures are dominated by a spring the reader
    /// isn't asking about, so the windows have to carry the recent slice
    /// separately — and say how thin it is.
    #[test]
    fn the_recent_windows_describe_now_rather_than_the_whole_span() {
        let pts = daily_points(vec![
            w(1, "2026-03-29", 86.0),
            w(2, "2026-06-28", 88.0),
            w(3, "2026-08-04", 86.0),
            w(4, "2026-08-06", 85.5),
        ]);
        let ws = windows(&pts, on("2026-08-09"));

        let week = &ws[0];
        assert_eq!(week.days, 7);
        assert_eq!(week.count, 2, "August 4th and 6th are inside the week");
        assert_eq!(week.low_kg, Some(85.5));
        assert_eq!(week.high_kg, Some(86.0));

        let month = &ws[1];
        assert_eq!(month.days, 30);
        assert_eq!(month.count, 2, "June 28th is 42 days back and outside it");
        assert!(
            month.change_kg.is_some(),
            "two readings in the month is a direction, however short"
        );
    }

    /// A window with one reading in it has no direction, and must not invent
    /// one — a paragraph that reads a change off a single point is the failure
    /// this field exists to prevent.
    #[test]
    fn a_single_reading_in_a_window_is_not_a_trend() {
        let pts = daily_points(vec![w(1, "2026-07-01", 87.0), w(2, "2026-08-08", 86.0)]);
        let ws = windows(&pts, on("2026-08-09"));

        let week = &ws[0];
        assert_eq!(week.count, 1);
        assert_eq!(week.change_kg, None);
        assert_eq!(week.trend_start_kg, None);
        assert_eq!(week.trend_end_kg, None);
        assert_eq!(
            week.low_kg,
            Some(86.0),
            "the reading itself is still worth reporting"
        );
    }

    /// Outliers are already excluded from the trend; they have to be excluded
    /// from the window counts too, or a month holding one real weigh-in and one
    /// mis-entry reads as two and invites a change to be drawn between them.
    #[test]
    fn a_mis_entry_does_not_pad_a_window_count() {
        let pts = daily_points(vec![
            w(1, "2026-07-20", 86.0),
            w(2, "2026-07-28", 86.2),
            w(3, "2026-07-29", 97.0),
            w(4, "2026-07-30", 86.1),
        ]);
        let ws = windows(&pts, on("2026-08-09"));

        assert!(pts.iter().any(|p| p.outlier), "the 97.0 is flagged");
        assert_eq!(
            ws[1].count, 3,
            "four readings in the month, one of them a mis-entry, leaves three"
        );
        assert!(
            ws[1].high_kg.unwrap() < 90.0,
            "and the mis-entry is not the heaviest of them"
        );
    }

    /// A step that the following reading confirms is a body, not a typo — the
    /// spike rule needs both neighbours to disagree, in the same direction.
    #[test]
    fn a_confirmed_step_is_not_an_outlier() {
        let pts = daily_points(vec![
            w(1, "2026-01-01", 92.0),
            w(2, "2026-02-01", 86.0),
            w(3, "2026-03-01", 85.5),
        ]);
        assert!(pts.iter().all(|p| !p.outlier));
    }

    /// Two entries on one day means the second corrected the first.
    #[test]
    fn a_reweighed_day_keeps_only_the_correction() {
        let pts = daily_points(vec![
            w(1, "2026-03-01", 96.0),
            w(2, "2026-03-01", 86.0),
            w(3, "2026-03-08", 85.8),
        ]);
        assert_eq!(pts.len(), 2);
        assert_eq!(pts[0].kg, 86.0);
    }

    /// The trend must not be dragged by a reading the spike rule rejected.
    #[test]
    fn the_trend_ignores_outliers() {
        let pts = daily_points(vec![
            w(1, "2026-01-01", 86.0),
            w(2, "2026-01-08", 86.0),
            w(3, "2026-01-15", 97.0),
            w(4, "2026-01-22", 86.0),
        ]);
        let trend = pts.last().unwrap().trend_kg.unwrap();
        assert!(
            (trend - 86.0).abs() < 0.2,
            "an ignored 97 still moved the trend to {trend}"
        );
    }

    /// Irregular spacing is the normal case, so the rate has to come from the
    /// dates rather than from the reading count: one kilo a month is
    /// ~0.23 kg/week however unevenly the weigh-ins fall.
    #[test]
    fn the_rate_is_measured_against_dates_not_reading_count() {
        let pts = daily_points(vec![
            w(1, "2026-01-01", 90.0),
            w(2, "2026-01-03", 89.93),
            w(3, "2026-01-05", 89.87),
            w(4, "2026-04-01", 87.0),
        ]);
        let per_week = slope_kg_per_day(&pts).unwrap() * 7.0;
        assert!(
            (per_week + 0.23).abs() < 0.05,
            "expected about -0.23 kg/week, got {per_week}"
        );
    }

    /// Two readings a week apart say nothing about a direction, and the app
    /// must not pretend otherwise.
    #[test]
    fn too_little_history_has_no_rate() {
        let pts = daily_points(vec![w(1, "2026-01-01", 90.0), w(2, "2026-01-06", 89.0)]);
        assert_eq!(slope_kg_per_day(&pts), None);
    }
}
