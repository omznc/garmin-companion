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
    /// `good`, `caution` or `poor`: how far the zone split above can carry an
    /// argument. Anything other than `good` comes with `hr_caveats` saying why,
    /// and those are worth passing on rather than quietly discounting.
    pub hr_confidence: &'static str,
    /// `notChecked`, `unlikely`, `possible` or `likely`. The wrist sensor can
    /// lock onto arm swing and report step rate as pulse; `likely` means heart
    /// rate shadowed cadence for most of the session, which no real heart rate
    /// does.
    pub cadence_lock: &'static str,
    pub hr_caveats: Vec<String>,
    /// True indoors, where distance and pace are estimated from the arm
    /// accelerometer rather than measured. Heart rate and cadence indoors are
    /// fine; comparing indoor paces compares two estimates.
    pub pace_estimated: bool,
    /// Pace over moving time. Differs from `pace_min_per_km` on a run/walk
    /// session, where that one averages the walk breaks in.
    pub moving_pace_min_per_km: Option<f64>,
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
            hr_confidence: a.hr_confidence.level.as_str(),
            cadence_lock: a.hr_confidence.cadence_lock.as_str(),
            hr_caveats: a.hr_confidence.notes,
            pace_estimated: a.pace_estimated,
            moving_pace_min_per_km: a.moving_pace_min_per_km,
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
    /// Which measurement `resting_hr` actually is. Worn overnight, Garmin
    /// reports the lowest 30-minute average, which lands in deep sleep and is
    /// within about a beat of an ECG. Not worn overnight, it reports a rough
    /// estimate from the lowest waking minute instead — the same field, a
    /// different and weaker measurement. Only `overnight` days belong on one
    /// trend line together.
    pub resting_hr_source: &'static str,
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
            resting_hr_source: d.resting_hr_source.as_str(),
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
    /// When the app last asked Garmin for data. Says nothing about whether
    /// there was any: a sync against a watch nobody wore succeeds and writes
    /// nothing, so this moves to now while the data underneath stays where it
    /// was. Read `days_since_daily` for that.
    pub last_sync: Option<String>,
    pub database_path: Option<String>,
    pub connected_to_garmin: bool,
    /// Local date of the newest cached activity, and how many days back it is.
    pub newest_activity_date: Option<String>,
    pub days_since_activity: Option<i64>,
    /// Same for wellness data — resting HR, HRV, sleep, readiness. A worn watch
    /// produces a row every day, so a gap here means it was off the wrist.
    pub newest_daily_date: Option<String>,
    pub days_since_daily: Option<i64>,
    /// True when no wellness data has arrived for several days. Everything the
    /// other tools return is still real, but it describes then, not now.
    pub stale: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SyncSummary {
    pub activities_seen: usize,
    pub activities_written: usize,
    pub days_written: usize,
    pub workouts_written: usize,
    pub tracks_written: usize,
    pub sets_written: usize,
    pub records_written: usize,
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

/* -------------------------------------------------------------------- sleep --- */

/// One night, in the units a reader thinks in.
///
/// Deliberately not a mirror of `sleep::SleepNight`. That type carries the
/// hypnogram and the overnight heart-rate curve, which exist for the app's
/// charts — a hundred points per night is noise in a conversation, and ninety
/// nights of it would crowd out everything worth saying. Seconds become minutes
/// and hours for the same reason.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SleepNightView {
    /// The date woken up on, which is how Garmin keys a night.
    pub date: String,
    pub score: Option<f64>,
    /// Garmin's word for the score: `EXCELLENT`, `GOOD`, `FAIR`, `POOR`.
    pub score_qualifier: Option<String>,
    /// Garmin's own verdict on the night, as a shouted enum.
    pub feedback: Option<String>,
    pub hours_asleep: Option<f64>,
    pub deep_mins: Option<f64>,
    pub light_mins: Option<f64>,
    pub rem_mins: Option<f64>,
    pub awake_mins: Option<f64>,
    /// Local wall clock, `HH:MM`.
    pub bedtime: Option<String>,
    pub wake_time: Option<String>,
    /// What Garmin reckoned this athlete needed that night.
    pub need_hours: Option<f64>,
    /// Share of time in bed actually spent asleep. Below 85% is worth noting.
    pub efficiency_pct: Option<f64>,
    pub overnight_hrv: Option<f64>,
    /// Measured across the night, so this is the trustworthy kind of resting
    /// HR rather than the daytime estimate — see `recovery`.
    pub resting_hr: Option<f64>,
    pub avg_stress: Option<f64>,
    pub respiration: Option<f64>,
    pub lowest_spo2: Option<f64>,
    pub restless_count: Option<f64>,
    /// The components Garmin itself marked `FAIR` or `POOR` on this night —
    /// `deepPercentage`, `remPercentage`, `awakeCount` and the like. Empty
    /// means Garmin was happy with every part of it.
    pub garmin_flagged: Vec<String>,
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

impl From<garmin_core::sleep::SleepNight> for SleepNightView {
    fn from(n: garmin_core::sleep::SleepNight) -> Self {
        let mins = |s: Option<f64>| s.map(|v| round1(v / 60.0));
        let hours = |s: Option<f64>| s.map(|v| round1(v / 3600.0));
        let clock = |t: &Option<String>| t.as_ref().and_then(|s| s.get(11..16)).map(str::to_string);

        Self {
            hours_asleep: hours(n.total_secs),
            deep_mins: mins(n.deep_secs),
            light_mins: mins(n.light_secs),
            rem_mins: mins(n.rem_secs),
            awake_mins: mins(n.awake_secs),
            bedtime: clock(&n.start_local),
            wake_time: clock(&n.end_local),
            need_hours: hours(n.need_secs),
            efficiency_pct: n.efficiency().map(round1),
            garmin_flagged: n
                .score_parts
                .iter()
                .filter(|p| matches!(p.qualifier.as_deref(), Some("FAIR" | "POOR")))
                .map(|p| p.key.clone())
                .collect(),
            date: n.date,
            score: n.score,
            score_qualifier: n.score_qualifier,
            feedback: n.feedback,
            overnight_hrv: n.avg_overnight_hrv,
            resting_hr: n.resting_hr,
            avg_stress: n.avg_stress,
            respiration: n.avg_respiration,
            lowest_spo2: n.lowest_spo2,
            restless_count: n.restless_count,
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SleepAverages {
    pub nights: usize,
    pub hours_asleep: Option<f64>,
    pub score: Option<f64>,
    pub deep_pct: Option<f64>,
    pub rem_pct: Option<f64>,
    pub efficiency_pct: Option<f64>,
    pub overnight_hrv: Option<f64>,
    /// Typical bedtime and wake time, `HH:MM`, with the spread either side of
    /// them in minutes. The spread is the interesting half: regularity moves
    /// sleep quality more reliably than duration does.
    pub bedtime: Option<String>,
    pub bedtime_swing_mins: Option<f64>,
    pub wake_time: Option<String>,
    pub wake_swing_mins: Option<f64>,
    pub nights_under_seven_hours: usize,
}

/// Minutes past 18:00 back into a clock face, which is how the averages store
/// bedtimes so that midnight doesn't split them.
fn clock_from_mins(mins: f64) -> String {
    let t = (mins.round() as i64 + 18 * 60).rem_euclid(24 * 60);
    format!("{:02}:{:02}", t / 60, t % 60)
}

impl From<garmin_core::sleep::SleepAverages> for SleepAverages {
    fn from(a: garmin_core::sleep::SleepAverages) -> Self {
        Self {
            nights: a.nights,
            hours_asleep: a.total_secs.map(|s| round1(s / 3600.0)),
            score: a.score.map(round1),
            deep_pct: a.deep_pct.map(round1),
            rem_pct: a.rem_pct.map(round1),
            efficiency_pct: a.efficiency.map(round1),
            overnight_hrv: a.overnight_hrv.map(round1),
            bedtime: a.bedtime_mins.map(clock_from_mins),
            bedtime_swing_mins: a.bedtime_sd_mins.map(|m| m.round()),
            wake_time: a.wake_mins.map(clock_from_mins),
            wake_swing_mins: a.wake_sd_mins.map(|m| m.round()),
            nights_under_seven_hours: a.short_nights,
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SleepInsight {
    pub id: String,
    /// `good`, `note` or `watch`.
    pub tone: String,
    pub claim: String,
    pub detail: String,
    /// How many nights the claim rests on.
    pub nights: usize,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SleepReport {
    pub last_night: Option<SleepNightView>,
    /// The window, newest first.
    pub nights: Vec<SleepNightView>,
    pub averages: SleepAverages,
    /// What this window says, computed from the rows rather than written by a
    /// model. Quote these rather than re-deriving them.
    pub insights: Vec<SleepInsight>,
    /// True when hours and scores are cached but the detail behind them isn't
    /// — an unsynced cache, not a run of sleepless nights.
    pub needs_backfill: bool,
}

impl From<garmin_core::sleep::SleepReport> for SleepReport {
    fn from(r: garmin_core::sleep::SleepReport) -> Self {
        Self {
            last_night: r.last_night.map(SleepNightView::from),
            nights: r.nights.into_iter().map(SleepNightView::from).collect(),
            averages: r.averages.into(),
            insights: r
                .insights
                .into_iter()
                .map(|i| SleepInsight {
                    id: i.id,
                    tone: match i.tone {
                        garmin_core::sleep::Tone::Good => "good",
                        garmin_core::sleep::Tone::Note => "note",
                        garmin_core::sleep::Tone::Watch => "watch",
                    }
                    .into(),
                    claim: i.claim,
                    detail: i.detail,
                    nights: i.nights,
                })
                .collect(),
            needs_backfill: r.needs_backfill,
        }
    }
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

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ExerciseCount {
    /// Garmin's category for the movement. Always the watch's guess.
    pub exercise: String,
    pub sets: usize,
    pub reps: i64,
    /// How sure the watch was, 0-100, averaged across those sets.
    pub confidence: f64,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct StrengthSession {
    pub activity_id: i64,
    pub name: Option<String>,
    pub date: Option<String>,
    pub duration_min: Option<f64>,
    pub avg_hr: Option<f64>,
    pub max_hr: Option<f64>,
    /// Work sets. Rest periods are not counted here.
    pub work_sets: usize,
    pub total_reps: i64,
    /// Seconds spent working — the closest thing to time under tension this
    /// data supports.
    pub work_s: f64,
    pub rest_s: f64,
    pub work_rest_ratio: Option<f64>,
    /// Median seconds of rest between work sets, so one long break doesn't
    /// misrepresent the session's pacing.
    pub median_rest_s: Option<f64>,
    pub avg_reps_per_set: Option<f64>,
    /// Movements the watch was confident enough to name. Often empty.
    pub guessed_exercises: Vec<ExerciseCount>,
    /// Work sets with no usable guess.
    pub unlabelled_sets: usize,
}

impl From<garmin_core::StrengthSession> for StrengthSession {
    fn from(s: garmin_core::StrengthSession) -> Self {
        Self {
            activity_id: s.activity_id,
            name: s.name,
            date: s.date,
            duration_min: s.duration_min,
            avg_hr: s.avg_hr,
            max_hr: s.max_hr,
            work_sets: s.work_sets,
            total_reps: s.total_reps,
            work_s: query::round1(s.work_s),
            rest_s: query::round1(s.rest_s),
            work_rest_ratio: s.work_rest_ratio.map(query::round1),
            median_rest_s: s.median_rest_s.map(query::round1),
            avg_reps_per_set: s.avg_reps_per_set.map(query::round1),
            guessed_exercises: s
                .guessed_exercises
                .into_iter()
                .map(|e| ExerciseCount {
                    exercise: e.exercise,
                    sets: e.sets,
                    reps: e.reps,
                    confidence: query::round1(e.confidence),
                })
                .collect(),
            unlabelled_sets: s.unlabelled_sets,
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct StrengthReport {
    pub sessions: Vec<StrengthSession>,
    pub sessions_examined: usize,
    pub avg_work_sets: Option<f64>,
    pub avg_reps: Option<f64>,
    pub median_rest_s: Option<f64>,
    pub labelled_sets: usize,
    pub unlabelled_sets: usize,
    /// Always true on this account: the watch cannot know the load, so there is
    /// no volume in kilograms anywhere in this report and none can be derived.
    pub no_weights_recorded: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct PersonalRecord {
    pub type_id: i64,
    /// What the record measures. Null for a `type_id` this build doesn't
    /// recognise — don't describe those to the athlete, the number alone is
    /// meaningless.
    pub label: Option<String>,
    /// `seconds`, `metres`, `count` or `days`, matching `value`.
    pub unit: Option<String>,
    pub value: f64,
    pub activity_id: Option<i64>,
    pub activity_name: Option<String>,
    pub activity_type: Option<String>,
    pub set_on: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct FitnessReport {
    pub date: Option<String>,
    /// Garmin's phrase for the current status — `RECOVERY_1`, `PRODUCTIVE_1`,
    /// `UNPRODUCTIVE_2` and so on.
    pub status_phrase: Option<String>,
    /// Load over roughly the last week, and over the last month.
    pub acute_load: Option<f64>,
    pub chronic_load: Option<f64>,
    /// Acute over chronic. Under ~0.8 is detraining, 0.8-1.3 productive, over
    /// ~1.5 is where injury risk climbs.
    pub acwr: Option<f64>,
    pub acwr_status: Option<String>,
    /// A month of load split three ways, each against the range Garmin wants it
    /// in. This is the 80/20 question answered in Garmin's own numbers.
    pub aerobic_low: Option<f64>,
    pub aerobic_low_target: Option<[f64; 2]>,
    pub aerobic_high: Option<f64>,
    pub aerobic_high_target: Option<[f64; 2]>,
    pub anaerobic: Option<f64>,
    pub anaerobic_target: Option<[f64; 2]>,
    /// `BALANCED`, `ANAEROBIC_FOCUS`, `LOW_AEROBIC_SHORTAGE`, …
    pub balance_phrase: Option<String>,
    /// True when the month's anaerobic load has passed the top of its target
    /// range — hard work has crowded out easy work.
    pub anaerobic_over_target: bool,
    /// Running VO2 max. Null until an outdoor GPS run exists; a treadmill never
    /// populates it, so a null here is a prompt to suggest one rather than a
    /// sign of poor fitness.
    pub vo2max: Option<f64>,
    pub vo2max_missing: bool,
    /// Predicted finishing times in seconds. Garmin derives these from HR and
    /// pace, so they exist even with no VO2 max — treat them as extrapolation.
    pub race_5k_s: Option<f64>,
    pub race_10k_s: Option<f64>,
    pub race_half_s: Option<f64>,
    pub race_marathon_s: Option<f64>,
    /// Older days, newest first, for describing which way these are moving.
    pub history: Vec<FitnessPoint>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct FitnessPoint {
    pub date: String,
    pub acute_load: Option<f64>,
    pub chronic_load: Option<f64>,
    pub acwr: Option<f64>,
    pub status_phrase: Option<String>,
    pub vo2max: Option<f64>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TagCount {
    pub tag: String,
    pub activities: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema, Default)]
pub struct LimitParams {
    /// How many to return. Defaults to 10.
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TagParams {
    /// The tag to look up. Matched after trimming and lowercasing, the same way
    /// tags are stored.
    pub tag: String,
    /// How many activities to return. Defaults to 20.
    #[serde(default)]
    pub limit: Option<u32>,
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

/// How many recent sessions the analysis compares one activity against.
/// Matches the desktop app, so both produce the same comparison.
const COMPARE_POOL: u32 = 120;

/// The full analysis of one session, from the cache when it's there and from
/// Garmin when it isn't.
///
/// Deliberately the same sequence the desktop app's `activity_analysis` command
/// runs, including writing the result back under the same fingerprint key — so
/// a session analysed in Claude Desktop opens instantly in the app afterwards,
/// and vice versa.
async fn analysis_for(activity_id: Option<i64>) -> anyhow::Result<serde_json::Value> {
    let (activity, tags, key, cached) = {
        let db = Db::open_default()?;
        // No id means the latest session, matching every other tool here.
        let activity = match activity_id {
            Some(id) => db.activity(id)?,
            None => db.recent_activities(1, None)?.into_iter().next(),
        }
        .ok_or_else(|| anyhow::anyhow!("No such activity in the local cache. Run `sync` first."))?;
        let tags = db.activity_tags(activity.activity_id)?;
        let key = garmin_core::analysis::fingerprint(&activity, &tags);
        let cached = db.activity_analysis(activity.activity_id, &key)?;
        (activity, tags, key, cached)
    };
    let id = activity.activity_id;

    if let Some(json) = cached {
        // A stored analysis from an older build may not decode into the current
        // shape. That's a reason to recompute, not to fail.
        if let Ok(a) = garmin_core::analysis::decode(&json) {
            return Ok(serde_json::to_value(a)?);
        }
    }

    // Each of the three is optional: a session Garmin has no samples for still
    // has laps and a zone breakdown worth returning, and being unable to reach
    // Garmin should cost the charts rather than the answer.
    let client = garmin_core::client_from_keyring().ok().flatten();
    let (details, splits, zones) = match client {
        Some(c) => (
            c.activity_details(id, 500).await.ok(),
            c.activity_splits(id).await.ok(),
            c.hr_time_in_zones(id)
                .await
                .ok()
                .and_then(|z| serde_json::to_value(z).ok()),
        ),
        None => (None, None, None),
    };

    let now = chrono::Utc::now().to_rfc3339();
    let db = Db::open_default()?;
    let recent = db.recent_activities(COMPARE_POOL, None)?;
    let analysis = garmin_core::analysis::analyse(
        &activity,
        details.as_ref(),
        splits.as_ref(),
        zones.as_ref(),
        &recent,
        tags,
        &now,
    );

    // Only worth storing when Garmin's samples went into it — caching the
    // degraded offline version would leave the app's charts empty afterwards.
    if details.is_some() {
        if let Ok(json) = serde_json::to_string(&analysis) {
            let _ = db.save_activity_analysis(id, &key, &now, &json);
        }
    }

    Ok(serde_json::to_value(analysis)?)
}

// ----------------------------------------------------------------------- tools

#[tool_router]
impl GarminServer {
    #[tool(
        description = "List recent activities with per-zone HR breakdown, pace, \
                       cadence and training effect. Start here for 'how am I \
                       doing'. Every row carries an `activity_id`, which is how \
                       one session is named to `activity_zones` or \
                       `activity_analysis` — so this is also the first call when \
                       the question is about a particular run and its id isn't \
                       known yet."
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
                       percent in each of zones 1-5, and nothing else. Defaults \
                       to the latest activity. This is the cheap, shallow read — \
                       when the question is about how a session went rather than \
                       what its split was, call `activity_analysis` instead.")]
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

    #[tool(
        description = "Sleep in detail, which `recovery` only summarises: last \
                       night's stage split (deep, light, REM, awake), when they \
                       fell asleep and woke, sleep efficiency, overnight HRV, \
                       resting HR, respiration, SpO2 and restlessness — plus the \
                       window behind it, the typical bedtime and how far it \
                       swings, and which score components Garmin itself marked \
                       short. `insights` are computed from the rows, not \
                       written by a model: quote them rather than re-deriving \
                       the same claim. Call this \
                       for anything about sleep quality, bedtime, stages or why \
                       a night scored badly; `recovery` remains the right call \
                       for the day-by-day recovery picture."
    )]
    async fn sleep(
        &self,
        Parameters(p): Parameters<DaysParams>,
    ) -> Result<Json<SleepReport>, ErrorData> {
        let db = db()?;
        let days = p.days.unwrap_or(30);
        let report = query::sleep(&db, days).map_err(internal)?;
        // The window is capped independently of `days`: thirty nights is
        // already more than any answer quotes, and the averages above them
        // cover the rest of whatever was asked for.
        Ok(Json(report.brief(30).into()))
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
        description = "Body weight over time: every weigh-in, the smoothed trend, \
                       the rate of change per week, BMI, and how the logged \
                       calories compare with what the scale actually did. \
                       Readings the app judged impossible are returned with \
                       `outlier: true` and are excluded from the trend and the \
                       rate — mention them rather than averaging them in."
    )]
    async fn weight(
        &self,
        Parameters(p): Parameters<DaysParams>,
    ) -> Result<Json<serde_json::Value>, ErrorData> {
        let db = db()?;
        let report = query::weight(&db, p.days.unwrap_or(90)).map_err(internal)?;
        Ok(Json(serde_json::to_value(report).map_err(|e| {
            ErrorData::internal_error(e.to_string(), None)
        })?))
    }

    #[tool(
        description = "Strength sessions, set by set. Reports work sets, reps, \
                       time working, rest between sets and the work:rest ratio. \
                       Read the caveats: there is NO load anywhere in this data \
                       — the watch cannot know the weight on the bar, so never \
                       talk about volume in kilograms or progression by weight. \
                       Exercise names are the watch guessing from wrist motion \
                       and are absent for most sets; when one is present, say it \
                       is a guess."
    )]
    async fn strength_sessions(
        &self,
        Parameters(p): Parameters<LimitParams>,
    ) -> Result<Json<StrengthReport>, ErrorData> {
        let db = db()?;
        let r = query::strength_trend(&db, p.limit.unwrap_or(10)).map_err(internal)?;
        Ok(Json(StrengthReport {
            sessions: r.sessions.into_iter().map(StrengthSession::from).collect(),
            sessions_examined: r.sessions_examined,
            avg_work_sets: r.avg_work_sets,
            avg_reps: r.avg_reps,
            median_rest_s: r.median_rest_s,
            labelled_sets: r.labelled_sets,
            unlabelled_sets: r.unlabelled_sets,
            no_weights_recorded: r.no_weights_recorded,
        }))
    }

    #[tool(
        description = "The deep findings — what a year of this history says when \
                       it is asked properly, rather than what one session says. \
                       Covers whether fitness is moving (pace at a fixed heart \
                       rate, this account's stand-in for a VO2 max Garmin will \
                       never compute indoors), what cadence has actually been \
                       worth, which recorded metric moves overnight HRV most, \
                       the easy/hard share over time, which weekday gets skipped, \
                       how rest days differ from training days, and whether a \
                       block of training has quietly stopped.\n\n\
                       Each finding carries `claim` (the sentence), `detail` (the \
                       reasoning), `basis` (what was counted) and usually \
                       `estimate` — a bootstrap interval with `low`, `high` and \
                       `n`. Quote the interval whenever you quote the number: \
                       these rest on a few dozen runs, and a finding only appears \
                       here at all if its interval excludes zero. An empty list \
                       means nothing cleared that bar, which is a normal result \
                       and not an error."
    )]
    async fn findings(
        &self,
        Parameters(p): Parameters<DaysParams>,
    ) -> Result<Json<serde_json::Value>, ErrorData> {
        // A `Value` rather than a mirrored schemars type, for the same reason
        // `activity_analysis` is one: a finding carries an estimate, a list of
        // charted series, a list of table rows and three enums, and the fields
        // present vary by which finding it is. Hand-copying six shapes here
        // would be six more definitions to keep in step with
        // `garmin_core::findings` forever, and the descriptions above are what
        // the model actually reads.
        let db = db()?;
        let days = p.days.unwrap_or(365);
        let daily = db
            .daily_since(&garmin_core::days_ago(days))
            .map_err(internal)?;
        let activities = db
            .activities_since(&garmin_core::days_ago(days))
            .map_err(internal)?;
        let today = chrono::Local::now().date_naive();
        let all = garmin_core::findings::all(&daily, &activities, today);
        Ok(Json(serde_json::to_value(all).map_err(|e| {
            ErrorData::internal_error(e.to_string(), None)
        })?))
    }

    #[tool(description = "Garmin's personal records across every sport: fastest \
                       distances, longest run, step records. A record with a null \
                       `label` is one this build doesn't recognise — skip it \
                       rather than guessing what it measures.")]
    async fn personal_records(&self) -> Result<Json<Vec<PersonalRecord>>, ErrorData> {
        let db = db()?;
        let rows = query::personal_records(&db).map_err(internal)?;
        Ok(Json(
            rows.into_iter()
                .map(|r| PersonalRecord {
                    type_id: r.type_id,
                    label: r.label,
                    unit: r.unit.map(|u| {
                        match u {
                            garmin_core::records::RecordUnit::Seconds => "seconds",
                            garmin_core::records::RecordUnit::Metres => "metres",
                            garmin_core::records::RecordUnit::Count => "count",
                            garmin_core::records::RecordUnit::Days => "days",
                        }
                        .to_string()
                    }),
                    value: r.value,
                    activity_id: r.activity_id,
                    activity_name: r.activity_name,
                    activity_type: r.activity_type,
                    set_on: r.set_on,
                })
                .collect(),
        ))
    }

    #[tool(
        description = "Garmin's own verdict on the training: status, acute and \
                       chronic load, the acute:chronic ratio, the monthly \
                       aerobic/anaerobic load balance against Garmin's target \
                       ranges, VO2 max and race predictions. Use this alongside \
                       `zone_drift` — this is Garmin's answer to 'is the balance \
                       right', and the app's zone arithmetic is a second opinion \
                       on the same question."
    )]
    async fn fitness(
        &self,
        Parameters(p): Parameters<DaysParams>,
    ) -> Result<Json<FitnessReport>, ErrorData> {
        let db = db()?;
        let r = query::fitness(&db, p.days.unwrap_or(30)).map_err(internal)?;
        let latest = r.latest.as_ref();
        let s = latest.map(|d| &d.status);
        let pred = latest.map(|d| &d.predictions);
        let band = |lo: Option<f64>, hi: Option<f64>| Some([lo?, hi?]);

        Ok(Json(FitnessReport {
            date: latest.map(|d| d.date.clone()),
            status_phrase: s.and_then(|s| s.status_phrase.clone()),
            acute_load: s.and_then(|s| s.acute_load),
            chronic_load: s.and_then(|s| s.chronic_load),
            acwr: s.and_then(|s| s.acwr),
            acwr_status: s.and_then(|s| s.acwr_status.clone()),
            aerobic_low: s.and_then(|s| s.aerobic_low),
            aerobic_low_target: s
                .and_then(|s| band(s.aerobic_low_target_min, s.aerobic_low_target_max)),
            aerobic_high: s.and_then(|s| s.aerobic_high),
            aerobic_high_target: s
                .and_then(|s| band(s.aerobic_high_target_min, s.aerobic_high_target_max)),
            anaerobic: s.and_then(|s| s.anaerobic),
            anaerobic_target: s.and_then(|s| band(s.anaerobic_target_min, s.anaerobic_target_max)),
            balance_phrase: s.and_then(|s| s.balance_phrase.clone()),
            anaerobic_over_target: r.anaerobic_over_target,
            vo2max: s.and_then(|s| s.vo2max),
            vo2max_missing: r.vo2max_missing,
            race_5k_s: pred.and_then(|p| p.time_5k_s),
            race_10k_s: pred.and_then(|p| p.time_10k_s),
            race_half_s: pred.and_then(|p| p.time_half_s),
            race_marathon_s: pred.and_then(|p| p.time_marathon_s),
            history: r
                .days
                .into_iter()
                .skip(1)
                .map(|d| FitnessPoint {
                    date: d.date,
                    acute_load: d.status.acute_load,
                    chronic_load: d.status.chronic_load,
                    acwr: d.status.acwr,
                    status_phrase: d.status.status_phrase,
                    vo2max: d.status.vo2max,
                })
                .collect(),
        }))
    }

    #[tool(
        description = "Every label the athlete has put on their own sessions, \
                       with how many carry each. Tags are local to this app and \
                       have no Garmin equivalent, so this is the only way to \
                       learn what groupings they think in."
    )]
    async fn list_tags(&self) -> Result<Json<Vec<TagCount>>, ErrorData> {
        let db = db()?;
        Ok(Json(
            db.all_tags()
                .map_err(internal)?
                .into_iter()
                .map(|(tag, activities)| TagCount { tag, activities })
                .collect(),
        ))
    }

    #[tool(
        description = "Activities carrying one tag, newest first, with the same \
                       zone breakdown `recent_activities` gives. Use after \
                       `list_tags` to compare one kind of session against itself \
                       over time."
    )]
    async fn tagged_activities(
        &self,
        Parameters(p): Parameters<TagParams>,
    ) -> Result<Json<Vec<ActivityView>>, ErrorData> {
        let db = db()?;
        let rows = db
            .activities_with_tag(&p.tag, p.limit.unwrap_or(20))
            .map_err(internal)?;
        Ok(Json(
            rows.iter()
                .map(query::ActivityView::from)
                .map(ActivityView::from)
                .collect(),
        ))
    }

    #[tool(description = "Everything one session holds, read closely: the lap \
                       splits, the HR/pace/cadence series, drift within the run, \
                       how it compares with recent sessions, and the specific \
                       moments worth noticing with where in the session they \
                       happened. This is far richer than `activity_zones` and is \
                       the right tool when asked about one run in detail. \
                       Computing it costs three Garmin requests the first time; \
                       afterwards it is served from the cache.")]
    async fn activity_analysis(
        &self,
        Parameters(p): Parameters<ActivityParams>,
    ) -> Result<Json<serde_json::Value>, ErrorData> {
        // Returned as a Value rather than mirrored into a schemars type: the
        // analysis is a large, deeply nested shape whose fields vary by sport,
        // and a hand-copied mirror of it would be a second definition to keep
        // in step with `garmin_core::analysis` forever.
        let handle = tokio::runtime::Handle::current();
        let value = tokio::task::spawn_blocking(move || -> anyhow::Result<serde_json::Value> {
            handle.block_on(analysis_for(p.activity_id))
        })
        .await
        .map_err(|e| ErrorData::internal_error(format!("analysis task panicked: {e}"), None))?
        .map_err(internal)?;
        Ok(Json(value))
    }

    #[tool(
        description = "What the app's own coach would say unprompted today, and \
                       where the current week stands against the athlete's goals. \
                       Each nudge carries the numbers behind it in `evidence`, and \
                       `daysRunning` says how long it has been standing. An empty \
                       list means nothing is worth raising — that is the normal \
                       state, not an error. Good for opening a check-in, or for \
                       finding out what the athlete has already been told."
    )]
    async fn coach(&self) -> Result<Json<serde_json::Value>, ErrorData> {
        let db = db()?;
        let today = chrono::Local::now().date_naive();
        let report = garmin_core::coach::for_today(&db, today).map_err(internal)?;
        Ok(Json(serde_json::to_value(report).map_err(|e| {
            ErrorData::internal_error(e.to_string(), None)
        })?))
    }

    #[tool(
        description = "What's in the local cache, when it was last refreshed, and \
                       how current the data itself is. Those last two are \
                       different questions — check `stale` and `days_since_daily` \
                       before describing anything as recent."
    )]
    async fn cache_status(&self) -> Result<Json<CacheStatus>, ErrorData> {
        let db = db()?;
        let s = query::cache_status(&db).map_err(internal)?;
        Ok(Json(CacheStatus {
            activities_cached: s.activities_cached,
            last_sync: s.last_sync,
            database_path: s.database_path,
            connected_to_garmin: s.connected_to_garmin,
            newest_activity_date: s.newest_activity_date,
            days_since_activity: s.days_since_activity,
            newest_daily_date: s.newest_daily_date,
            days_since_daily: s.days_since_daily,
            stale: s.stale,
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
            sets_written: report.sets_written,
            records_written: report.records_written,
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
             Not every recorded number is a measurement. Each activity carries \
             `hr_confidence` (`good`/`caution`/`poor`) for its zone split, with \
             `hr_caveats` giving the reason, and `cadence_lock` for the specific \
             failure where the wrist sensor reads arm swing as pulse. That one \
             matters most here: a locked reading at 170 steps per minute reports \
             170 bpm, which is this athlete's Z4 — so the hard-effort drift this \
             account is coached on is exactly what the artefact counterfeits. \
             Keep flagged sessions in the totals, say the caveat when you lean \
             on one, and never build a drift argument on `poor` sessions alone. \
             `pace_estimated` marks sessions whose distance and pace came off \
             the arm accelerometer rather than GPS; heart rate and cadence there \
             are still fine, but two estimated paces do not make a comparison. \
             `moving_pace_min_per_km` excludes walk breaks where the other pace \
             does not.\n\n\
             In `recovery`, `resting_hr_source` says which of Garmin's two \
             resting-heart-rate measurements a day holds. Only `overnight` \
             values belong on one trend line; a `daytimeEstimate` is a rougher \
             figure from the waking day, and a step between the two kinds is not \
             a change in fitness.\n\n\
             Strength sessions carry no load. `strength_sessions` reports reps, \
             set durations and rest, because that is what the watch records — \
             the weight on the bar is not in this data and cannot be inferred, \
             so don't discuss volume in kilograms or progression by weight. \
             Exercise names there are the watch guessing from wrist motion and \
             are missing for most sets; name one only as a guess.\n\n\
             VO2 max is null on this account because it is only computed from \
             outdoor GPS runs and the athlete runs on a treadmill. That is a \
             missing input, not poor fitness.\n\n\
             All tools read cached data, and every window is by date, so a tool \
             that returns nothing means nothing was recorded in that window — \
             not that the athlete did nothing. The two are told apart by \
             `cache_status`: `last_sync` is when the app last asked Garmin, \
             which moves even when a watch left in a drawer had nothing to give, \
             while `days_since_daily` is how old the data actually is. When \
             `stale` is set, say so and give the date the data stops, rather \
             than describing the newest reading as current — and call `sync` \
             before assuming anything is missing."
                .into(),
        );
        info
    }
}
