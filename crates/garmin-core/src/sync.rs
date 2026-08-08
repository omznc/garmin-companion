//! Pulling Garmin data into the local cache.
//!
//! Sync is incremental and idempotent: activities upsert by id, daily metrics
//! upsert by date. Re-running it is always safe.

use anyhow::Result;
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
    /// Which stage: `activities`, `wellness`, `workouts`, `tracks`, `done`.
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
    pub workouts_written: usize,
    pub tracks_written: usize,
    pub weigh_ins_written: usize,
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

    for (i, date) in dates.iter().enumerate() {
        let offset = i as u32;
        let date_str = date.format("%Y-%m-%d").to_string();
        let mut metrics = DailyMetrics {
            date: date_str.clone(),
            ..Default::default()
        };

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
            Err(e) => report
                .warnings
                .push(format!("user summary {date_str}: {e}")),
        }

        match client.hrv(&date_str).await {
            Ok(v) => {
                metrics.hrv_last_night = num(&v, &["hrvSummary", "lastNightAvg"]);
                metrics.hrv_weekly_avg = num(&v, &["hrvSummary", "weeklyAvg"]);
                metrics.hrv_status = text(&v, &["hrvSummary", "status"]);
            }
            Err(e) => report.warnings.push(format!("hrv {date_str}: {e}")),
        }

        match client.training_readiness(&date_str).await {
            Ok(v) => {
                // This endpoint returns an array with at most one entry.
                let first = v.as_array().and_then(|a| a.first()).unwrap_or(&v);
                metrics.training_readiness = num(first, &["score"]);
            }
            Err(e) => report
                .warnings
                .push(format!("training readiness {date_str}: {e}")),
        }

        match client.hydration(&date_str).await {
            Ok(v) => {
                metrics.hydration_ml = num(&v, &["valueInML"]);
                metrics.hydration_goal_ml = num(&v, &["goalInML"]);
                metrics.sweat_loss_ml = num(&v, &["sweatLossInML"]);
            }
            Err(e) => report.warnings.push(format!("hydration {date_str}: {e}")),
        }

        match client.sleep(display_name, &date_str).await {
            Ok(v) => {
                metrics.sleep_secs = num(&v, &["dailySleepDTO", "sleepTimeSeconds"]);
                metrics.sleep_score =
                    num(&v, &["dailySleepDTO", "sleepScores", "overall", "value"]);
            }
            Err(e) => report.warnings.push(format!("sleep {date_str}: {e}")),
        }

        // The day counts as real if any figure came back with a value in it —
        // not merely because the requests succeeded.
        if metrics.has_data() {
            db.upsert_daily(&metrics)?;
            report.days_written += 1;
            empty_streak = 0;
        } else {
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

    let today = Utc::now().date_naive();
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
                report.warnings.push(format!("track {id}: {e}"));
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

    match client.profile().await {
        Ok(p) => sync_daily(client, db, &p.display_name, &dates, full, &mut report, on).await?,
        Err(e) => report.warnings.push(format!(
            "could not resolve profile, skipped wellness data: {e}"
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
    let today = Utc::now().date_naive();
    let all = (0..days).map(|o| today - Duration::days(o as i64));

    if full {
        return Ok(all.collect());
    }

    let oldest = today - Duration::days(days.saturating_sub(1) as i64);
    let have = db.daily_dates_since(&oldest.format("%Y-%m-%d").to_string())?;

    Ok(all
        .enumerate()
        .filter(|(offset, date)| {
            *offset < RECHECK_DAYS as usize || !have.contains(&date.format("%Y-%m-%d").to_string())
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
    (Utc::now().date_naive() - date).num_days().max(0) as u32 + 1
}
