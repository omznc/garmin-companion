//! The JSON entry point for the mobile bridge.
//!
//! Command names and argument names match the Tauri commands that `api.ts`
//! wrapped, so the result JSON has the same shape as before. The desktop app
//! calls the typed functions of this crate and does not use this module.
//!
//! [`crate::commands::COMMANDS`] lists every command with its argument and
//! result types. The TypeScript `Commands` map is generated from that list.

use crate::focus::{clip_activities, Focus};
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

/// The day or the range that a screen looks at. Both dates are `YYYY-MM-DD`.
/// With no date, a command reads the live day.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
pub struct FocusArgs {
    /// The first day of a range. It defaults to `to`.
    #[cfg_attr(feature = "ts-rs", ts(optional))]
    from: Option<String>,
    /// The last day. It defaults to the real today.
    #[cfg_attr(feature = "ts-rs", ts(optional))]
    to: Option<String>,
}

impl FocusArgs {
    fn focus(&self) -> Result<Option<Focus>, ApiError> {
        Focus::parse(self.from.as_deref(), self.to.as_deref())
    }
}

/// The arguments of `activities`: a focus and the most day groups to read.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
pub struct ActivitiesArgs {
    #[cfg_attr(feature = "ts-rs", ts(optional))]
    limit: Option<u32>,
    #[cfg_attr(feature = "ts-rs", ts(optional))]
    from: Option<String>,
    #[cfg_attr(feature = "ts-rs", ts(optional))]
    to: Option<String>,
}

/// The arguments of `cached_activities` and `cached_activities_since`.
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

/// The arguments of `claim_notification`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
pub struct ClaimArgs {
    /// The `kind` of a Today note.
    kind: String,
    /// The local day, `YYYY-MM-DD`.
    date: String,
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

/// A view of the cache. It ends on the last day of the focus in `args`, or on
/// the real today. The read hides the rows after that day.
fn view<T: Serialize>(
    args: &str,
    f: impl FnOnce(&Db, chrono::NaiveDate) -> Result<T, ApiError>,
) -> Result<String, ApiError> {
    let a: FocusArgs = parse(args)?;
    let end = a.focus()?.map_or_else(garmin_core::today, |f| f.end);
    let db = open_default_db()?;
    encode(&garmin_core::with_as_of(end, || f(&db, end))?)
}

/// Like `view`, for a view that clips its series to a range.
fn ranged_view<T: Serialize>(
    args: &str,
    f: impl FnOnce(&Db, Focus) -> Result<T, ApiError>,
) -> Result<String, ApiError> {
    let a: FocusArgs = parse(args)?;
    let focus = a
        .focus()?
        .unwrap_or_else(|| Focus::day(garmin_core::today()));
    let db = open_default_db()?;
    encode(&garmin_core::with_as_of(focus.end, || f(&db, focus))?)
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
    if source == Source::Sample {
        let args = serde_json::from_str(args_json).unwrap_or_default();
        if let Some(out) = crate::sample::call(cmd, &args) {
            return out;
        }
    }
    cache_call(cmd, args_json)
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
        "claim_notification" => run(args, |a: ClaimArgs| {
            chrono::NaiveDate::parse_from_str(&a.date, "%Y-%m-%d")
                .map_err(|_| ApiError::BadArguments("date must be YYYY-MM-DD".into()))?;
            if a.kind.is_empty() || a.kind.len() > 64 {
                return Err(ApiError::BadArguments(
                    "kind must be 1-64 characters".into(),
                ));
            }
            Ok(open_default_db()?.claim_note_notification(&a.kind, &a.date)?)
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
            let db = open_default_db()?;
            garmin_core::with_as_of(day, || today_view(&db, day))
        }),
        "activities" => run(args, |a: ActivitiesArgs| {
            let focus = Focus::parse(a.from.as_deref(), a.to.as_deref())?;
            let today = focus.map_or_else(garmin_core::today, |f| f.end);
            let db = open_default_db()?;
            let mut view = garmin_core::with_as_of(today, || {
                activities_view(&db, today, a.limit.unwrap_or(60))
            })?;
            if let Some(f) = focus {
                clip_activities(&mut view, f);
            }
            Ok(view)
        }),
        "activity_detail" => run(args, |a: ActivityIdArgs| {
            activity_detail_view(&open_default_db()?, a.activity_id)
        }),
        "health_view" => ranged_view(args, crate::health::health_view_in),
        "sleep_view" => ranged_view(args, crate::sleep_view::sleep_view_in),
        "weight_view" => view(args, body::weight_view),
        "food_view" => view(args, body::food_view),
        "strength_view" => view(args, body::strength_view),
        other => training_call(other, args),
    }
}

/// The Plan, Routes, Gear, Insights, Fitness, Report and chart screens.
fn training_call(cmd: &str, args: &str) -> Result<String, ApiError> {
    match cmd {
        "plan_view" => view(args, crate::plan::plan_view),
        "routes_view" => run(args, |a: RoutesArgs| {
            crate::routes::routes_view(&open_default_db()?, a.sort.unwrap_or_default())
        }),
        // This one calls Garmin: the cache holds no gear. It blocks.
        "gear_view" => encode(&crate::gear::gear_view(&crate::gear::fetch_gear()?)),
        "insights_view" => view(args, crate::insights::insights_view),
        "fitness_view" => view(args, crate::fitness::fitness_view),
        "report_view" => run(args, |a: OffsetArgs| {
            let today = garmin_core::today();
            crate::reports::report_view(&open_default_db()?, today, a.offset.unwrap_or(0))
        }),
        "tiles_view" => ranged_view(args, crate::tiles::tiles_view_in),
        "extras_view" => view(args, crate::extras::extras_view),
        "charts_view" => view(args, crate::charts::charts_view),
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

/// The arguments of `set_tab_order`. An empty list and an empty string reset.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TabOrderArgs {
    tab_order: Vec<String>,
    start_tab: String,
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
        "prefs" => encode(&settings::prefs()),
        "set_tab_order" => run(args, |a: TabOrderArgs| {
            settings::set_tab_order(a.tab_order, a.start_tab)
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
        let week = r#"{"from":"2026-03-05","to":"2026-03-11"}"#;
        for cmd in ["tiles_view", "extras_view", "charts_view", "sleep_view"] {
            for args in ["", week] {
                let view: serde_json::Value =
                    serde_json::from_str(&call(cmd, args).unwrap()).unwrap();
                assert!(view.is_object(), "{cmd}");
            }
        }
        let list = call("activities", week).unwrap();
        assert!(list.contains(r#""empty":true"#));
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
        for cmd in ["health_view", "tiles_view", "activities"] {
            assert!(
                matches!(
                    call(cmd, r#"{"to":"soon"}"#),
                    Err(ApiError::BadArguments(_))
                ),
                "{cmd}"
            );
        }
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
        // A bad tab list is refused before the prefs file is written.
        for args in [
            "{}",
            r#"{"tabOrder":["today"],"startTab":""}"#,
            r#"{"tabOrder":[],"startTab":"gear"}"#,
        ] {
            assert!(matches!(
                call("set_tab_order", args),
                Err(ApiError::BadArguments(_))
            ));
        }
    }

    #[test]
    fn claim_notification_checks_its_arguments_before_the_cache() {
        for bad in [
            r#"{"kind":"hrv","date":"soon"}"#,
            r#"{"kind":"","date":"2026-03-01"}"#,
            r#"{"date":"2026-03-01"}"#,
        ] {
            assert!(matches!(
                call("claim_notification", bad),
                Err(ApiError::BadArguments(_))
            ));
        }
        assert!(matches!(
            call_with(Source::Sample, "claim_notification", "{}"),
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
