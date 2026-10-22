//! The saved threads of the Ask screen: `chat_list`, `chat_get`,
//! `chat_rename` and `chat_delete`.
//!
//! The thread store is `garmin_api::chat`, which the desktop app also calls.
//! This module parses the JSON arguments and adds what only the bridge knows:
//! the turn that runs now.

use garmin_api::chat::{self as store, ListArgs, RenameArgs, SessionArgs};
pub(crate) use garmin_api::chat::{DraftEntry, Status, Step, StepKind, Turn};
use garmin_api::{ApiError, Source};
use serde_json::Value;

use crate::ask::{bad, discard_session, live_turn, running_session};
use crate::{api_error, FfiError};

/// Add a running turn to the thread.
pub(crate) fn begin_turn(
    fixture: bool,
    session_id: &str,
    question: &str,
) -> Result<Turn, ApiError> {
    store::begin_turn(Source::sample(fixture), session_id, question)
}

/// Store the state of a turn. A thread or turn that is gone stays gone.
pub(crate) fn finish_turn(fixture: bool, session_id: &str, turn: &Turn) -> Result<(), ApiError> {
    store::finish_turn(Source::sample(fixture), session_id, turn)
}

/// Set the saved workout id of one draft in the stored thread.
pub(crate) fn set_saved(
    fixture: bool,
    key: (&str, &str, &str),
    workout_id: i64,
) -> Result<Option<DraftEntry>, ApiError> {
    store::set_saved(Source::sample(fixture), key, workout_id)
}

/// The stored draft of one turn.
pub(crate) fn find_draft(
    fixture: bool,
    key: (&str, &str, &str),
) -> Result<Option<DraftEntry>, ApiError> {
    store::find_draft(Source::sample(fixture), key)
}

/// Handle a `chat_*` command, or return `None` for any other command.
pub(crate) fn handle(cmd: &str, args: &Value, fixture: bool) -> Option<Result<String, FfiError>> {
    let source = Source::sample(fixture);
    Some(match cmd {
        "chat_list" => list(args, source),
        "chat_get" => get(args, source),
        "chat_rename" => rename(args, source),
        "chat_delete" => delete(args, source),
        _ => return None,
    })
}

fn parse<T: serde::de::DeserializeOwned>(args: &Value) -> Result<T, FfiError> {
    serde_json::from_value(args.clone()).map_err(bad)
}

fn json(v: &impl serde::Serialize) -> Result<String, FfiError> {
    serde_json::to_string(v).map_err(|e| api_error(ApiError::Internal(e.to_string())))
}

fn list(args: &Value, source: Source) -> Result<String, FfiError> {
    let a: ListArgs = parse(args)?;
    let live = running_session();
    json(&store::list_threads(source, &a, live.as_deref()).map_err(api_error)?)
}

fn get(args: &Value, source: Source) -> Result<String, FfiError> {
    let a: SessionArgs = parse(args)?;
    let running = running_session().as_deref() == Some(a.session_id.as_str());
    let thread = store::get_thread(source, &a.session_id, running, live_turn(&a.session_id))
        .map_err(api_error)?;
    json(&thread)
}

fn rename(args: &Value, source: Source) -> Result<String, FfiError> {
    let a: RenameArgs = parse(args)?;
    store::rename_thread(source, &a.session_id, &a.title).map_err(api_error)?;
    Ok("{}".into())
}

fn delete(args: &Value, source: Source) -> Result<String, FfiError> {
    let a: SessionArgs = parse(args)?;
    store::delete_thread(source, &a.session_id).map_err(api_error)?;
    // After the delete: a failed delete keeps the running turn. A late save finds no row.
    discard_session(&a.session_id);
    Ok("{}".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_list_has_the_wire_shape_and_a_missing_thread_is_null() {
        let out: Value = serde_json::from_str(&list(&json!({}), Source::Sample).unwrap()).unwrap();
        let first = &out["threads"][0];
        for key in ["running", "sessionId", "title", "updatedAt", "turnCount"] {
            assert!(!first[key].is_null(), "{key}");
        }
        assert_eq!(first["running"], false);
        let none = get(&json!({ "sessionId": "nope" }), Source::Sample).unwrap();
        assert_eq!(none, "null");
    }

    #[test]
    fn a_rename_without_a_title_is_a_bad_argument() {
        let args = json!({ "sessionId": "x", "title": " " });
        assert!(matches!(
            rename(&args, Source::Sample),
            Err(FfiError::BadArgs { .. })
        ));
    }
}
