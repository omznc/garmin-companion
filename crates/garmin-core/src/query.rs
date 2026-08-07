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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStatus {
    pub activities_cached: i64,
    pub last_sync: Option<String>,
    pub database_path: Option<String>,
    pub connected_to_garmin: bool,
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
        .recent_daily(days)?
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
        .recent_daily(days)?
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
/// process down on the way in. Routes are sorted by how often they repeat, so
/// the ones past this are the tail of one-off outings; they still appear in
/// the list and the counts, just without a drawn thumbnail.
const TRACED_ROUTES: usize = 40;

/// Routes with their traces, for the screen that draws them.
pub fn routes(db: &Db) -> Result<Vec<Route>> {
    let mut routes = route_groups(db)?;

    // Coordinates are read only now, and only for the one outing per route the
    // screen actually draws. Fetching all of them up front was shipping tens of
    // thousands of unused points across the IPC boundary.
    for route in routes.iter_mut().take(TRACED_ROUTES) {
        route.outings[0].points = db.track_points(route.outings[0].activity_id)?;
    }

    Ok(routes)
}

/// The grouping on its own, with every `points` left empty.
fn route_groups(db: &Db) -> Result<Vec<Route>> {
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

    // Repeated routes first — those are the ones the screen exists to surface.
    routes.sort_by(|a, b| {
        b.times
            .cmp(&a.times)
            .then_with(|| b.outings[0].local_date.cmp(&a.outings[0].local_date))
    });
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
    Ok(route_groups(db)?
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

pub fn cache_status(db: &Db) -> Result<CacheStatus> {
    Ok(CacheStatus {
        activities_cached: db.activity_count()?,
        last_sync: db.sync_state("last_sync")?,
        database_path: crate::db::default_path().map(|p| p.display().to_string()),
        connected_to_garmin: crate::store::load_tokens()?.is_some(),
    })
}
