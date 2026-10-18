//! The native bridge of the mobile app.
//!
//! The surface is small on purpose: `call` runs one command and returns JSON,
//! `subscribe` receives events. The app can use the generated bindings or a
//! hand-written module, because the shape is the same.
//!
//! `call` sends the command to `garmin-api`. In fixture mode, `today` returns
//! recorded data, so the app runs with no Garmin account.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

uniffi::setup_scaffolding!();

const TODAY_SAMPLE: &str = include_str!("../../garmin-api/fixtures/today.sample.json");

const ACTIVITIES_SAMPLE: &str = include_str!("../../garmin-api/fixtures/activities.sample.json");
const ACTIVITY_SAMPLE: &str = include_str!("../../garmin-api/fixtures/activity.sample.json");
const HEALTH_SAMPLE: &str = include_str!("../../garmin-api/fixtures/health.sample.json");
const SLEEP_SAMPLE: &str = include_str!("../../garmin-api/fixtures/sleep.sample.json");

static FIXTURE_MODE: AtomicBool = AtomicBool::new(false);

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiError {
    #[error("bad arguments: {reason}")]
    BadArgs { reason: String },
    /// The command failed. `json` is the `garmin-api` error: `{"kind","message"}`.
    #[error("{json}")]
    Command { json: String },
}

/// The app implements this trait. The core calls it from any thread.
#[uniffi::export(with_foreign)]
pub trait EventListener: Send + Sync {
    fn on_event(&self, kind: String, payload_json: String);
}

type Listeners = Vec<(u64, Arc<dyn EventListener>)>;

#[derive(Default)]
struct Hub {
    next_id: u64,
    listeners: Listeners,
}

fn hub() -> &'static Mutex<Hub> {
    static HUB: OnceLock<Mutex<Hub>> = OnceLock::new();
    HUB.get_or_init(Mutex::default)
}

fn lock_hub() -> std::sync::MutexGuard<'static, Hub> {
    // A listener that panicked must not stop all later events.
    hub().lock().unwrap_or_else(|e| e.into_inner())
}

fn emit(kind: &str, payload: serde_json::Value) {
    // Copy the list first, so a listener can call `subscribe` without a deadlock.
    let listeners: Vec<_> = lock_hub()
        .listeners
        .iter()
        .map(|(_, l)| l.clone())
        .collect();
    for listener in listeners {
        listener.on_event(kind.to_owned(), payload.to_string());
    }
}

/// The handle of one subscription. Call `cancel` to stop the events.
#[derive(uniffi::Object)]
pub struct Subscription {
    id: u64,
}

#[uniffi::export]
impl Subscription {
    pub fn cancel(&self) {
        lock_hub().listeners.retain(|(id, _)| *id != self.id);
    }
}

/// Set the data folder of the app. Call it once, before `call`.
///
/// In fixture mode, `today` returns recorded data.
#[uniffi::export]
pub fn init(base_dir: String, fixture_mode: bool) {
    garmin_api::init(base_dir);
    FIXTURE_MODE.store(fixture_mode, Ordering::Relaxed);
}

/// A synchronous call. It proves the plain function path.
#[uniffi::export]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

#[uniffi::export]
pub fn subscribe(listener: Arc<dyn EventListener>) -> Arc<Subscription> {
    let mut hub = lock_hub();
    hub.next_id += 1;
    let id = hub.next_id;
    hub.listeners.push((id, listener));
    Arc::new(Subscription { id })
}

/// Run one command. `args_json` is a JSON object. The result is JSON.
#[uniffi::export(async_runtime = "tokio")]
pub async fn call(cmd: String, args_json: String) -> Result<String, FfiError> {
    let args: serde_json::Value =
        serde_json::from_str(&args_json).map_err(|e| FfiError::BadArgs {
            reason: e.to_string(),
        })?;
    dispatch(&cmd, &args, FIXTURE_MODE.load(Ordering::Relaxed)).await
}

async fn dispatch(cmd: &str, args: &serde_json::Value, fixture: bool) -> Result<String, FfiError> {
    match cmd {
        "today" if fixture => Ok(TODAY_SAMPLE.to_owned()),
        "activities" if fixture => Ok(ACTIVITIES_SAMPLE.to_owned()),
        "activity_detail" if fixture => Ok(ACTIVITY_SAMPLE.to_owned()),
        "health_view" if fixture => Ok(HEALTH_SAMPLE.to_owned()),
        "sleep_view" if fixture => Ok(SLEEP_SAMPLE.to_owned()),
        "sync" if fixture => Ok(run_sync(args).await),
        "sync" => run_real_sync(args).await,
        // Demo only: `today` for a chosen day, for a cache that is not current.
        "today" if args.get("date").is_some() => run_today_on(args).await,
        _ => run_api(cmd.to_owned(), args.to_string()).await,
    }
}

async fn run_today_on(args: &serde_json::Value) -> Result<String, FfiError> {
    let date = args["date"]
        .as_str()
        .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .ok_or_else(|| FfiError::BadArgs {
            reason: "date must be YYYY-MM-DD".into(),
        })?;
    let joined = tokio::task::spawn_blocking(move || {
        let db = garmin_api::open_default_db()?;
        let view = garmin_api::today_view(&db, date)?;
        serde_json::to_string(&view).map_err(|e| garmin_api::ApiError::Internal(e.to_string()))
    })
    .await;
    flatten(joined)
}

/// `garmin_api::call` blocks on the database, so it runs off the async threads.
async fn run_api(cmd: String, args_json: String) -> Result<String, FfiError> {
    flatten(tokio::task::spawn_blocking(move || garmin_api::call(&cmd, &args_json)).await)
}

fn flatten(
    joined: Result<Result<String, garmin_api::ApiError>, tokio::task::JoinError>,
) -> Result<String, FfiError> {
    match joined {
        Ok(Ok(json)) => Ok(json),
        Ok(Err(e)) => Err(FfiError::Command {
            json: serde_json::to_string(&e).unwrap_or_else(|_| e.to_string()),
        }),
        Err(e) => Err(FfiError::Command {
            json: serde_json::json!({ "kind": "internal", "message": e.to_string() }).to_string(),
        }),
    }
}

/// The real sync. It reads `{days, full}`, sends a `sync-progress` event for
/// each step, and returns the summary. It needs a signed-in account.
async fn run_real_sync(args: &serde_json::Value) -> Result<String, FfiError> {
    let days = args["days"].as_u64().unwrap_or(30).clamp(1, 3650) as u32;
    let full = args["full"].as_bool().unwrap_or(false);
    let joined = tokio::task::spawn_blocking(move || {
        let on = |e: garmin_api::session::SyncEvent| {
            emit("sync-progress", serde_json::to_value(e).unwrap_or_default());
        };
        let summary = garmin_api::session::sync(days, full, &on)?;
        serde_json::to_string(&summary).map_err(|e| garmin_api::ApiError::Internal(e.to_string()))
    })
    .await;
    flatten(joined)
}

/// A stand-in for the real sync, for fixture mode. It sends progress events.
async fn run_sync(args: &serde_json::Value) -> String {
    let steps = args
        .get("steps")
        .and_then(|v| v.as_u64())
        .unwrap_or(4)
        .clamp(1, 20);
    for step in 1..=steps {
        tokio::time::sleep(Duration::from_millis(150)).await;
        emit(
            "sync-progress",
            serde_json::json!({
                "phase": "wellness",
                "detail": "",
                "done": step,
                "total": steps,
                "line": format!("Reading your daily health data ({step} of {steps})"),
            }),
        );
    }
    serde_json::json!({ "ok": true, "steps": steps }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Recorder(Mutex<Vec<(String, String)>>);

    impl EventListener for Recorder {
        fn on_event(&self, kind: String, payload_json: String) {
            self.0.lock().unwrap().push((kind, payload_json));
        }
    }

    #[tokio::test]
    async fn fixture_today_is_json() {
        let out = dispatch("today", &serde_json::json!({}), true)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(value["lede"].is_string());
    }

    #[tokio::test]
    async fn unknown_command_fails_with_kind() {
        let err = call("nope".into(), "{}".into()).await.unwrap_err();
        let FfiError::Command { json } = err else {
            panic!("wrong error")
        };
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["kind"], "unknown_command");
    }

    #[tokio::test]
    async fn bad_json_fails() {
        let err = call("sync".into(), "{".into()).await.unwrap_err();
        assert!(matches!(err, FfiError::BadArgs { .. }));
    }

    #[tokio::test]
    async fn sync_sends_events_until_cancel() {
        let recorder = Arc::new(Recorder::default());
        let sub = subscribe(recorder.clone());
        let two = serde_json::json!({ "steps": 2 });
        let one = serde_json::json!({ "steps": 1 });
        dispatch("sync", &two, true).await.unwrap();
        sub.cancel();
        dispatch("sync", &one, true).await.unwrap();
        let seen = recorder.0.lock().unwrap();
        assert_eq!(seen.len(), 2);
        let last: serde_json::Value = serde_json::from_str(&seen[1].1).unwrap();
        assert_eq!(
            (last["done"].as_u64(), last["total"].as_u64()),
            (Some(2), Some(2))
        );
        assert!(last["line"].as_str().unwrap().contains("2 of 2"));
    }

    #[tokio::test]
    async fn today_with_a_bad_date_fails() {
        let err = call("today".into(), r#"{"date":"soon"}"#.into())
            .await
            .unwrap_err();
        assert!(matches!(err, FfiError::BadArgs { .. }));
    }

    #[test]
    fn version_is_set() {
        assert!(!version().is_empty());
    }
}
