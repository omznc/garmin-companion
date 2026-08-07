//! MCP tools over the local Garmin cache.
//!
//! Every tool reads SQLite, not Garmin — so tool calls are instant and can't
//! fail on a network hiccup mid-conversation. `sync` is the one tool that
//! touches the network, and it's explicit.
//!
//! The analyses themselves live in `garmin_core::query`, shared with the
//! desktop app's chat tools. What's here is the MCP surface: parameter schemas,
//! descriptions, and output shapes mirrored from core so this crate's schemars
//! version stays out of the core crate.

use garmin_core::{db::Db, query};
use rmcp::{
    handler::server::wrapper::{Json, Parameters},
    model::{ErrorData, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Default)]
pub struct GarminServer;

fn db() -> Result<Db, ErrorData> {
    Db::open_default().map_err(internal)
}

fn internal(e: anyhow::Error) -> ErrorData {
    ErrorData::internal_error(
        e.chain()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(": "),
        None,
    )
}

// -------------------------------------------------------------- output shapes
//
// One-to-one mirrors of the `garmin_core::query` types. They exist only to
// carry `JsonSchema`, which is what lets rmcp publish an output schema — the
// core crate stays free of the MCP SDK's schemars version. Every field is
// copied straight across; no logic lives in these conversions.

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ZoneSplit {
    pub zone: u8,
    pub minutes: f64,
    pub percent: f64,
}

impl From<query::ZoneSplit> for ZoneSplit {
    fn from(z: query::ZoneSplit) -> Self {
        Self {
            zone: z.zone,
            minutes: z.minutes,
            percent: z.percent,
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
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

impl From<query::ActivityView> for ActivityView {
    fn from(a: query::ActivityView) -> Self {
        Self {
            activity_id: a.activity_id,
            name: a.name,
            sport: a.sport,
            start: a.start,
            distance_km: a.distance_km,
            duration_min: a.duration_min,
            pace_min_per_km: a.pace_min_per_km,
            avg_hr: a.avg_hr,
            max_hr: a.max_hr,
            avg_cadence: a.avg_cadence,
            aerobic_training_effect: a.aerobic_training_effect,
            anaerobic_training_effect: a.anaerobic_training_effect,
            zones: a.zones.into_iter().map(ZoneSplit::from).collect(),
            easy_percent: a.easy_percent,
            has_hr_data: a.has_hr_data,
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
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

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct DriftReport {
    pub runs: Vec<DriftPoint>,
    /// Z1+Z2 share across the window, weighted by time and counting only runs
    /// that recorded HR.
    pub overall_easy_percent: f64,
    pub overall_hard_percent: f64,
    pub total_run_minutes: f64,
    pub longest_run_minutes: f64,
    pub runs_with_hr: usize,
    pub runs_examined: usize,
}

impl From<query::DriftReport> for DriftReport {
    fn from(r: query::DriftReport) -> Self {
        Self {
            runs: r
                .runs
                .into_iter()
                .map(|p| DriftPoint {
                    activity_id: p.activity_id,
                    date: p.date,
                    duration_min: p.duration_min,
                    avg_hr: p.avg_hr,
                    z5_percent: p.z5_percent,
                    hard_percent: p.hard_percent,
                    easy_percent: p.easy_percent,
                    has_hr_data: p.has_hr_data,
                })
                .collect(),
            overall_easy_percent: r.overall_easy_percent,
            overall_hard_percent: r.overall_hard_percent,
            total_run_minutes: r.total_run_minutes,
            longest_run_minutes: r.longest_run_minutes,
            runs_with_hr: r.runs_with_hr,
            runs_examined: r.runs_examined,
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CadencePoint {
    pub activity_id: i64,
    pub date: Option<String>,
    pub avg_cadence: Option<f64>,
    pub pace_min_per_km: Option<f64>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CadenceReport {
    pub runs: Vec<CadencePoint>,
    pub average_cadence: Option<f64>,
    /// Runs with cadence data, out of the runs examined. A treadmill run
    /// without a footpod reports nothing.
    pub runs_with_cadence: usize,
    pub runs_examined: usize,
}

impl From<query::CadenceReport> for CadenceReport {
    fn from(r: query::CadenceReport) -> Self {
        Self {
            runs: r
                .runs
                .into_iter()
                .map(|p| CadencePoint {
                    activity_id: p.activity_id,
                    date: p.date,
                    avg_cadence: p.avg_cadence,
                    pace_min_per_km: p.pace_min_per_km,
                })
                .collect(),
            average_cadence: r.average_cadence,
            runs_with_cadence: r.runs_with_cadence,
            runs_examined: r.runs_examined,
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
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

impl From<query::RecoveryDay> for RecoveryDay {
    fn from(d: query::RecoveryDay) -> Self {
        Self {
            date: d.date,
            resting_hr: d.resting_hr,
            hrv_last_night: d.hrv_last_night,
            hrv_weekly_avg: d.hrv_weekly_avg,
            hrv_status: d.hrv_status,
            training_readiness: d.training_readiness,
            sleep_hours: d.sleep_hours,
            sleep_score: d.sleep_score,
            steps: d.steps,
            stress_avg: d.stress_avg,
            body_battery_high: d.body_battery_high,
            body_battery_low: d.body_battery_low,
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CacheStatus {
    pub activities_cached: i64,
    pub last_sync: Option<String>,
    pub database_path: Option<String>,
    pub connected_to_garmin: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SyncSummary {
    pub activities_seen: usize,
    pub activities_written: usize,
    pub days_written: usize,
    pub workouts_written: usize,
    pub tracks_written: usize,
    pub warnings: Vec<String>,
}

// ------------------------------------------------------------------ parameters

#[derive(Debug, Deserialize, schemars::JsonSchema, Default)]
pub struct RecentParams {
    /// How many activities to return. Defaults to 10.
    #[serde(default)]
    pub limit: Option<u32>,
    /// Restrict to one sport, matched as a substring of Garmin's type key
    /// (e.g. "running", "strength", "jump_rope").
    #[serde(default)]
    pub sport: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema, Default)]
pub struct ActivityParams {
    /// Garmin activity id. Omit to use the most recent activity.
    #[serde(default)]
    pub activity_id: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema, Default)]
pub struct RunCountParams {
    /// How many recent runs to look across. Defaults to 10.
    #[serde(default)]
    pub count: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema, Default)]
pub struct DaysParams {
    /// How many days back to report. Defaults to 14.
    #[serde(default)]
    pub days: Option<u32>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct NutritionDay {
    pub date: String,
    /// Calories eaten. Null when nothing was logged that day — which is not a
    /// day of eating nothing, and must not be averaged in as zero.
    pub consumed_kcal: Option<f64>,
    /// Everything burned: active plus basal.
    pub total_burn_kcal: Option<f64>,
    pub active_kcal: Option<f64>,
    pub bmr_kcal: Option<f64>,
    pub net_calorie_goal: Option<f64>,
    /// Eaten minus burned. Negative is a deficit.
    pub balance_kcal: Option<f64>,
    pub hydration_ml: Option<f64>,
    pub sweat_loss_ml: Option<f64>,
    /// Whether a food log existed at all that day.
    pub logged: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct NutritionReport {
    pub days: Vec<NutritionDay>,
    pub days_logged: usize,
    /// Averaged over logged days only.
    pub avg_consumed_kcal: Option<f64>,
    pub avg_burn_kcal: Option<f64>,
    pub avg_balance_kcal: Option<f64>,
}

impl From<query::NutritionReport> for NutritionReport {
    fn from(r: query::NutritionReport) -> Self {
        Self {
            days: r
                .days
                .into_iter()
                .map(|d| NutritionDay {
                    date: d.date,
                    consumed_kcal: d.consumed_kcal,
                    total_burn_kcal: d.total_burn_kcal,
                    active_kcal: d.active_kcal,
                    bmr_kcal: d.bmr_kcal,
                    net_calorie_goal: d.net_calorie_goal,
                    balance_kcal: d.balance_kcal,
                    hydration_ml: d.hydration_ml,
                    sweat_loss_ml: d.sweat_loss_ml,
                    logged: d.logged,
                })
                .collect(),
            days_logged: r.days_logged,
            avg_consumed_kcal: r.avg_consumed_kcal,
            avg_burn_kcal: r.avg_burn_kcal,
            avg_balance_kcal: r.avg_balance_kcal,
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Workout {
    pub workout_id: i64,
    pub name: Option<String>,
    pub sport: Option<String>,
    pub description: Option<String>,
    /// Garmin reports zero for workouts with no timed structure, such as most
    /// strength sessions; those come through as null rather than a real zero.
    pub est_duration_min: Option<f64>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct RouteSummary {
    pub name: Option<String>,
    pub sport: Option<String>,
    /// How many outings matched into this route.
    pub times: usize,
    pub avg_distance_km: Option<f64>,
    pub dates: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema, Default)]
pub struct SyncParams {
    /// Days of wellness data to refresh. Defaults to 30.
    #[serde(default)]
    pub days: Option<u32>,
    /// Walk the full activity history instead of stopping once caught up.
    #[serde(default)]
    pub full: Option<bool>,
}

// ----------------------------------------------------------------------- tools

#[tool_router]
impl GarminServer {
    #[tool(
        description = "List recent activities with per-zone HR breakdown, pace, \
                       cadence and training effect. Start here for 'how am I doing'."
    )]
    async fn recent_activities(
        &self,
        Parameters(p): Parameters<RecentParams>,
    ) -> Result<Json<Vec<ActivityView>>, ErrorData> {
        let db = db()?;
        let views = query::recent_activities(&db, p.limit.unwrap_or(10), p.sport.as_deref())
            .map_err(internal)?;
        Ok(Json(views.into_iter().map(ActivityView::from).collect()))
    }

    #[tool(description = "Full HR zone breakdown for one activity: minutes and \
                       percent in each of zones 1-5. Defaults to the latest activity.")]
    async fn activity_zones(
        &self,
        Parameters(p): Parameters<ActivityParams>,
    ) -> Result<Json<ActivityView>, ErrorData> {
        let db = db()?;
        let found = query::activity_zones(&db, p.activity_id).map_err(internal)?;
        let a = found.ok_or_else(|| {
            ErrorData::invalid_params(
                "No such activity in the local cache. Run `sync` first.",
                None,
            )
        })?;
        Ok(Json(ActivityView::from(a)))
    }

    #[tool(
        description = "Track hard-effort drift across recent runs: per-run Z5 and \
                       Z3-5 share, plus the time-weighted easy/hard split for the \
                       whole window. Use this to answer whether easy runs are \
                       staying easy."
    )]
    async fn zone_drift(
        &self,
        Parameters(p): Parameters<RunCountParams>,
    ) -> Result<Json<DriftReport>, ErrorData> {
        let db = db()?;
        let report = query::zone_drift(&db, p.count.unwrap_or(10)).map_err(internal)?;
        Ok(Json(DriftReport::from(report)))
    }

    #[tool(
        description = "Running cadence across recent runs, for spotting a low \
                       or improving step rate."
    )]
    async fn cadence_trend(
        &self,
        Parameters(p): Parameters<RunCountParams>,
    ) -> Result<Json<CadenceReport>, ErrorData> {
        let db = db()?;
        let report = query::cadence_trend(&db, p.count.unwrap_or(10)).map_err(internal)?;
        Ok(Json(CadenceReport::from(report)))
    }

    #[tool(
        description = "Recovery signals by day: resting HR, HRV (last night and \
                       weekly average, plus Garmin's status), training readiness, \
                       sleep, stress and body battery. Use before advising a hard \
                       session."
    )]
    async fn recovery(
        &self,
        Parameters(p): Parameters<DaysParams>,
    ) -> Result<Json<Vec<RecoveryDay>>, ErrorData> {
        let db = db()?;
        let days = query::recovery(&db, p.days.unwrap_or(14)).map_err(internal)?;
        Ok(Json(days.into_iter().map(RecoveryDay::from).collect()))
    }

    #[tool(description = "Calories eaten against calories burned by day, plus \
                       hydration and sweat loss. Days with no food log report \
                       `logged: false` and a null `consumed_kcal` — that is a \
                       missing log, not a day of eating nothing, so exclude \
                       those rather than treating them as zero.")]
    async fn nutrition(
        &self,
        Parameters(p): Parameters<DaysParams>,
    ) -> Result<Json<NutritionReport>, ErrorData> {
        let db = db()?;
        let report = query::nutrition(&db, p.days.unwrap_or(30)).map_err(internal)?;
        Ok(Json(NutritionReport::from(report)))
    }

    #[tool(description = "The athlete's saved Garmin workouts — the structured \
                       sessions they built. There is no training plan or goal \
                       race on the account, so these are the closest thing to \
                       a plan; compare them against what was actually run.")]
    async fn workouts(&self) -> Result<Json<Vec<Workout>>, ErrorData> {
        let db = db()?;
        let rows = db.workouts().map_err(internal)?;
        Ok(Json(
            rows.into_iter()
                .map(|w| Workout {
                    workout_id: w.workout_id,
                    name: w.name,
                    sport: w.sport_type,
                    description: w.description,
                    est_duration_min: w
                        .est_duration_s
                        .filter(|s| *s > 0.0)
                        .map(|s| query::round1(s / 60.0)),
                })
                .collect(),
        ))
    }

    #[tool(
        description = "Routes built from cached GPS traces, grouped when outings \
                       start and finish in the same place and cover a similar \
                       distance. Only activities recorded outdoors have a trace \
                       — treadmill sessions never will."
    )]
    async fn routes(&self) -> Result<Json<Vec<RouteSummary>>, ErrorData> {
        let db = db()?;
        let rows = query::route_summaries(&db).map_err(internal)?;
        Ok(Json(
            rows.into_iter()
                .map(|r| RouteSummary {
                    name: r.name,
                    sport: r.sport,
                    times: r.times,
                    avg_distance_km: r.avg_distance_km,
                    dates: r.dates,
                })
                .collect(),
        ))
    }

    #[tool(
        description = "What's in the local cache and when it was last refreshed. \
                       Check this if data looks stale or a session seems missing."
    )]
    async fn cache_status(&self) -> Result<Json<CacheStatus>, ErrorData> {
        let db = db()?;
        let s = query::cache_status(&db).map_err(internal)?;
        Ok(Json(CacheStatus {
            activities_cached: s.activities_cached,
            last_sync: s.last_sync,
            database_path: s.database_path,
            connected_to_garmin: s.connected_to_garmin,
        }))
    }

    #[tool(
        description = "Fetch new data from Garmin into the local cache. The only \
                       tool that hits the network; run it when data looks stale."
    )]
    async fn sync(
        &self,
        Parameters(p): Parameters<SyncParams>,
    ) -> Result<Json<SyncSummary>, ErrorData> {
        let (days, full) = (p.days.unwrap_or(30), p.full.unwrap_or(false));

        // `rusqlite::Connection` isn't `Sync`, and a sync interleaves DB writes
        // with network awaits — so the whole thing runs on a blocking thread
        // that owns its own connection, rather than holding one across awaits.
        let handle = tokio::runtime::Handle::current();
        let report = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            let client = garmin_core::require_client()?;
            let db = Db::open_default()?;
            handle.block_on(garmin_core::sync::sync_all(&client, &db, days, full))
        })
        .await
        .map_err(|e| ErrorData::internal_error(format!("sync task panicked: {e}"), None))?
        .map_err(internal)?;

        Ok(Json(SyncSummary {
            activities_seen: report.activities_seen,
            activities_written: report.activities_written,
            days_written: report.days_written,
            workouts_written: report.workouts_written,
            tracks_written: report.tracks_written,
            warnings: report.warnings,
        }))
    }
}

#[tool_handler]
impl ServerHandler for GarminServer {
    fn get_info(&self) -> ServerInfo {
        // ServerInfo and Implementation are #[non_exhaustive], so they're built
        // by mutating a default rather than with a struct literal.
        let mut info = ServerInfo::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.server_info.name = "garmin-mcp".into();
        info.server_info.version = env!("CARGO_PKG_VERSION").into();
        info.instructions = Some(
            "Garmin Connect training and recovery data, served from a local \
             cache.\n\n\
             Zone numbers come from the athlete's own Garmin zone \
             configuration, so Z2 means their Z2. Treat Z1+Z2 as easy and \
             Z3-Z5 as hard when reasoning about an 80/20 split.\n\n\
             Activities without HR data (some strength sessions) report every \
             zone as zero and set `has_hr_data: false`. That is not a session \
             spent entirely in Z1 — exclude those before drawing a conclusion \
             about effort.\n\n\
             All tools read cached data. Call `sync` first if `cache_status` \
             shows a stale `last_sync`."
                .into(),
        );
        info
    }
}
