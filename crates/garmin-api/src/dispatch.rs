//! The JSON entry point for the mobile bridge.
//!
//! Command names and argument names match the Tauri commands that `api.ts`
//! wraps, so the result JSON has the same shape as before. `today` is new.

use crate::settings::{self, ThemeMode};
use crate::{open_default_db, session, today_view, ApiError};
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
/// - `cached_daily` `{days}`: daily metrics, newest first.
/// - `cached_activities` `{limit, typeKey}`: recent activities, newest first.
/// - `cached_activities_since` `{from}`: activities on or after `YYYY-MM-DD`.
/// - `cached_activity` `{activityId}`: one activity or `null`.
/// - `nutrition` `{days}`: calories in against calories out.
/// - `weight` `{days}`: weigh-ins, trend and energy cross-check.
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
        other => account_call(other, args_json),
    }
}

#[derive(Deserialize)]
struct Ticket {
    ticket: String,
}

#[derive(Deserialize)]
struct Path {
    path: Option<String>,
}

#[derive(Deserialize)]
struct Theme {
    theme: ThemeMode,
}

/// The sign-in, sign-out and settings commands. `sync` is not here, because
/// it sends events. The bridge host runs `session::sync` itself.
///
/// - `status`: `{connected, importableTokenPath}`.
/// - `sign_in_url`: the page to show in a browser view.
/// - `redeem_ticket` `{ticket}`: trade a ticket for tokens.
/// - `import_tokens` `{path?}`: adopt a `garminconnect` token file.
/// - `sign_out`: remove the tokens.
/// - `settings`: the Settings view model.
/// - `set_theme` `{theme}`: `system`, `light` or `dark`.
fn account_call(cmd: &str, args_json: &str) -> Result<String, ApiError> {
    match cmd {
        "status" => encode(&session::status()?),
        "sign_in_url" => encode(&session::sign_in_url()),
        "redeem_ticket" => {
            let a: Ticket = parse(args_json)?;
            session::redeem_ticket(&a.ticket)?;
            encode(&session::status()?)
        }
        "import_tokens" => {
            let a: Path = parse(args_json)?;
            session::import_tokens(a.path.as_deref())?;
            encode(&session::status()?)
        }
        "sign_out" => {
            session::sign_out()?;
            encode(&session::status()?)
        }
        "settings" => encode(&settings::settings_view()?),
        "set_theme" => {
            let a: Theme = parse(args_json)?;
            encode(&settings::set_theme(a.theme)?)
        }
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
    }

    #[test]
    fn error_serializes_with_a_kind() {
        let json = serde_json::to_string(&ApiError::UnknownCommand("x".into())).unwrap();
        assert_eq!(json, r#"{"kind":"unknown_command","message":"x"}"#);
    }
}
