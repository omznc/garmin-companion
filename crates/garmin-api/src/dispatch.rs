//! The JSON entry point for the mobile bridge.
//!
//! Command names and argument names match the Tauri commands that `api.ts`
//! wraps, so the result JSON has the same shape as before. `today` is new.

use crate::{activities_view, activity_detail_view, body, open_default_db, today_view, ApiError};
use garmin_core::{days_ago, query};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Days {
    days: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Recent {
    limit: Option<u32>,
    type_key: Option<String>,
}

#[derive(Deserialize)]
struct Since {
    from: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActivityId {
    activity_id: i64,
}

fn parse<T: DeserializeOwned>(args: &str) -> Result<T, ApiError> {
    // A caller with no arguments may send an empty string.
    let json = if args.trim().is_empty() { "{}" } else { args };
    serde_json::from_str(json).map_err(|e| ApiError::BadArguments(e.to_string()))
}

fn encode<T: Serialize>(v: &T) -> Result<String, ApiError> {
    serde_json::to_string(v).map_err(|e| ApiError::Internal(e.to_string()))
}

/// Run one command and return its result as JSON.
///
/// `args_json` is a JSON object, or an empty string for no arguments. Commands:
///
/// - `today`: the full Today view model.
/// - `health_view`: the Health screen, one trend for each metric the watch records.
/// - `sleep_view`: the Sleep screen, last night and the 7 and 30 night windows.
/// - `cached_daily` `{days}`: daily metrics, newest first.
/// - `cached_activities` `{limit, typeKey}`: recent activities, newest first.
/// - `cached_activities_since` `{from}`: activities on or after `YYYY-MM-DD`.
/// - `cached_activity` `{activityId}`: one activity or `null`.
/// - `nutrition` `{days}`: calories in against calories out.
/// - `weight` `{days}`: weigh-ins, trend and energy cross-check.
/// - `activities` `{limit}`: the Activities list view model, grouped by day.
/// - `activity_detail` `{activityId}`: the detail view model of one session, or `null`.
/// - `weight_view`, `food_view`, `strength_view`: the Weight, Food and Strength screens.
///
/// It opens a fresh database handle per call, like the Tauri commands do.
pub fn call(cmd: &str, args_json: &str) -> Result<String, ApiError> {
    match cmd {
        "today" => encode(&today_view(&open_default_db()?, garmin_core::today())?),
        "cached_daily" => {
            let a: Days = parse(args_json)?;
            let db = open_default_db()?;
            encode(&db.daily_since(&days_ago(a.days.unwrap_or(30)))?)
        }
        "cached_activities" => {
            let a: Recent = parse(args_json)?;
            let db = open_default_db()?;
            encode(&db.recent_activities(a.limit.unwrap_or(30), a.type_key.as_deref())?)
        }
        "cached_activities_since" => {
            let a: Since = parse(args_json)?;
            encode(&open_default_db()?.activities_since(&a.from)?)
        }
        "cached_activity" => {
            let a: ActivityId = parse(args_json)?;
            encode(&open_default_db()?.activity(a.activity_id)?)
        }
        "nutrition" => {
            let a: Days = parse(args_json)?;
            encode(&query::nutrition(
                &open_default_db()?,
                a.days.unwrap_or(30),
            )?)
        }
        "weight" => {
            let a: Days = parse(args_json)?;
            encode(&query::weight(&open_default_db()?, a.days.unwrap_or(180))?)
        }
        "activities" => {
            let a: Recent = parse(args_json)?;
            let db = open_default_db()?;
            let today = garmin_core::today();
            encode(&activities_view(&db, today, a.limit.unwrap_or(60))?)
        }
        "activity_detail" => {
            let a: ActivityId = parse(args_json)?;
            encode(&activity_detail_view(&open_default_db()?, a.activity_id)?)
        }
        "health_view" => encode(&crate::health_view(
            &open_default_db()?,
            garmin_core::today(),
        )?),
        "sleep_view" => encode(&crate::sleep_view(
            &open_default_db()?,
            garmin_core::today(),
        )?),
        "weight_view" => encode(&body::weight_view(
            &open_default_db()?,
            garmin_core::today(),
        )?),
        "food_view" => encode(&body::food_view(&open_default_db()?, garmin_core::today())?),
        "strength_view" => encode(&body::strength_view(
            &open_default_db()?,
            garmin_core::today(),
        )?),
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
        let _ = std::fs::remove_dir_all(dir);
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
    }

    #[test]
    fn error_serializes_with_a_kind() {
        let json = serde_json::to_string(&ApiError::UnknownCommand("x".into())).unwrap();
        assert_eq!(json, r#"{"kind":"unknown_command","message":"x"}"#);
    }
}
