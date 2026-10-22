//! Pulling Garmin data into the local cache.
//!
//! Sync is incremental and idempotent: activities upsert by id, daily metrics
//! upsert by date. Re-running it is always safe.

use anyhow::{bail, Context, Result};
use chrono::{Duration, NaiveDate, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::client::GarminClient;
use crate::db::{ActivityTrack, DailyMetrics, Db, WeighIn, Workout};

/// How many consecutive already-cached activities we tolerate before deciding
/// we've caught up. Not zero, because Garmin occasionally backfills an older
/// activity that would otherwise hide everything behind it.
const CATCH_UP_STREAK: usize = 10;

const PAGE: u32 = 50;

/// A step of a sync, as it happens.
///
/// A first sync of a watch that's been worn for a year is minutes of silence
/// otherwise — one request per day per endpoint, and nothing on screen to say
/// whether it's working or wedged. `label` is written to be shown verbatim.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncProgress {
    /// Which stage: `activities`, `wellness`, `workouts`, `tracks`, `extras`, `done`.
    pub phase: &'static str,
    /// What that stage is doing right now, e.g. `2025-11-04`.
    pub detail: String,
    /// Steps finished in this stage.
    pub done: u32,
    /// Steps expected, where the stage knows in advance. Activities don't.
    pub total: Option<u32>,
}

/// Somewhere to send progress. `Sync` so the futures below stay `Send`.
pub type Progress<'a> = &'a (dyn Fn(SyncProgress) + Send + Sync);

/// For callers that don't care — every `_with` function takes a sink, and this
/// is the one that drops it on the floor.
pub fn ignore(_: SyncProgress) {}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub activities_seen: usize,
    pub activities_written: usize,
    pub days_written: usize,
    /// Nights of detailed sleep written. Counted apart from `days_written`
    /// because they land in their own table and a day can produce one without
    /// the other — a watch worn only overnight writes a night and almost no
    /// day, and one left charging writes the reverse.
    pub nights_written: usize,
    pub workouts_written: usize,
    pub tracks_written: usize,
    pub weigh_ins_written: usize,
    pub sets_written: usize,
    pub records_written: usize,
    pub fitness_days_written: usize,
    /// Non-fatal problems — one endpoint 404ing shouldn't abort the whole sync.
    pub warnings: Vec<String>,
}

/// Walk a JSON object by key path, e.g. `["hrvSummary", "weeklyAvg"]`.
fn dig<'v>(v: &'v Value, path: &[&str]) -> Option<&'v Value> {
    path.iter().try_fold(v, |acc, k| acc.get(k))
}

fn num(v: &Value, path: &[&str]) -> Option<f64> {
    dig(v, path)?.as_f64()
}

fn text(v: &Value, path: &[&str]) -> Option<String> {
    dig(v, path)?.as_str().map(str::to_owned)
}

/// Pull activities newer than what's cached.
///
/// `full` ignores the catch-up heuristic and walks all the way back, for the
/// first sync or after a cache reset.
pub async fn sync_activities(
    client: &GarminClient,
    db: &Db,
    full: bool,
    report: &mut SyncReport,
    on: Progress<'_>,
) -> Result<()> {
    let mut start = 0u32;
    let mut streak = 0usize;

    loop {
        let page = client.activities(start, PAGE).await?;
        if page.is_empty() {
            break;
        }

        for a in &page {
            report.activities_seen += 1;
            let known = db.has_activity(a.activity_id)?;
            db.upsert_activity(a)?;
            if known {
                streak += 1;
            } else {
                report.activities_written += 1;
                streak = 0;
            }
        }

        // No total to report: the only way to learn how many activities there
        // are is to page to the end, which is the thing being reported on.
        on(SyncProgress {
            phase: "activities",
            detail: page
                .last()
                .and_then(|a| a.start_time_local.as_deref()?.get(..10).map(str::to_owned))
                .unwrap_or_default(),
            done: report.activities_seen as u32,
            total: None,
        });

        if !full && streak >= CATCH_UP_STREAK {
            break;
        }
        if page.len() < PAGE as usize {
            break;
        }
        start += PAGE;
    }

    Ok(())
}

/// Pull the wellness metrics for each of `dates`, newest first.
///
/// Each endpoint is fetched independently and failures are collected rather
/// than propagated — Garmin returns 404 for days with no data (device not worn,
/// no sleep recorded), which is normal and shouldn't fail the sync.
///
/// `stop_when_empty` only makes sense for a contiguous walk backwards; see
/// `EMPTY_DAY_STREAK`.
pub async fn sync_daily(
    client: &GarminClient,
    db: &Db,
    display_name: &str,
    dates: &[NaiveDate],
    stop_when_empty: bool,
    report: &mut SyncReport,
    on: Progress<'_>,
) -> Result<()> {
    let total = dates.len() as u32;
    let mut empty_streak = 0u32;
    let mut outage_streak = 0u32;

    for (i, date) in dates.iter().enumerate() {
        let offset = i as u32;
        let date_str = date.format("%Y-%m-%d").to_string();
        let mut metrics = DailyMetrics {
            date: date_str.clone(),
            ..Default::default()
        };
        let mut had_5xx = false;

        // Announced before the five requests rather than after, so the date on
        // screen is the one currently being waited on.
        on(SyncProgress {
            phase: "wellness",
            detail: date_str.clone(),
            done: offset,
            total: Some(total),
        });

        match client.user_summary(display_name, &date_str).await {
            Ok(v) => {
                metrics.resting_hr = num(&v, &["restingHeartRate"]);
                metrics.steps = num(&v, &["totalSteps"]).map(|n| n as i64);
                metrics.stress_avg = num(&v, &["averageStressLevel"]);
                metrics.body_battery_high = num(&v, &["bodyBatteryHighestValue"]);
                metrics.body_battery_low = num(&v, &["bodyBatteryLowestValue"]);
                // Nutrition rides along on this same response. `consumed` is
                // absent on days with no food logged, which stays None rather
                // than becoming a zero-calorie day.
                metrics.consumed_kcal = num(&v, &["consumedKilocalories"]);
                metrics.total_burn_kcal = num(&v, &["totalKilocalories"]);
                metrics.active_kcal = num(&v, &["activeKilocalories"]);
                metrics.bmr_kcal = num(&v, &["bmrKilocalories"]);
                metrics.net_calorie_goal = num(&v, &["netCalorieGoal"]);
            }
            Err(e) => warn_or_abort(report, "user summary", &date_str, e, &mut had_5xx)?,
        }

        match client.hrv(&date_str).await {
            Ok(v) => {
                metrics.hrv_last_night = num(&v, &["hrvSummary", "lastNightAvg"]);
                metrics.hrv_weekly_avg = num(&v, &["hrvSummary", "weeklyAvg"]);
                metrics.hrv_status = text(&v, &["hrvSummary", "status"]);
                metrics.hrv_low_upper = num(&v, &["hrvSummary", "baseline", "lowUpper"]);
                metrics.hrv_balanced_low = num(&v, &["hrvSummary", "baseline", "balancedLow"]);
                metrics.hrv_balanced_upper = num(&v, &["hrvSummary", "baseline", "balancedUpper"]);
                // The same response holds the night's readings. Only the recent
                // days keep them, so the full history import stays as it was.
                if is_recent(*date) {
                    let readings = crate::intraday::parse_hrv_readings(&v);
                    if let Err(e) = db.save_hrv_readings(&date_str, &readings) {
                        report
                            .warnings
                            .push(format!("hrv readings {date_str}: {e}"));
                    }
                }
            }
            Err(e) => warn_or_abort(report, "hrv", &date_str, e, &mut had_5xx)?,
        }

        match client.training_readiness(&date_str).await {
            Ok(v) => {
                // This endpoint returns an array with at most one entry.
                let first = v.as_array().and_then(|a| a.first()).unwrap_or(&v);
                metrics.training_readiness = num(first, &["score"]);
            }
            Err(e) => warn_or_abort(report, "training readiness", &date_str, e, &mut had_5xx)?,
        }

        match client.hydration(&date_str).await {
            Ok(v) => {
                metrics.hydration_ml = num(&v, &["valueInML"]);
                metrics.hydration_goal_ml = num(&v, &["goalInML"]);
                metrics.sweat_loss_ml = num(&v, &["sweatLossInML"]);
            }
            Err(e) => warn_or_abort(report, "hydration", &date_str, e, &mut had_5xx)?,
        }

        match client.sleep(display_name, &date_str).await {
            Ok(v) => {
                metrics.sleep_secs = num(&v, &["dailySleepDTO", "sleepTimeSeconds"]);
                metrics.sleep_score =
                    num(&v, &["dailySleepDTO", "sleepScores", "overall", "value"]);

                // The same response, kept whole this time. The two figures
                // above are what a recovery reading needs; the night behind
                // them — stages, timings, overnight vitals — was being parsed
                // and dropped, and it costs no extra request to keep.
                //
                // Written outside `has_data()` below because it is its own
                // table: a day whose only content is a nap still has a night
                // worth recording, and a night's failure to write must not
                // stop the wellness row.
                let night = crate::sleep::SleepNight::from_payload(&date_str, &v);
                if night.has_data() {
                    if let Err(e) = db.upsert_sleep_night(&night) {
                        report
                            .warnings
                            .push(format!("sleep detail {date_str}: {e}"));
                    } else {
                        report.nights_written += 1;
                    }
                }
            }
            Err(e) => warn_or_abort(report, "sleep", &date_str, e, &mut had_5xx)?,
        }

        outage_streak = if had_5xx { outage_streak + 1 } else { 0 };
        if outage_streak >= OUTAGE_DAYS {
            bail!("Garmin answered with server errors on {OUTAGE_DAYS} days in a row, up to {date_str}");
        }

        // The day counts as real if any figure came back with a value in it —
        // not merely because the requests succeeded.
        if metrics.has_data() {
            db.upsert_daily(&metrics)?;
            report.days_written += 1;
            empty_streak = 0;
        } else if !had_5xx {
            // A day with a server error is unknown, not empty.
            empty_streak += 1;
            // Walking back past the day the watch was first worn means every
            // endpoint returns nothing, forever. The account can predate the
            // watch by years — this one does — so a full sync has to notice it
            // has run out of history rather than grind through 2018.
            if stop_when_empty && empty_streak >= EMPTY_DAY_STREAK {
                report.warnings.push(format!(
                    "stopped at {date_str}: {EMPTY_DAY_STREAK} days running with no data of any kind"
                ));
                break;
            }
        }
    }

    Ok(())
}

/// How many days back (today counts) the per-day curves are kept. The history
/// import never reaches further than this for them.
const CURVE_DAYS: i64 = 3;

/// Whether `date` is inside the window of the per-day curves.
fn is_recent(date: NaiveDate) -> bool {
    (crate::today() - date).num_days() < CURVE_DAYS
}

/// A reply that says "no data" is a warning. A failed request is not a reply:
/// a dropped network would count every day as empty, end the walk early and
/// report success. So a transport error, a 429 or a dead token stops the walk.
/// A 5xx is a warning that sets `had_5xx`, because one bad endpoint on one
/// day must not end the walk.
fn warn_or_abort(
    report: &mut SyncReport,
    what: &str,
    date: &str,
    e: anyhow::Error,
    had_5xx: &mut bool,
) -> Result<()> {
    if is_fatal(&e) {
        return Err(e.context(format!("{what} {date}")));
    }
    *had_5xx |= http_status(&e).is_some_and(|s| s >= 500);
    report.warnings.push(format!("{what} {date}: {e}"));
    Ok(())
}

pub(crate) fn http_status(e: &anyhow::Error) -> Option<u16> {
    e.downcast_ref::<crate::client::HttpStatus>()
        .map(|h| h.status)
}

/// A failure that must stop a sync: a 429, a dropped network or a dead token.
pub(crate) fn is_fatal(e: &anyhow::Error) -> bool {
    http_status(e) == Some(429)
        || e.downcast_ref::<reqwest::Error>().is_some()
        || e.downcast_ref::<crate::auth::AuthFailed>().is_some()
}

/// The reply of an optional endpoint, as data or as "no data".
///
/// A 404, a 403, an empty body and a JSON null are all "no data" and give
/// `None` with no warning. Any other status gives `None` and a warning. Only
/// the failures in [`is_fatal`] give an error, as for the older calls. A server
/// error is never counted toward the outage streak of the wellness walk.
pub(crate) fn optional(
    report: &mut SyncReport,
    what: &str,
    reply: Result<Value>,
) -> Result<Option<Value>> {
    match reply {
        Ok(Value::Null) => Ok(None),
        Ok(v) => Ok(Some(v)),
        Err(e) if is_fatal(&e) => Err(e.context(what.to_string())),
        Err(e) => {
            if !matches!(http_status(&e), Some(403 | 404 | 204)) {
                report.warnings.push(format!("{what}: {e}"));
            }
            Ok(None)
        }
    }
}

/// The result of the extras phase. A fatal failure becomes a warning, because
/// the required steps already succeeded. Any other error stays an error.
fn extras_outcome(report: &mut SyncReport, extras: Result<()>) -> Result<()> {
    match extras {
        Err(e) if is_fatal(&e) => {
            report.warnings.push(format!("extras stopped: {e:#}"));
            Ok(())
        }
        other => other,
    }
}

/// A per-item failure is a warning, but a dead network stops the loop. Each
/// failing item costs a full set of retries, so going on would take minutes.
fn warn_or_stop(report: &mut SyncReport, what: &str, e: anyhow::Error) -> Result<()> {
    if e.downcast_ref::<reqwest::Error>().is_some() {
        return Err(e.context(what.to_string()));
    }
    report.warnings.push(format!("{what}: {e}"));
    Ok(())
}

/// How many days in a row with a 5xx end the walk. One or two bad days are
/// a flaky endpoint. Three in a row mean Garmin is down.
const OUTAGE_DAYS: u32 = 3;

/// How many consecutive days with nothing at all end a full wellness walk.
/// Long enough to ride out a holiday or a month of a broken strap, short
/// enough that it doesn't cost thousands of pointless requests.
const EMPTY_DAY_STREAK: u32 = 45;

/// Pull the athlete's saved workouts.
pub async fn sync_workouts(
    client: &GarminClient,
    db: &Db,
    report: &mut SyncReport,
    on: Progress<'_>,
) -> Result<()> {
    on(SyncProgress {
        phase: "workouts",
        detail: String::new(),
        done: 0,
        total: None,
    });

    let list = match client.workouts(100).await {
        Ok(l) => l,
        Err(e) => {
            report.warnings.push(format!("workouts: {e}"));
            return Ok(());
        }
    };

    for w in &list {
        let Some(id) = num(w, &["workoutId"]).map(|n| n as i64) else {
            continue;
        };
        db.upsert_workout(&Workout {
            workout_id: id,
            name: text(w, &["workoutName"]),
            sport_type: text(w, &["sportType", "sportTypeKey"]),
            description: text(w, &["description"]),
            est_duration_s: num(w, &["estimatedDurationInSecs"]),
            est_distance_m: num(w, &["estimatedDistanceInMeters"]),
            updated_at: text(w, &["updateDate"]),
            raw: Some(w.to_string()),
        })?;
        report.workouts_written += 1;
    }

    Ok(())
}

/// Pull weigh-ins for the window, and the height BMI needs.
///
/// One request covers the whole window — weigh-ins are sparse and irregular, so
/// a day-by-day walk would be hundreds of requests to learn that most days have
/// nothing. That makes this cheap enough to always take the full window rather
/// than only the recent tail: a weigh-in can be edited or backdated in the phone
/// app long after the fact, and re-reading them all costs one call.
pub async fn sync_weight(
    client: &GarminClient,
    db: &Db,
    days: u32,
    report: &mut SyncReport,
    on: Progress<'_>,
) -> Result<()> {
    on(SyncProgress {
        phase: "weight",
        detail: String::new(),
        done: 0,
        total: None,
    });

    let today = crate::today();
    let start = today - Duration::days(days.saturating_sub(1) as i64);

    match client
        .weight_range(
            &start.format("%Y-%m-%d").to_string(),
            &today.format("%Y-%m-%d").to_string(),
        )
        .await
    {
        Ok(range) => {
            for s in &range.date_weight_list {
                // Both are required to plot a point. Garmin has never sent one
                // without them, but a weigh-in with no weight is not a weigh-in.
                let (Some(date), Some(grams)) = (s.calendar_date.as_deref(), s.weight) else {
                    continue;
                };
                db.upsert_weigh_in(&WeighIn {
                    sample_pk: s.sample_pk,
                    calendar_date: date.to_string(),
                    weight_g: grams,
                    bmi: s.bmi,
                    body_fat: s.body_fat,
                    body_water: s.body_water,
                    bone_mass: s.bone_mass,
                    muscle_mass: s.muscle_mass,
                    source_type: s.source_type.clone(),
                })?;
                report.weigh_ins_written += 1;
            }
        }
        Err(e) => report.warnings.push(format!("weight: {e}")),
    }

    // Height changes about never, but it lives nowhere else in the cache and
    // BMI is meaningless without it. A failure here costs BMI, not the sync.
    match client.user_settings().await {
        Ok(v) => {
            if let Some(cm) = num(&v, &["userData", "height"]) {
                db.set_sync_state("height_cm", &cm.to_string())?;
            }
        }
        Err(e) => report.warnings.push(format!("user settings: {e}")),
    }

    Ok(())
}

/// Pull the set-by-set record for strength sessions that don't have one.
///
/// One request per session, so it walks newest-first and stops at `limit` like
/// the track sync does. A session that comes back with no sets is still marked
/// fetched — otherwise every future sync would ask about it again forever.
pub async fn sync_strength(
    client: &GarminClient,
    db: &Db,
    limit: usize,
    report: &mut SyncReport,
    on: Progress<'_>,
) -> Result<()> {
    let pending = db.activities_missing_sets()?;
    let planned = pending.len().min(limit) as u32;

    for (i, id) in pending.into_iter().take(limit).enumerate() {
        on(SyncProgress {
            phase: "strength",
            detail: String::new(),
            done: i as u32,
            total: Some(planned),
        });

        match client.activity_exercise_sets(id).await {
            Ok(v) => {
                let sets = crate::strength::parse_sets(id, &v);
                report.sets_written += sets.len();
                db.save_exercise_sets(id, &sets)?;
            }
            Err(e) => warn_or_stop(report, &format!("exercise sets {id}"), e)?,
        }
    }

    Ok(())
}

/// Pull the personal records, and today's training status and race predictions.
///
/// Three requests for the whole account, not per day: Garmin only serves the
/// current value of each. History accumulates because this writes one row per
/// day it runs — which is the only way to get a picture of how they moved.
pub async fn sync_fitness(
    client: &GarminClient,
    db: &Db,
    display_name: &str,
    report: &mut SyncReport,
    on: Progress<'_>,
) -> Result<()> {
    on(SyncProgress {
        phase: "fitness",
        detail: String::new(),
        done: 0,
        total: None,
    });

    match client.personal_records(display_name).await {
        Ok(rows) => {
            for r in crate::records::parse_records(&rows) {
                db.upsert_personal_record(&r)?;
                report.records_written += 1;
            }
        }
        Err(e) => report.warnings.push(format!("personal records: {e}")),
    }

    let today = crate::today().format("%Y-%m-%d").to_string();

    let status = match client.training_status(&today).await {
        Ok(v) => crate::records::parse_training_status(&v),
        Err(e) => {
            report.warnings.push(format!("training status: {e}"));
            Default::default()
        }
    };
    let predictions = match client.race_predictions(display_name).await {
        Ok(v) => crate::records::parse_race_predictions(&v),
        Err(e) => {
            report.warnings.push(format!("race predictions: {e}"));
            Default::default()
        }
    };

    // Garmin dates the status itself; trust that over today's date, which can
    // differ if the watch hasn't synced. Nothing is written when both requests
    // came back empty — an all-null row would look like a recorded day.
    if status.has_data() || predictions.has_data() {
        let date = status.date.clone().unwrap_or(today);
        db.upsert_fitness_day(&date, &status, &predictions)?;
        report.fitness_days_written += 1;
    }

    Ok(())
}

/// How many points to keep per trace. Enough to draw a recognisable shape at
/// screen size without storing a survey of every ride.
const TRACK_POINTS: usize = 400;

/// Fetch GPS traces for activities that have one and aren't cached yet.
///
/// Each trace is a separate request, so this walks newest-first and stops at
/// `limit` rather than pulling years of rides on the first sync.
pub async fn sync_tracks(
    client: &GarminClient,
    db: &Db,
    limit: usize,
    report: &mut SyncReport,
    on: Progress<'_>,
) -> Result<()> {
    let pending = db.activities_missing_tracks()?;
    let planned = pending.len().min(limit) as u32;

    for (i, id) in pending.into_iter().take(limit).enumerate() {
        on(SyncProgress {
            phase: "tracks",
            detail: String::new(),
            done: i as u32,
            total: Some(planned),
        });

        let detail = match client.activity_details(id, TRACK_POINTS as u32).await {
            Ok(d) => d,
            Err(e) => {
                warn_or_stop(report, &format!("track {id}"), e)?;
                continue;
            }
        };

        let raw = dig(&detail, &["geoPolylineDTO", "polyline"])
            .and_then(|p| p.as_array())
            .cloned()
            .unwrap_or_default();

        // Garmin sets `hasPolyline` on activities whose trace it then returns
        // empty. Recording the empty result keeps the sync from asking again
        // on every run.
        let mut points: Vec<[f64; 2]> = raw
            .iter()
            .filter_map(|p| Some([p.get("lat")?.as_f64()?, p.get("lon")?.as_f64()?]))
            .collect();

        // The API honours maxPolylineSize loosely; thin whatever comes back.
        if points.len() > TRACK_POINTS {
            let step = points.len().div_ceil(TRACK_POINTS);
            points = points.iter().step_by(step).copied().collect();
        }

        let lats = || points.iter().map(|p| p[0]);
        let lons = || points.iter().map(|p| p[1]);
        let min = |it: &mut dyn Iterator<Item = f64>| it.fold(f64::INFINITY, f64::min);
        let max = |it: &mut dyn Iterator<Item = f64>| it.fold(f64::NEG_INFINITY, f64::max);

        db.upsert_track(&ActivityTrack {
            activity_id: id,
            point_count: points.len() as i64,
            start_lat: points.first().map(|p| p[0]),
            start_lon: points.first().map(|p| p[1]),
            end_lat: points.last().map(|p| p[0]),
            end_lon: points.last().map(|p| p[1]),
            min_lat: (!points.is_empty()).then(|| min(&mut lats())),
            max_lat: (!points.is_empty()).then(|| max(&mut lats())),
            min_lon: (!points.is_empty()).then(|| min(&mut lons())),
            max_lon: (!points.is_empty()).then(|| max(&mut lons())),
            points,
            ..Default::default()
        })?;
        report.tracks_written += 1;
    }

    Ok(())
}

/// A ceiling on the wellness walk, so a decade-old account can't turn one
/// "Full re-sync" click into tens of thousands of requests.
const MAX_DAYS: u32 = 1500;

/// The least weight history any sync pulls. Two years, so a trend line has
/// something to be a trend of even on an account that weighs in twice a month.
const WEIGHT_DAYS: u32 = 730;

/// Full refresh: activities plus the last `days` of wellness data.
pub async fn sync_all(client: &GarminClient, db: &Db, days: u32, full: bool) -> Result<SyncReport> {
    sync_all_with(client, db, days, full, &ignore).await
}

/// [`sync_all`], reporting each step to `on` as it goes.
pub async fn sync_all_with(
    client: &GarminClient,
    db: &Db,
    days: u32,
    full: bool,
    on: Progress<'_>,
) -> Result<SyncReport> {
    // One sync at a time across the desktop app, the mobile app and the MCP
    // server. A second caller waits, as the in-process lock of the app did.
    let _lock = acquire_sync_lock(&crate::paths::ensure_base_dir()?).await?;
    sync_all_locked(client, db, days, full, on).await
}

/// How often a waiting sync checks the lock.
const LOCK_POLL: std::time::Duration = std::time::Duration::from_millis(250);

/// Take the exclusive lock file of the data directory. The operating system
/// frees the lock when the process ends, so a crash leaves no stale lock.
async fn acquire_sync_lock(dir: &std::path::Path) -> Result<std::fs::File> {
    let path = dir.join("sync.lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .with_context(|| format!("could not open {}", path.display()))?;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) => tokio::time::sleep(LOCK_POLL).await,
            Err(std::fs::TryLockError::Error(e)) => {
                return Err(e).context("could not lock the sync file")
            }
        }
    }
}

async fn sync_all_locked(
    client: &GarminClient,
    db: &Db,
    days: u32,
    full: bool,
    on: Progress<'_>,
) -> Result<SyncReport> {
    let mut report = SyncReport::default();

    sync_activities(client, db, full, &mut report, on).await?;

    // A full sync means "everything I have", and how far back that goes is a
    // property of the watch, not of whatever number the caller guessed. The
    // activities are already in by now, so the first one dates the account.
    let days = if full {
        days.max(history_days(db)).min(MAX_DAYS)
    } else {
        days
    };
    let dates = wellness_dates(db, days, full)?;

    // The display name is in the path of the wellness, record and prediction
    // endpoints alike, so one failure here costs all of them.
    match client.profile().await {
        Ok(p) => {
            sync_daily(client, db, &p.display_name, &dates, full, &mut report, on).await?;
            sync_fitness(client, db, &p.display_name, &mut report, on).await?;
        }
        // A full pass that skips the wellness walk must not look like success.
        Err(e) if full => return Err(e.context("could not resolve profile")),
        Err(e) => report.warnings.push(format!(
            "could not resolve profile, skipped wellness and fitness data: {e}"
        )),
    }

    sync_workouts(client, db, &mut report, on).await?;

    // Always a generous window, whatever the caller asked for. The whole range
    // costs one request, so narrowing it saves nothing and would leave anyone
    // who only ever presses the sidebar's 30-day Sync without the history the
    // trend line needs.
    sync_weight(
        client,
        db,
        days.clamp(WEIGHT_DAYS, MAX_DAYS),
        &mut report,
        on,
    )
    .await?;

    // One request per trace, so a full sync takes the backlog in bites. An
    // incremental sync only ever has the handful of new ones to fetch.
    sync_tracks(client, db, if full { 200 } else { 25 }, &mut report, on).await?;

    // Same shape, and a smaller backlog — strength sessions are a fraction of
    // the activities that carry GPS.
    sync_strength(client, db, if full { 200 } else { 25 }, &mut report, on).await?;

    // Last, so that no optional endpoint can come between the sync and the
    // steps above. A fatal failure there only stops the extras: the required
    // steps are done, so the sync still ends and writes `last_sync`.
    let extras = crate::sync_extra::sync_extras(client, db, &mut report, on).await;
    extras_outcome(&mut report, extras)?;

    db.set_sync_state("last_sync", &Utc::now().to_rfc3339())?;
    on(SyncProgress {
        phase: "done",
        detail: String::new(),
        done: 0,
        total: None,
    });
    Ok(report)
}

/// How many recent days an incremental sync re-fetches unconditionally.
///
/// Only the very recent past can still change on Garmin's side: today is half
/// written, and last night's sleep, body battery and HRV land the following
/// morning. A day from a week ago is settled — re-asking for it is five HTTP
/// requests that overwrite a row with itself.
///
/// Three rather than two so a sync run just after midnight still covers the
/// day that just ended along with the night attributed to it.
const RECHECK_DAYS: u32 = 3;

/// Which days the wellness walk should ask about.
///
/// A full sync takes the lot, contiguously — that's what makes it full, and
/// `EMPTY_DAY_STREAK` needs the walk to be unbroken to know where history ends.
///
/// An incremental sync takes `RECHECK_DAYS`, plus any older day in the window
/// the cache has nothing for. That second part is what keeps the short window
/// honest: a fortnight with the app closed, or days the watch hadn't uploaded
/// yet when the last sync ran, are holes rather than stale rows, and they get
/// filled without making every ordinary refresh pay for a month of requests.
fn wellness_dates(db: &Db, days: u32, full: bool) -> Result<Vec<NaiveDate>> {
    let today = crate::today();
    let all = (0..days).map(|o| today - Duration::days(o as i64));

    if full {
        return Ok(all.collect());
    }

    let oldest = today - Duration::days(days.saturating_sub(1) as i64);
    let oldest_str = oldest.format("%Y-%m-%d").to_string();
    let have = db.daily_dates_since(&oldest_str)?;
    // Days whose wellness row records that there *was* a night, but whose
    // detail this cache doesn't hold — every day of every install that
    // predates the `sleep_nights` table.
    //
    // Asking on the strength of `sleep_secs` rather than on the absence of a
    // night is what makes this self-limiting: a day the watch wasn't worn has
    // no sleep seconds either, so it is never a hole and never re-fetched. A
    // day that is one gets filled on the next sync and stops being one.
    let missing_detail = db.dates_missing_sleep_detail(&oldest_str)?;

    Ok(all
        .enumerate()
        .filter(|(offset, date)| {
            let key = date.format("%Y-%m-%d").to_string();
            *offset < RECHECK_DAYS as usize || !have.contains(&key) || missing_detail.contains(&key)
        })
        .map(|(_, date)| date)
        .collect())
}

/// Days from the oldest cached activity to today, or 0 if there are none.
fn history_days(db: &Db) -> u32 {
    let Ok(Some(first)) = db.earliest_activity_date() else {
        return 0;
    };
    let Some(date) = first
        .get(..10)
        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
    else {
        return 0;
    };
    // +1 so the day of the first activity is itself included.
    (crate::today() - date).num_days().max(0) as u32 + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_second_sync_waits_for_the_lock_file() {
        let dir =
            std::env::temp_dir().join(format!("garmin-companion-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = acquire_sync_lock(&dir).await.unwrap();
        // Each `open` has its own lock, so one process can test the wait.
        let second = tokio::time::timeout(LOCK_POLL * 3, acquire_sync_lock(&dir)).await;
        assert!(second.is_err(), "the second caller must wait");
        drop(first);
        tokio::time::timeout(LOCK_POLL * 8, acquire_sync_lock(&dir))
            .await
            .expect("the lock is free after the first drops")
            .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_fatal_failure_in_the_extras_does_not_fail_the_sync() {
        let status = |code| {
            anyhow::Error::new(crate::client::HttpStatus {
                path: "/x".into(),
                status: code,
                body: String::new(),
            })
        };
        let mut report = SyncReport::default();
        assert!(extras_outcome(&mut report, Ok(())).is_ok());
        assert!(report.warnings.is_empty());
        assert!(extras_outcome(&mut report, Err(status(429).context("courses"))).is_ok());
        let dead = anyhow::Error::new(crate::auth::AuthFailed("refresh failed".into()));
        assert!(extras_outcome(&mut report, Err(dead)).is_ok());
        let transport = reqwest::Client::new().get("not a url").build().unwrap_err();
        assert!(extras_outcome(&mut report, Err(anyhow::Error::new(transport))).is_ok());
        assert_eq!(report.warnings.len(), 3);
        // An error that is not fatal (for example a database error) still fails.
        assert!(extras_outcome(&mut report, Err(anyhow::anyhow!("disk full"))).is_err());
    }

    #[test]
    fn a_failed_request_aborts_but_a_status_error_only_warns() {
        let mut report = SyncReport::default();
        let mut had_5xx = false;
        let status = |code| {
            anyhow::Error::new(crate::client::HttpStatus {
                path: "/x".into(),
                status: code,
                body: String::new(),
            })
        };
        let mut run = |report: &mut SyncReport, e| {
            warn_or_abort(report, "hrv", "2025-01-01", e, &mut had_5xx)
        };
        assert!(run(&mut report, status(404)).is_ok());
        assert_eq!(report.warnings.len(), 1);
        assert!(run(&mut report, status(429)).is_err());
        let dead = anyhow::Error::new(crate::auth::AuthFailed("refresh failed".into()));
        assert!(run(&mut report, dead).is_err());
        let transport = reqwest::Client::new().get("not a url").build().unwrap_err();
        let wrapped = anyhow::Error::new(transport).context("request to /x failed");
        assert!(run(&mut report, wrapped).is_err());
        assert_eq!(report.warnings.len(), 1);
        assert!(run(&mut report, status(503)).is_ok());
        assert_eq!(report.warnings.len(), 2);
        assert!(had_5xx);
    }
}
