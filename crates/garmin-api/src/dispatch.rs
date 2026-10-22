//! The JSON entry point for the mobile bridge.
//!
//! Command names and argument names match the Tauri commands that `api.ts`
//! wrapped, so the result JSON has the same shape as before. The desktop app
//! calls the typed functions of this crate and does not use this module.
//!
//! [`crate::commands::COMMANDS`] lists every command with its argument and
//! result types. The TypeScript `Commands` map is generated from that list.

use crate::export::{write_export, Kind};
use crate::settings::{self, ThemeMode};
use crate::{
    activities_view, activity_detail_view, body, open_default_db, session, today_view, ApiError,
};
use garmin_core::{days_ago, query, Db};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Where a command reads from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The cache and the Garmin account of the person.
    Live,
    /// Recorded data, for an app with no account. No command opens the cache.
    Sample,
}

impl Source {
    /// `Sample` when `sample` is true.
    pub fn sample(sample: bool) -> Self {
        if sample {
            Self::Sample
        } else {
            Self::Live
        }
    }
}

/// The arguments of `today`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
pub struct TodayArgs {
    /// Demo only: the day to show, as `YYYY-MM-DD`, for a cache that is not current.
    #[cfg_attr(feature = "ts-rs", ts(optional))]
    date: Option<String>,
}

/// The arguments of the commands that read a window of days.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DaysArgs {
    #[cfg_attr(feature = "ts-rs", ts(optional))]
    days: Option<u32>,
}

/// The longest window that a command reads. A larger value would overflow the
/// date maths.
const MAX_DAYS: u32 = 3650;

impl DaysArgs {
    fn days_or(&self, default: u32) -> u32 {
        self.days.unwrap_or(default).clamp(1, MAX_DAYS)
    }
}

/// The arguments of `cached_activities` and `activities`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct RecentArgs {
    #[cfg_attr(feature = "ts-rs", ts(optional))]
    limit: Option<u32>,
    /// Only `cached_activities` reads this.
    #[cfg_attr(feature = "ts-rs", ts(optional))]
    type_key: Option<String>,
}

/// The arguments of `report_view`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
pub struct OffsetArgs {
    /// 0 is the newest week.
    #[cfg_attr(feature = "ts-rs", ts(optional, type = "number"))]
    offset: Option<usize>,
}

/// The arguments of `cached_activities_since`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
pub struct SinceArgs {
    /// `YYYY-MM-DD`.
    from: String,
}

/// The arguments of `cached_activity` and `activity_detail`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ActivityIdArgs {
    activity_id: i64,
}

/// The arguments of `routes_view`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
pub struct RoutesArgs {
    #[cfg_attr(feature = "ts-rs", ts(optional))]
    sort: Option<crate::routes::RouteSort>,
}

fn parse<T: DeserializeOwned>(args: &str) -> Result<T, ApiError> {
    // A caller with no arguments may send an empty string.
    let json = if args.trim().is_empty() { "{}" } else { args };
    serde_json::from_str(json).map_err(|e| ApiError::BadArguments(e.to_string()))
}

fn encode<T: Serialize>(v: &T) -> Result<String, ApiError> {
    serde_json::to_string(v).map_err(|e| ApiError::Internal(e.to_string()))
}

/// Parse the arguments, run the command and encode the result.
fn run<A: DeserializeOwned, T: Serialize>(
    args: &str,
    f: impl FnOnce(A) -> Result<T, ApiError>,
) -> Result<String, ApiError> {
    encode(&f(parse(args)?)?)
}

/// A view of the cache for one day. `args` is not read.
fn view<T: Serialize>(
    f: impl FnOnce(&Db, chrono::NaiveDate) -> Result<T, ApiError>,
) -> Result<String, ApiError> {
    encode(&f(&open_default_db()?, garmin_core::today())?)
}

/// Run one command on live data and return its result as JSON.
///
/// `args_json` is a JSON object, or an empty string for no arguments. Read
/// [`crate::commands::COMMANDS`] for the commands and their types.
///
/// It opens a fresh database handle per call, like the Tauri commands do.
pub fn call(cmd: &str, args_json: &str) -> Result<String, ApiError> {
    call_with(Source::Live, cmd, args_json)
}

/// Run one command on `source` and return its result as JSON.
///
/// In sample mode a command returns its recorded view, and a command that
/// needs the cache returns an error. The account commands work in both modes.
pub fn call_with(source: Source, cmd: &str, args_json: &str) -> Result<String, ApiError> {
    if let Some(out) = export_call(source, cmd, args_json) {
        return out;
    }
    if source == Source::Sample {
        let args = serde_json::from_str(args_json).unwrap_or_default();
        if let Some(out) = crate::sample::call(cmd, &args) {
            return out;
        }
    }
    cache_call(cmd, args_json)
}

/// The arguments of `export_csv`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
pub struct ExportCsvArgs {
    /// The folder to write into. The app passes its cache folder.
    dir: String,
}

/// The period of `export_report`.
#[derive(Deserialize, Clone, Copy)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum ExportPeriod {
    Week,
    Month,
}

/// The arguments of `export_report`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
pub struct ExportReportArgs {
    /// The folder to write into. The app passes its cache folder.
    dir: String,
    period: ExportPeriod,
    /// 0 is the newest week, or the month of today.
    #[cfg_attr(feature = "ts-rs", ts(optional))]
    offset: Option<u32>,
}

/// The commands that write a file. They return the path, not the bytes. Sample
/// mode exports the sample data.
fn export_call(source: Source, cmd: &str, args: &str) -> Option<Result<String, ApiError>> {
    Some(match cmd {
        "export_csv" => run(args, |a: ExportCsvArgs| {
            write_export(source, &a.dir, Kind::Csv)
        }),
        "export_report" => run(args, |a: ExportReportArgs| {
            let offset = a.offset.unwrap_or(0);
            let kind = match a.period {
                ExportPeriod::Week => Kind::Week(offset as usize),
                ExportPeriod::Month => Kind::Month(offset),
            };
            write_export(source, &a.dir, kind)
        }),
        _ => return None,
    })
}

/// The commands that read the cache, one value or one window.
fn cache_call(cmd: &str, args: &str) -> Result<String, ApiError> {
    match cmd {
        "cached_daily" => run(args, |a: DaysArgs| {
            Ok(open_default_db()?.daily_since(&days_ago(a.days_or(30)))?)
        }),
        "cached_activities" => run(args, |a: RecentArgs| {
            let db = open_default_db()?;
            Ok(db.recent_activities(a.limit.unwrap_or(30), a.type_key.as_deref())?)
        }),
        "cached_activities_since" => run(args, |a: SinceArgs| {
            Ok(open_default_db()?.activities_since(&a.from)?)
        }),
        "cached_activity" => run(args, |a: ActivityIdArgs| {
            Ok(open_default_db()?.activity(a.activity_id)?)
        }),
        "nutrition" => run(args, |a: DaysArgs| {
            Ok(query::nutrition(&open_default_db()?, a.days_or(30))?)
        }),
        "weight" => run(args, |a: DaysArgs| {
            Ok(query::weight(&open_default_db()?, a.days_or(180))?)
        }),
        other => screen_call(other, args),
    }
}

/// The Today, Activities, Health, Sleep and body screens.
fn screen_call(cmd: &str, args: &str) -> Result<String, ApiError> {
    match cmd {
        "today" => run(args, |a: TodayArgs| {
            let day = match a.date {
                Some(d) => chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d")
                    .map_err(|_| ApiError::BadArguments("date must be YYYY-MM-DD".into()))?,
                None => garmin_core::today(),
            };
            today_view(&open_default_db()?, day)
        }),
        "activities" => run(args, |a: RecentArgs| {
            let today = garmin_core::today();
            activities_view(&open_default_db()?, today, a.limit.unwrap_or(60))
        }),
        "activity_detail" => run(args, |a: ActivityIdArgs| {
            activity_detail_view(&open_default_db()?, a.activity_id)
        }),
        "health_view" => view(crate::health_view),
        "sleep_view" => view(crate::sleep_view),
        "weight_view" => view(body::weight_view),
        "food_view" => view(body::food_view),
        "strength_view" => view(body::strength_view),
        other => training_call(other, args),
    }
}

/// The Plan, Routes, Gear, Insights, Fitness, Report and chart screens.
fn training_call(cmd: &str, args: &str) -> Result<String, ApiError> {
    match cmd {
        "plan_view" => view(crate::plan::plan_view),
        "routes_view" => run(args, |a: RoutesArgs| {
            crate::routes::routes_view(&open_default_db()?, a.sort.unwrap_or_default())
        }),
        // This one calls Garmin: the cache holds no gear. It blocks.
        "gear_view" => encode(&crate::gear::gear_view(&crate::gear::fetch_gear()?)),
        "insights_view" => view(crate::insights::insights_view),
        "fitness_view" => view(crate::fitness::fitness_view),
        "report_view" => run(args, |a: OffsetArgs| {
            let today = garmin_core::today();
            crate::reports::report_view(&open_default_db()?, today, a.offset.unwrap_or(0))
        }),
        "tiles_view" => view(crate::tiles::tiles_view),
        "extras_view" => view(crate::extras::extras_view),
        "charts_view" => view(crate::charts::charts_view),
        other => account_call(other, args),
    }
}

/// The arguments of `redeem_ticket`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
pub struct TicketArgs {
    ticket: String,
}

/// The arguments of `import_tokens`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
pub struct ImportArgs {
    /// The token file. Empty means the default place.
    #[cfg_attr(feature = "ts-rs", ts(optional))]
    path: Option<String>,
}

/// The arguments of `set_theme`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
pub struct ThemeArgs {
    theme: ThemeMode,
}

/// The arguments of `set_nav_order`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct NavOrderArgs {
    nav_order: Vec<String>,
    default_screen: String,
}

/// The arguments of `set_palettes`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PalettesArgs {
    theme_light: String,
    theme_dark: String,
}

/// The sign-in, sign-out and settings commands. `sync` is not here, because
/// it sends events. The bridge host runs `session::sync` itself.
fn account_call(cmd: &str, args: &str) -> Result<String, ApiError> {
    match cmd {
        "status" => encode(&session::status()?),
        "sign_in_url" => encode(&session::sign_in_url()),
        "redeem_ticket" => run(args, |a: TicketArgs| {
            session::redeem_ticket(&a.ticket)?;
            session::status()
        }),
        "import_tokens" => run(args, |a: ImportArgs| {
            session::import_tokens(a.path.as_deref())?;
            session::status()
        }),
        "sign_out" => {
            session::sign_out()?;
            encode(&session::status()?)
        }
        "settings" => encode(&settings::settings_view()?),
        "set_theme" => run(args, |a: ThemeArgs| settings::set_theme(a.theme)),
        "set_nav_order" => run(args, |a: NavOrderArgs| {
            settings::set_nav_order(a.nav_order, a.default_screen)
        }),
        "set_palettes" => run(args, |a: PalettesArgs| {
            settings::set_palettes(a.theme_light, a.theme_dark)
        }),
        other => Err(ApiError::UnknownCommand(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_command_is_an_error() {
        assert!(matches!(
            call("nope", ""),
            Err(ApiError::UnknownCommand(c)) if c == "nope"
        ));
    }

    #[test]
    fn commands_return_json_from_the_base_dir() {
        let dir = std::env::temp_dir().join(format!("garmin-api-dispatch-{}", std::process::id()));
        crate::init(&dir);
        let today: serde_json::Value = serde_json::from_str(&call("today", "").unwrap()).unwrap();
        assert_eq!(today["empty"], true);
        let rows = call("cached_activities", r#"{"limit":1,"typeKey":"running"}"#).unwrap();
        assert_eq!(rows, "[]");
        // A huge window must not panic in the date maths.
        for cmd in ["cached_daily", "nutrition", "weight"] {
            assert!(call(cmd, r#"{"days":4294967295}"#).is_ok(), "{cmd}");
        }
        for cmd in ["tiles_view", "extras_view", "charts_view"] {
            let view: serde_json::Value = serde_json::from_str(&call(cmd, "").unwrap()).unwrap();
            assert!(view.is_object(), "{cmd}");
        }
        // An empty cache still exports a header-only CSV and a valid PDF.
        let out = dir.join("out");
        let args = serde_json::json!({ "dir": out }).to_string();
        let csv: String = serde_json::from_str(&call("export_csv", &args).unwrap()).unwrap();
        assert!(std::fs::read_to_string(csv)
            .unwrap()
            .starts_with("date,name"));
        let args = serde_json::json!({ "dir": out, "period": "week" }).to_string();
        let pdf: String = serde_json::from_str(&call("export_report", &args).unwrap()).unwrap();
        assert!(std::fs::read(pdf).unwrap().starts_with(b"%PDF"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sample_mode_exports_the_sample_data_into_the_folder() {
        let dir =
            std::env::temp_dir().join(format!("garmin-api-export-cmd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = |cmd: &str, args: serde_json::Value| -> std::path::PathBuf {
            let out = call_with(Source::Sample, cmd, &args.to_string()).unwrap();
            serde_json::from_str::<String>(&out).unwrap().into()
        };
        let csv = path("export_csv", serde_json::json!({ "dir": dir }));
        assert_eq!(csv, dir.join("garmin-activities.csv"));
        assert!(std::fs::read_to_string(&csv).unwrap().lines().count() > 1);
        let week = path(
            "export_report",
            serde_json::json!({ "dir": dir, "period": "week" }),
        );
        let month = path(
            "export_report",
            serde_json::json!({ "dir": dir, "period": "month", "offset": 1 }),
        );
        assert!(week
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("garmin-week-2026-"));
        assert_eq!(month, dir.join("garmin-2026-02.pdf"));
        for pdf in [week, month] {
            assert!(std::fs::read(pdf).unwrap().starts_with(b"%PDF"));
        }
        // The scratch cache of the sample is gone, and the files are all that is left.
        assert!(!dir.join("sample-export").exists());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 3);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_export_checks_its_arguments() {
        for args in [r#"{"dir":""}"#, r#"{"dir":" "}"#, "{}"] {
            let out = call_with(Source::Sample, "export_csv", args);
            assert!(matches!(out, Err(ApiError::BadArguments(_))), "{args}");
        }
        let bad = call_with(
            Source::Sample,
            "export_report",
            r#"{"dir":"x","period":"year"}"#,
        );
        assert!(matches!(bad, Err(ApiError::BadArguments(_))));
        let dir =
            std::env::temp_dir().join(format!("garmin-api-export-huge-{}", std::process::id()));
        let huge = serde_json::json!({ "dir": dir, "period": "month", "offset": u32::MAX });
        let out = call_with(Source::Sample, "export_report", &huge.to_string());
        assert!(matches!(out, Err(ApiError::BadArguments(_))));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn days_are_clamped_to_a_sane_range() {
        let days = |d| DaysArgs { days: d }.days_or(30);
        assert_eq!(
            (days(None), days(Some(0)), days(Some(u32::MAX))),
            (30, 1, MAX_DAYS)
        );
    }

    #[test]
    fn bad_arguments_are_rejected() {
        assert!(matches!(
            call("cached_activity", r#"{"activityId":"x"}"#),
            Err(ApiError::BadArguments(_))
        ));
        assert!(matches!(
            call("cached_activities_since", "{}"),
            Err(ApiError::BadArguments(_))
        ));
        assert!(matches!(
            call("today", r#"{"date":"soon"}"#),
            Err(ApiError::BadArguments(_))
        ));
    }

    #[test]
    fn sample_mode_returns_the_recording_and_refuses_the_cache() {
        let today = call_with(Source::Sample, "today", "{}").unwrap();
        assert!(today.contains("\"lede\""));
        assert!(matches!(
            call_with(Source::Sample, "cached_daily", "{}"),
            Err(ApiError::BadArguments(_))
        ));
        assert!(matches!(
            call_with(Source::Sample, "nope", "{}"),
            Err(ApiError::UnknownCommand(_))
        ));
    }

    #[test]
    fn account_commands_check_their_arguments() {
        let url: String = serde_json::from_str(&call("sign_in_url", "").unwrap()).unwrap();
        assert!(url.contains("sso.garmin.com"));
        assert!(matches!(
            call("set_theme", r#"{"theme":"pink"}"#),
            Err(ApiError::BadArguments(_))
        ));
        assert!(matches!(
            call("redeem_ticket", "{}"),
            Err(ApiError::BadArguments(_))
        ));
        assert!(matches!(
            call("set_nav_order", "{}"),
            Err(ApiError::BadArguments(_))
        ));
    }

    #[test]
    fn error_serializes_with_a_kind() {
        let json = serde_json::to_string(&ApiError::UnknownCommand("x".into())).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"unknown_command","message":"unknown command: x"}"#
        );
    }
}
