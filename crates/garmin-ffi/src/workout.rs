//! The `workout_create` command: save a workout that the coach proposed.
//!
//! The args are `{"sessionId","turnId","callId"}`. The result is
//! `{"workoutId": <i64>}`. The stored draft is the source, so the app cannot
//! send a workout that the coach did not propose. A draft that was saved before
//! returns its id and does not call Garmin again.

use std::future::Future;

use garmin_core::workout::WorkoutDraft;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::ask::{bad, failed, mark_saved};
use crate::chat;
use crate::FfiError;

/// The id that sample mode returns. Sample mode never calls Garmin.
const SAMPLE_WORKOUT_ID: i64 = 9_000_000_001;

/// One save at a time, so a double press cannot create two workouts. It is held
/// across the call to Garmin.
static SAVE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Create {
    session_id: String,
    turn_id: String,
    call_id: String,
}

/// Handle a `workout_*` command, or return `None` for any other command.
pub(crate) async fn handle(
    cmd: &str,
    args: &Value,
    fixture: bool,
) -> Option<Result<String, FfiError>> {
    if cmd != "workout_create" {
        return None;
    }
    Some(if fixture {
        create(args, true, |_| async { Ok(SAMPLE_WORKOUT_ID) }, || {}).await
    } else {
        create(args, false, save_to_garmin, refresh_in_background).await
    })
}

/// Create the workout on Garmin. The call blocks, so it runs off the async
/// threads.
async fn save_to_garmin(draft: WorkoutDraft) -> Result<i64, FfiError> {
    let joined =
        tokio::task::spawn_blocking(move || garmin_api::workouts::create_workout(&draft)).await;
    match joined {
        Ok(Ok(id)) => Ok(id),
        Ok(Err(e)) => Err(FfiError::Command {
            json: serde_json::to_string(&e).unwrap_or_else(|_| e.to_string()),
        }),
        Err(e) => Err(failed(e)),
    }
}

/// Refresh the cached workouts on a worker thread. A failure is ignored.
fn refresh_in_background() {
    drop(tokio::task::spawn_blocking(
        garmin_api::workouts::refresh_workouts,
    ));
}

/// Read the stored draft, save it when it has no id, and store the id in that
/// one turn. `save` is the call to Garmin. `refresh` runs after the id is
/// stored and must not wait for the network.
async fn create<F, Fut>(
    args: &Value,
    fixture: bool,
    save: F,
    refresh: impl FnOnce(),
) -> Result<String, FfiError>
where
    F: FnOnce(WorkoutDraft) -> Fut,
    Fut: Future<Output = Result<i64, FfiError>>,
{
    let a: Create = serde_json::from_value(args.clone()).map_err(bad)?;
    let key = (
        a.session_id.as_str(),
        a.turn_id.as_str(),
        a.call_id.as_str(),
    );
    let _one_save = SAVE_LOCK.lock().await;
    let entry = chat::find_draft(fixture, key)
        .map_err(failed)?
        .ok_or_else(|| bad("no such draft"))?;
    let id = match entry.saved_workout_id {
        Some(id) => id,
        None => {
            let id = save(entry.draft).await?;
            // Store the id before any other call, so a kill cannot lose it.
            chat::set_saved(fixture, key, id).map_err(failed)?;
            mark_saved(key.0, key.1, key.2, id);
            refresh();
            id
        }
    };
    Ok(json!({ "workoutId": id }).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::{DraftEntry, Status, Turn};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn draft() -> WorkoutDraft {
        garmin_core::workout::parse_draft(&json!({
            "name": "Easy", "sport": "running",
            "steps": [{ "kind": "interval", "end": { "seconds": 600 } }]
        }))
        .unwrap()
    }

    /// A sample-mode thread with one turn that holds one draft.
    fn thread(session: &str, saved: Option<i64>) -> Value {
        let turn = chat::begin_turn(true, session, "plan").unwrap();
        let done = Turn {
            status: Status::Done,
            drafts: vec![DraftEntry {
                call_id: "c1".into(),
                draft: draft(),
                saved_workout_id: saved,
            }],
            ..turn
        };
        chat::finish_turn(true, session, &done).unwrap();
        json!({ "sessionId": session, "turnId": done.id, "callId": "c1" })
    }

    fn id_of(out: Result<String, FfiError>) -> i64 {
        let v: Value = serde_json::from_str(&out.unwrap()).unwrap();
        v["workoutId"].as_i64().unwrap()
    }

    #[tokio::test]
    async fn a_saved_draft_returns_its_id_and_does_not_call_garmin() {
        let args = thread("wk-saved", Some(77));
        let calls = AtomicUsize::new(0);
        let out = create(
            &args,
            true,
            |_| async {
                calls.fetch_add(1, Ordering::Relaxed);
                Ok(1)
            },
            || {},
        )
        .await;
        assert_eq!(id_of(out), 77);
        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn a_new_draft_saves_once_and_the_id_stays() {
        let args = thread("wk-new", None);
        let calls = AtomicUsize::new(0);
        let save = |_| async {
            calls.fetch_add(1, Ordering::Relaxed);
            Ok(55)
        };
        let refreshes = AtomicUsize::new(0);
        let refresh = || {
            // The id is already stored when the refresh runs.
            let key = ("wk-new", args["turnId"].as_str().unwrap(), "c1");
            let stored = chat::find_draft(true, key).unwrap().unwrap();
            assert_eq!(stored.saved_workout_id, Some(55));
            refreshes.fetch_add(1, Ordering::Relaxed);
        };
        assert_eq!(id_of(create(&args, true, save, refresh).await), 55);
        assert_eq!(id_of(create(&args, true, save, || {}).await), 55);
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(refreshes.load(Ordering::Relaxed), 1);
        let key = ("wk-new", args["turnId"].as_str().unwrap(), "c1");
        let stored = chat::find_draft(true, key).unwrap().unwrap();
        assert_eq!(stored.saved_workout_id, Some(55));
    }

    #[tokio::test]
    async fn saving_the_second_draft_marks_only_the_second() {
        let mut turn = chat::begin_turn(true, "wk-two", "plan").unwrap();
        turn.status = Status::Done;
        turn.drafts = ["c1", "c1-1"]
            .map(|id| DraftEntry {
                call_id: id.into(),
                draft: draft(),
                saved_workout_id: None,
            })
            .to_vec();
        chat::finish_turn(true, "wk-two", &turn).unwrap();
        let turn_id = turn.id.as_str();
        let args2 = json!({ "sessionId": "wk-two", "turnId": turn_id, "callId": "c1-1" });
        assert_eq!(
            id_of(create(&args2, true, |_| async { Ok(8) }, || {}).await),
            8
        );
        let find = |call: &str| chat::find_draft(true, ("wk-two", turn_id, call)).unwrap();
        assert_eq!(find("c1").unwrap().saved_workout_id, None);
        assert_eq!(find("c1-1").unwrap().saved_workout_id, Some(8));
    }

    /// The turn in memory is older than the store. Its end must keep the id.
    #[test]
    fn the_end_of_a_turn_keeps_a_saved_id() {
        let args = thread("wk-race", None);
        let id = args["turnId"].as_str().unwrap();
        let key = ("wk-race", id, "c1");
        chat::set_saved(true, key, 5).unwrap();
        let stale = Turn {
            id: id.into(),
            question: "plan".into(),
            answer: String::new(),
            steps: Vec::new(),
            status: Status::Done,
            error: None,
            drafts: vec![DraftEntry {
                call_id: "c1".into(),
                draft: draft(),
                saved_workout_id: None,
            }],
        };
        chat::finish_turn(true, "wk-race", &stale).unwrap();
        let stored = chat::find_draft(true, key).unwrap().unwrap();
        assert_eq!(stored.saved_workout_id, Some(5));
    }

    #[tokio::test]
    async fn an_unknown_call_id_is_a_bad_argument() {
        let mut args = thread("wk-unknown", None);
        args["callId"] = "nope".into();
        let err = create(&args, true, |_| async { Ok(1) }, || {})
            .await
            .unwrap_err();
        assert!(matches!(err, FfiError::BadArgs { .. }));
        let err = handle("workout_create", &json!({}), true).await;
        assert!(matches!(err, Some(Err(FfiError::BadArgs { .. }))));
        assert!(handle("other", &json!({}), true).await.is_none());
    }
}
