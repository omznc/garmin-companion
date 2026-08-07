//! Pulling Garmin data into the local cache.
//!
//! Sync is incremental and idempotent: activities upsert by id, daily metrics
//! upsert by date. Re-running it is always safe.

use anyhow::Result;
use chrono::{Duration, NaiveDate, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::client::GarminClient;
use crate::db::{ActivityTrack, DailyMetrics, Db, Workout};

/// How many consecutive already-cached activities we tolerate before deciding
/// we've caught up. Not zero, because Garmin occasionally backfills an older
/// activity that would otherwise hide everything behind it.
const CATCH_UP_STREAK: usize = 10;

const PAGE: u32 = 50;

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub activities_seen: usize,
    pub activities_written: usize,
    pub days_written: usize,
    pub workouts_written: usize,
    pub tracks_written: usize,
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

/// Pull the wellness metrics for the last `days` days.
///
/// Each endpoint is fetched independently and failures are collected rather
/// than propagated — Garmin returns 404 for days with no data (device not worn,
/// no sleep recorded), which is normal and shouldn't fail the sync.
pub async fn sync_daily(
    client: &GarminClient,
    db: &Db,
    display_name: &str,
    days: u32,
    report: &mut SyncReport,
) -> Result<()> {
    let today = Utc::now().date_naive();

    for offset in 0..days {
        let date: NaiveDate = today - Duration::days(offset as i64);
        let date_str = date.format("%Y-%m-%d").to_string();
        let mut metrics = DailyMetrics {
            date: date_str.clone(),
            ..Default::default()
        };
        let mut got_anything = false;

        match client.user_summary(display_name, &date_str).await {
            Ok(v) => {
                got_anything = true;
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
                got_anything = true;
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
                if let Some(score) = num(first, &["score"]) {
                    got_anything = true;
                    metrics.training_readiness = Some(score);
                }
            }
            Err(e) => report
                .warnings
                .push(format!("training readiness {date_str}: {e}")),
        }

        match client.hydration(&date_str).await {
            Ok(v) => {
                got_anything = true;
                metrics.hydration_ml = num(&v, &["valueInML"]);
                metrics.hydration_goal_ml = num(&v, &["goalInML"]);
                metrics.sweat_loss_ml = num(&v, &["sweatLossInML"]);
            }
            Err(e) => report.warnings.push(format!("hydration {date_str}: {e}")),
        }

        match client.sleep(display_name, &date_str).await {
            Ok(v) => {
                got_anything = true;
                metrics.sleep_secs = num(&v, &["dailySleepDTO", "sleepTimeSeconds"]);
                metrics.sleep_score =
                    num(&v, &["dailySleepDTO", "sleepScores", "overall", "value"]);
            }
            Err(e) => report.warnings.push(format!("sleep {date_str}: {e}")),
        }

        if got_anything {
            db.upsert_daily(&metrics)?;
            report.days_written += 1;
        }
    }

    Ok(())
}

/// Pull the athlete's saved workouts.
pub async fn sync_workouts(client: &GarminClient, db: &Db, report: &mut SyncReport) -> Result<()> {
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
) -> Result<()> {
    let pending = db.activities_missing_tracks()?;

    for id in pending.into_iter().take(limit) {
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

/// Full refresh: activities plus the last `days` of wellness data.
pub async fn sync_all(client: &GarminClient, db: &Db, days: u32, full: bool) -> Result<SyncReport> {
    let mut report = SyncReport::default();

    sync_activities(client, db, full, &mut report).await?;

    match client.profile().await {
        Ok(p) => sync_daily(client, db, &p.display_name, days, &mut report).await?,
        Err(e) => report.warnings.push(format!(
            "could not resolve profile, skipped wellness data: {e}"
        )),
    }

    sync_workouts(client, db, &mut report).await?;

    // One request per trace, so a full sync takes the backlog in bites. An
    // incremental sync only ever has the handful of new ones to fetch.
    sync_tracks(client, db, if full { 200 } else { 25 }, &mut report).await?;

    db.set_sync_state("last_sync", &Utc::now().to_rfc3339())?;
    Ok(report)
}
