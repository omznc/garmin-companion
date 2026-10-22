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

use garmin_api::session::{SyncArgs, SyncEvent, SyncSummary};
use garmin_api::{ApiError, Source};

// The plain `release` profile sets panic = "abort". A panic in a Rust task
// would then kill the app. Build for mobile with `--profile release-mobile`.
#[cfg(all(any(target_os = "android", target_os = "ios"), panic = "abort"))]
compile_error!("garmin-ffi needs unwinding on mobile: build with --profile release-mobile");

mod ask;
#[cfg(target_os = "ios")]
mod backup;
mod chat;
mod cloud;
#[cfg(test)]
mod contract;
mod workout;

uniffi::setup_scaffolding!();

static FIXTURE_MODE: AtomicBool = AtomicBool::new(false);

/// One turn runs at a time, and `init` sets the sample mode for the process.
/// A test that sends, stops or deletes a turn, or reads the mode, takes this lock.
#[cfg(test)]
pub(crate) static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
    #[cfg(target_os = "ios")]
    backup::exclude_dir(std::path::Path::new(&base_dir));
    garmin_api::init(base_dir);
    // A signed-in account always reads real data, also when no sync has run yet.
    let signed_in = garmin_core::store::load_tokens().is_ok_and(|t| t.is_some());
    FIXTURE_MODE.store(fixture_mode && !signed_in, Ordering::Relaxed);
}

/// Sign-in worked, so the process leaves sample mode for good.
fn leave_fixture_mode() {
    FIXTURE_MODE.store(false, Ordering::Relaxed);
    if let Ok(dir) = garmin_core::paths::base_dir() {
        // The app writes the marker in its document folder: `files/` on Android, the base on iOS.
        for marker in [dir.join("files/sample-mode"), dir.join("sample-mode")] {
            let _ = std::fs::remove_file(marker);
        }
    }
}

/// A synchronous call. It proves the plain function path.
#[uniffi::export]
pub fn version() -> String {
    garmin_api::settings::APP_VERSION.to_owned()
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
    if let Some(out) = ask::handle(cmd, args, fixture).await {
        return out;
    }
    if let Some(out) = chat::handle(cmd, args, fixture) {
        return out;
    }
    if let Some(out) = cloud::handle(cmd, args, fixture).await {
        return out;
    }
    if let Some(out) = workout::handle(cmd, args, fixture).await {
        return out;
    }
    match cmd {
        "redeem_ticket" if fixture => {
            let out = run_api(Source::Sample, cmd.to_owned(), args.to_string()).await?;
            leave_fixture_mode();
            Ok(out)
        }
        "sync" if fixture => run_sample_sync(args).await,
        "sync" => run_real_sync(args).await,
        _ => run_api(Source::sample(fixture), cmd.to_owned(), args.to_string()).await,
    }
}

/// `garmin_api::call_with` blocks on the database, so it runs off the async threads.
async fn run_api(source: Source, cmd: String, args_json: String) -> Result<String, FfiError> {
    flatten(
        tokio::task::spawn_blocking(move || garmin_api::call_with(source, &cmd, &args_json)).await,
    )
}

/// A bad argument stays a bad argument. Any other error keeps its `kind` and message as JSON.
pub(crate) fn api_error(e: garmin_api::ApiError) -> FfiError {
    match e {
        garmin_api::ApiError::BadArguments(reason) => FfiError::BadArgs { reason },
        e => FfiError::Command {
            json: serde_json::to_string(&e).unwrap_or_else(|_| e.to_string()),
        },
    }
}

fn flatten(
    joined: Result<Result<String, garmin_api::ApiError>, tokio::task::JoinError>,
) -> Result<String, FfiError> {
    match joined {
        Ok(out) => out.map_err(api_error),
        Err(e) => Err(FfiError::Command {
            json: serde_json::json!({ "kind": "internal", "message": e.to_string() }).to_string(),
        }),
    }
}

/// The real sync. It reads `{days, full}`, sends a `sync-progress` event for
/// each step, and returns the summary. It needs a signed-in account.
async fn run_real_sync(args: &serde_json::Value) -> Result<String, FfiError> {
    let a = sync_args(args)?;
    let days = a.days.unwrap_or(30).clamp(1, 3650);
    let full = a.full.unwrap_or(false);
    let joined = tokio::task::spawn_blocking(move || {
        let on = |e: SyncEvent| {
            emit("sync-progress", serde_json::to_value(e).unwrap_or_default());
        };
        let summary = garmin_api::session::sync(days, full, &on)?;
        serde_json::to_string(&summary).map_err(|e| garmin_api::ApiError::Internal(e.to_string()))
    })
    .await;
    flatten(joined)
}

/// A stand-in for the real sync, for fixture mode. It sends progress events.
async fn run_sample_sync(args: &serde_json::Value) -> Result<String, FfiError> {
    let steps = sync_args(args)?.steps.unwrap_or(4).clamp(1, 20);
    for step in 1..=steps {
        tokio::time::sleep(Duration::from_millis(150)).await;
        let event = SyncEvent {
            phase: "wellness".into(),
            detail: String::new(),
            done: step,
            total: Some(steps),
            line: format!("Reading your daily health data ({step} of {steps})"),
        };
        emit(
            "sync-progress",
            serde_json::to_value(event).unwrap_or_default(),
        );
    }
    let summary = SyncSummary {
        line: "Sample sync finished. No data was changed.".into(),
        activities_seen: 0,
        activities_written: 0,
        days_written: 0,
        warnings: Vec::new(),
    };
    serde_json::to_string(&summary).map_err(|e| api_error(ApiError::Internal(e.to_string())))
}

fn sync_args(args: &serde_json::Value) -> Result<SyncArgs, FfiError> {
    serde_json::from_value(args.clone()).map_err(|e| FfiError::BadArgs {
        reason: e.to_string(),
    })
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
    async fn a_sample_sync_with_a_bad_days_is_a_bad_argument() {
        let err = dispatch("sync", &serde_json::json!({ "days": "x" }), true)
            .await
            .unwrap_err();
        assert!(matches!(err, FfiError::BadArgs { .. }));
    }

    #[tokio::test]
    async fn sync_sends_events_until_cancel() {
        let recorder = Arc::new(Recorder::default());
        let sub = subscribe(recorder.clone());
        let two = serde_json::json!({ "steps": 2 });
        let one = serde_json::json!({ "steps": 1 });
        let out = dispatch("sync", &two, true).await.unwrap();
        let summary: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(summary["line"].is_string() && summary["warnings"].is_array());
        assert_eq!(summary["daysWritten"], 0);
        sub.cancel();
        dispatch("sync", &one, true).await.unwrap();
        // The hub is global, so `ask` events of parallel tests are not counted.
        let all = recorder.0.lock().unwrap();
        let seen: Vec<_> = all.iter().filter(|(k, _)| k == "sync-progress").collect();
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
        let _seq = TEST_LOCK.lock().await;
        // A parallel `init` may have set sample mode, which skips the date check.
        FIXTURE_MODE.store(false, Ordering::Relaxed);
        let err = call("today".into(), r#"{"date":"soon"}"#.into())
            .await
            .unwrap_err();
        assert!(matches!(err, FfiError::BadArgs { .. }));
    }

    #[tokio::test]
    async fn fixture_body_and_detail_need_no_cache() {
        let w = dispatch("weight_view", &serde_json::json!({}), true)
            .await
            .unwrap();
        assert!(serde_json::from_str::<serde_json::Value>(&w)
            .unwrap()
            .is_object());
        for cmd in ["insights_view", "fitness_view", "report_view"] {
            let v = dispatch(cmd, &serde_json::json!({ "offset": 99 }), true)
                .await
                .unwrap();
            assert!(serde_json::from_str::<serde_json::Value>(&v)
                .unwrap()
                .is_object());
        }
        let d = dispatch(
            "activity_detail",
            &serde_json::json!({ "activityId": 42 }),
            true,
        )
        .await
        .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&d).unwrap()["activityId"],
            42
        );
    }

    #[tokio::test]
    async fn fixture_commands_create_no_cache() {
        let _seq = TEST_LOCK.lock().await;
        let dir = std::env::temp_dir().join(format!("garmin-ffi-fixture-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        init(dir.to_string_lossy().into_owned(), true);
        let none = serde_json::json!({});
        let save = serde_json::json!({ "provider": "openrouter", "key": "k" });
        let sid = serde_json::json!({ "sessionId": "guard" });
        let rename = serde_json::json!({ "sessionId": "guard", "title": "t" });
        let create =
            serde_json::json!({ "sessionId": "guard", "turnId": "guard:0", "callId": "c" });
        let cmds = [
            ("today", &none),
            ("activities", &none),
            ("activity_detail", &none),
            ("health_view", &none),
            ("sleep_view", &none),
            ("weight_view", &none),
            ("food_view", &none),
            ("strength_view", &none),
            ("plan_view", &none),
            ("routes_view", &none),
            ("gear_view", &none),
            ("cached_daily", &none),
            ("cached_activities", &none),
            ("cached_activities_since", &none),
            ("cached_activity", &none),
            ("nutrition", &none),
            ("weight", &none),
            ("settings", &none),
            ("ask_config", &none),
            ("ask_save_config", &save),
            ("chat_list", &none),
            ("chat_get", &sid),
            ("chat_rename", &rename),
            ("chat_delete", &sid),
            ("workout_create", &create),
            ("cloud_account", &none),
            ("cloud_sign_out", &none),
        ];
        for (cmd, args) in cmds {
            let _ = dispatch(cmd, args, true).await;
            assert!(!dir.join("cache.sqlite3").exists(), "{cmd} made the cache");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn version_is_set() {
        assert!(!version().is_empty());
    }
}
