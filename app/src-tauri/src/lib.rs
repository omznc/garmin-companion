mod chat;
mod login;

use anyhow::Result;
use garmin_core::{db, db::Db, query, store, CachedActivity, GarminClient};
use serde::Serialize;
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::RwLock;

/// Errors cross the Tauri boundary as plain strings — the frontend only ever
/// displays them, and anyhow's context chain reads well enough as-is.
type CmdResult<T> = std::result::Result<T, String>;

fn to_msg(e: anyhow::Error) -> String {
    e.chain()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(": ")
}

#[derive(Default)]
pub struct AppState {
    garmin: RwLock<Option<Arc<GarminClient>>>,
}

impl AppState {
    async fn client(&self) -> CmdResult<Arc<GarminClient>> {
        if let Some(c) = self.garmin.read().await.as_ref() {
            return Ok(c.clone());
        }
        let mut guard = self.garmin.write().await;
        // Re-check: another task may have connected while we waited for the lock.
        if let Some(c) = guard.as_ref() {
            return Ok(c.clone());
        }
        let built = garmin_core::client_from_keyring().map_err(to_msg)?;
        match built {
            Some(c) => {
                let c = Arc::new(c);
                *guard = Some(c.clone());
                Ok(c)
            }
            None => Err("Not connected to Garmin yet.".to_string()),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    connected: bool,
    /// Present only when a `garminconnect` token file exists that we could
    /// adopt, so the UI can offer one-click import instead of a login.
    importable_token_path: Option<String>,
}

#[tauri::command]
async fn garmin_status() -> CmdResult<ConnectionStatus> {
    let connected = store::load_tokens().map_err(to_msg)?.is_some();
    let importable = store::python_token_path()
        .filter(|p| p.exists())
        .map(|p| p.display().to_string());

    Ok(ConnectionStatus {
        connected,
        importable_token_path: if connected { None } else { importable },
    })
}

/// Adopt tokens from an existing `garminconnect` install.
#[tauri::command]
async fn garmin_import_tokens(
    state: tauri::State<'_, AppState>,
    path: Option<String>,
) -> CmdResult<()> {
    let path = match path {
        Some(p) => std::path::PathBuf::from(p),
        None => store::python_token_path()
            .ok_or_else(|| "could not locate a home directory".to_string())?,
    };

    let tokens = store::import_python_tokens(&path).map_err(to_msg)?;
    store::save_tokens(&tokens).map_err(to_msg)?;
    *state.garmin.write().await = None; // force a rebuild on next use
    Ok(())
}

#[tauri::command]
async fn garmin_disconnect(state: tauri::State<'_, AppState>) -> CmdResult<()> {
    store::clear_tokens().map_err(to_msg)?;
    *state.garmin.write().await = None;
    Ok(())
}

#[tauri::command]
async fn garmin_profile(state: tauri::State<'_, AppState>) -> CmdResult<serde_json::Value> {
    let client = state.client().await?;
    let profile = client.profile().await.map_err(to_msg)?;
    serde_json::to_value(profile).map_err(|e| e.to_string())
}

#[tauri::command]
async fn garmin_activities(
    state: tauri::State<'_, AppState>,
    start: Option<u32>,
    limit: Option<u32>,
) -> CmdResult<serde_json::Value> {
    let client = state.client().await?;
    let acts = client
        .activities(start.unwrap_or(0), limit.unwrap_or(20))
        .await
        .map_err(to_msg)?;
    serde_json::to_value(acts).map_err(|e| e.to_string())
}

#[tauri::command]
async fn garmin_hr_zones(
    state: tauri::State<'_, AppState>,
    activity_id: i64,
) -> CmdResult<serde_json::Value> {
    let client = state.client().await?;
    let zones = client.hr_time_in_zones(activity_id).await.map_err(to_msg)?;
    serde_json::to_value(zones).map_err(|e| e.to_string())
}

/// Pull fresh data from Garmin into the local cache.
///
/// A fresh `Db` handle is opened per command rather than shared in `AppState`:
/// `rusqlite::Connection` isn't `Sync`, and WAL mode makes short-lived handles
/// cheap and safe alongside the MCP server holding its own.
#[tauri::command]
async fn sync_now<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    days: Option<u32>,
    full: Option<bool>,
) -> CmdResult<serde_json::Value> {
    let client = state.client().await?;
    let (days, full) = (days.unwrap_or(30), full.unwrap_or(false));

    let handle = tokio::runtime::Handle::current();
    let report = tokio::task::spawn_blocking(move || -> Result<_> {
        let db = Db::open_default()?;
        // A first sync is one request per endpoint per day, which is minutes of
        // nothing to look at. Each step goes out as an event so the screen can
        // say which date it's on. A dropped event only costs a stale line.
        let on = move |p: garmin_core::sync::SyncProgress| {
            let _ = app.emit("sync:progress", p);
        };
        handle.block_on(garmin_core::sync::sync_all_with(
            &client, &db, days, full, &on,
        ))
    })
    .await
    .map_err(|e| format!("sync task panicked: {e}"))?
    .map_err(to_msg)?;

    serde_json::to_value(report).map_err(|e| e.to_string())
}

// The cache reads are synchronous commands: they're single indexed queries, and
// `rusqlite::Connection` isn't `Sync`, which an async command would require.
#[tauri::command]
fn cached_activities(
    limit: Option<u32>,
    type_key: Option<String>,
) -> CmdResult<Vec<CachedActivity>> {
    let db = Db::open_default().map_err(to_msg)?;
    db.recent_activities(limit.unwrap_or(30), type_key.as_deref())
        .map_err(to_msg)
}

#[tauri::command]
fn cached_daily(days: Option<u32>) -> CmdResult<Vec<garmin_core::DailyMetrics>> {
    let db = Db::open_default().map_err(to_msg)?;
    db.recent_daily(days.unwrap_or(30)).map_err(to_msg)
}

#[tauri::command]
fn cached_activity(activity_id: i64) -> CmdResult<Option<CachedActivity>> {
    let db = Db::open_default().map_err(to_msg)?;
    db.activity(activity_id).map_err(to_msg)
}

/// Every cached activity on or after `from` (`YYYY-MM-DD`). The screens that
/// reason over a window — weekly load, correlations — need the whole window,
/// not the most recent N.
#[tauri::command]
fn cached_activities_since(from: String) -> CmdResult<Vec<CachedActivity>> {
    let db = Db::open_default().map_err(to_msg)?;
    db.activities_since(&from).map_err(to_msg)
}

/// Calories in against calories out, plus hydration, for the Food screen.
#[tauri::command]
fn nutrition(days: Option<u32>) -> CmdResult<query::NutritionReport> {
    let db = Db::open_default().map_err(to_msg)?;
    query::nutrition(&db, days.unwrap_or(30)).map_err(to_msg)
}

/// Weigh-ins, trend and the energy cross-check, for the Weight screen.
#[tauri::command]
fn weight(days: Option<u32>) -> CmdResult<query::WeightReport> {
    let db = Db::open_default().map_err(to_msg)?;
    query::weight(&db, days.unwrap_or(180)).map_err(to_msg)
}

/// Set or clear the target weight.
///
/// This one figure is the app's own, not Garmin's: the account exposes no
/// readable weight goal, so there is nothing to sync it against. Every actual
/// weigh-in still comes from Garmin and is never written to from here.
#[tauri::command]
fn set_weight_goal(target_kg: Option<f64>) -> CmdResult<()> {
    let db = Db::open_default().map_err(to_msg)?;
    match target_kg {
        Some(kg) if kg > 20.0 && kg < 500.0 => db
            .set_sync_state("weight_goal_kg", &kg.to_string())
            .map_err(to_msg),
        // Out of range is treated as clearing rather than as an error: the
        // field it comes from is a text box, and an empty one means "no goal".
        _ => db.set_sync_state("weight_goal_kg", "").map_err(to_msg),
    }
}

/* ---------------------------------------------------------------- themes --- */

/// Every custom theme in the folder. Read fresh each time rather than cached:
/// the folder is meant to be edited from outside the app, and the cost of
/// noticing is a directory listing.
#[tauri::command]
fn themes_list() -> CmdResult<Vec<garmin_core::theme::Theme>> {
    garmin_core::theme::list().map_err(to_msg)
}

/// Write a theme, returning it with the slug it was filed under — which the
/// caller needs, because the slug is what a selection is stored as and it is
/// derived from the name rather than sent.
#[tauri::command]
fn themes_save(theme: garmin_core::theme::Theme) -> CmdResult<garmin_core::theme::Theme> {
    garmin_core::theme::save(theme).map_err(to_msg)
}

#[tauri::command]
fn themes_delete(slug: String) -> CmdResult<()> {
    garmin_core::theme::delete(&slug).map_err(to_msg)
}

/// The folder itself, so Settings can show the path and open it in the file
/// manager. Creates it on the way past, so "Open folder" never lands on
/// something that isn't there yet.
#[tauri::command]
fn themes_dir() -> CmdResult<String> {
    garmin_core::theme::themes_dir()
        .map(|p| p.display().to_string())
        .map_err(to_msg)
}

/// Show the themes folder in the file manager.
///
/// Done here rather than by handing the path to the frontend's opener, which
/// would mean granting the webview `opener:allow-open-path` — a permission to
/// open *any* path, added for the sake of one folder this process already
/// knows. The path never crosses the boundary, so there is nothing to scope.
#[tauri::command]
fn themes_open() -> CmdResult<()> {
    let dir = garmin_core::theme::themes_dir().map_err(to_msg)?;
    tauri_plugin_opener::open_path(dir, None::<&str>).map_err(|e| e.to_string())
}

/// Saved Garmin workouts, for the Plan screen.
#[tauri::command]
fn workouts() -> CmdResult<Vec<garmin_core::db::Workout>> {
    let db = Db::open_default().map_err(to_msg)?;
    db.workouts().map_err(to_msg)
}

/// Save a drafted workout to the Garmin account.
///
/// The only command in this app that writes to Garmin, and the only caller of
/// the only client method that can. It exists because a human pressed a button
/// on a workout they were shown: the chat model can propose a draft but has no
/// way to invoke this, which is what keeps "the model suggested a session" and
/// "a session appeared on the watch" as two separate events.
///
/// Validated again here rather than trusting the draft that came back from the
/// screen. It left as something `chat::draft_workout` checked, but it has been
/// through a form since, and the cost of rechecking is a microsecond against a
/// bad workout landing on a watch.
#[tauri::command]
async fn create_workout(
    state: tauri::State<'_, AppState>,
    draft: garmin_core::WorkoutDraft,
) -> CmdResult<i64> {
    draft.validate().map_err(to_msg)?;
    let client = state.client().await?;
    let workout_id = client.create_workout(&draft).await.map_err(to_msg)?;

    // Pull the workout list back down so Plan and the `workouts` tool see what
    // was just created. A failure here is not a failed write — the workout is
    // on the account either way — so it costs a stale list until the next sync
    // rather than an error on a button that worked.
    let handle = tokio::runtime::Handle::current();
    let _ = tokio::task::spawn_blocking(move || -> Result<()> {
        let db = Db::open_default()?;
        let mut report = garmin_core::sync::SyncReport::default();
        handle.block_on(garmin_core::sync::sync_workouts(
            &client,
            &db,
            &mut report,
            &|_| {},
        ))
    })
    .await;

    Ok(workout_id)
}

/// Cached GPS traces grouped into repeated routes, in the requested order.
/// The sort belongs to the query rather than the screen because it decides
/// which routes are cheap enough to send a trace for.
#[tauri::command]
fn routes(sort: Option<query::RouteSort>) -> CmdResult<Vec<query::Route>> {
    let db = Db::open_default().map_err(to_msg)?;
    query::routes(&db, sort.unwrap_or_default()).map_err(to_msg)
}

/* ------------------------------------------------------------ chat sessions --- */

/// A page of saved conversations, newest first. Paged rather than returned
/// whole because the Ask screen scrolls them and there is no ceiling on how
/// many you accumulate.
#[tauri::command]
fn chat_sessions(limit: Option<u32>, offset: Option<u32>) -> CmdResult<Vec<db::ChatSessionMeta>> {
    let db = Db::open_default().map_err(to_msg)?;
    db.chat_sessions(limit.unwrap_or(20), offset.unwrap_or(0))
        .map_err(to_msg)
}

#[tauri::command]
fn chat_session(session_id: String) -> CmdResult<Option<db::ChatSession>> {
    let db = Db::open_default().map_err(to_msg)?;
    db.chat_session(&session_id).map_err(to_msg)
}

/// Insert or update a conversation. Called after every completed turn, so an
/// app that dies mid-session still leaves everything up to the last answer.
#[tauri::command]
fn save_chat_session(
    session_id: String,
    title: String,
    started_at: String,
    messages: String,
    message_count: i64,
) -> CmdResult<()> {
    let db = Db::open_default().map_err(to_msg)?;
    db.save_chat_session(&db::ChatSession {
        session_id,
        started_at,
        updated_at: chrono::Utc::now().to_rfc3339(),
        title,
        message_count,
        messages,
    })
    .map_err(to_msg)
}

#[tauri::command]
fn delete_chat_session(session_id: String) -> CmdResult<()> {
    let db = Db::open_default().map_err(to_msg)?;
    db.delete_chat_session(&session_id).map_err(to_msg)
}

/// Three things worth asking next, given the conversation so far.
#[tauri::command]
async fn chat_followups(history: Vec<chat::HistoryMessage>) -> CmdResult<Vec<String>> {
    chat::followups(history).await.map_err(to_msg)
}

/// The written summary at the top of the Weight screen.
///
/// Kept until the numbers behind it change, so returning to the page doesn't
/// bill a fresh request for prose about identical data. `force` is the
/// regenerate control.
#[tauri::command]
async fn weight_summary(days: Option<u32>, force: Option<bool>) -> CmdResult<chat::WeightSummary> {
    chat::weight_summary(days.unwrap_or(180), force.unwrap_or(false))
        .await
        .map_err(to_msg)
}

#[tauri::command]
fn cache_summary() -> CmdResult<serde_json::Value> {
    let db = Db::open_default().map_err(to_msg)?;
    Ok(serde_json::json!({
        "activities": db.activity_count().map_err(to_msg)?,
        "lastSync": db.sync_state("last_sync").map_err(to_msg)?,
        "path": garmin_core::db::default_path().map(|p| p.display().to_string()),
    }))
}

/// Per-lap splits, fetched live. Splits are only ever looked at one activity at
/// a time, so caching them would mean syncing thousands of laps nobody opens.
#[tauri::command]
async fn activity_splits(
    state: tauri::State<'_, AppState>,
    activity_id: i64,
) -> CmdResult<serde_json::Value> {
    let client = state.client().await?;
    client.activity_splits(activity_id).await.map_err(to_msg)
}

/// Sampled HR / pace / cadence / elevation series for the activity charts.
/// Live for the same reason as splits, and downsampled server-side.
#[tauri::command]
async fn activity_details(
    state: tauri::State<'_, AppState>,
    activity_id: i64,
    points: Option<u32>,
) -> CmdResult<serde_json::Value> {
    let client = state.client().await?;
    client
        .activity_details(activity_id, points.unwrap_or(400))
        .await
        .map_err(to_msg)
}

/* -------------------------------------------------------------- analysis --- */

/// How many earlier activities the comparison in an analysis is drawn from.
///
/// Over-fetched because the same-sport filter happens in `analysis`, and this
/// athlete's history interleaves runs, rides and strength sessions — asking for
/// eight rows would frequently yield nothing to compare against.
const COMPARE_POOL: u32 = 120;

/// Everything the activity screen draws below the summary numbers, plus the
/// bundle the written summary is generated from.
///
/// Three Garmin requests go into one of these — the sampled series, the laps,
/// and the zone boundaries — so the result is cached against a fingerprint of
/// the activity. Opening a session a second time is a cache read, and works
/// with no network at all.
///
/// `force` re-fetches from Garmin. Nothing in the UI calls it that way today;
/// it exists because a session Garmin was still processing at first open can
/// come back with more than it had.
#[tauri::command]
async fn activity_analysis(
    state: tauri::State<'_, AppState>,
    activity_id: i64,
    force: Option<bool>,
) -> CmdResult<garmin_core::ActivityAnalysis> {
    // The connection is opened and dropped inside each block: rusqlite's isn't
    // `Sync`, so a handle held across an await makes the future non-`Send`.
    let (activity, tags, key, cached) = {
        let db = Db::open_default().map_err(to_msg)?;
        let activity = db
            .activity(activity_id)
            .map_err(to_msg)?
            .ok_or_else(|| "That activity isn't in the local cache.".to_string())?;
        let tags = db.activity_tags(activity_id).map_err(to_msg)?;
        let key = garmin_core::analysis::fingerprint(&activity, &tags);
        let cached = if force.unwrap_or(false) {
            None
        } else {
            db.activity_analysis(activity_id, &key).map_err(to_msg)?
        };
        (activity, tags, key, cached)
    };

    if let Some(json) = cached {
        // A stored analysis written by an older build may not deserialize into
        // the current shape. That is a reason to recompute, not to fail.
        if let Ok(a) = garmin_core::analysis::decode(&json) {
            return Ok(a);
        }
    }

    // Each of the three is optional: an activity Garmin has no samples for
    // still has a zone breakdown and a lap list worth showing, and being
    // offline should cost the charts rather than the page.
    let client = state.client().await.ok();
    let (details, splits, zones) = match client {
        Some(c) => (
            c.activity_details(activity_id, 500).await.ok(),
            c.activity_splits(activity_id).await.ok(),
            c.hr_time_in_zones(activity_id)
                .await
                .ok()
                .and_then(|z| serde_json::to_value(z).ok()),
        ),
        None => (None, None, None),
    };

    let now = chrono::Utc::now().to_rfc3339();
    let db = Db::open_default().map_err(to_msg)?;
    let recent = db.recent_activities(COMPARE_POOL, None).map_err(to_msg)?;
    let analysis = garmin_core::analysis::analyse(
        &activity,
        details.as_ref(),
        splits.as_ref(),
        zones.as_ref(),
        &recent,
        tags,
        &now,
    );

    // Only worth storing when it was built from Garmin's samples. Caching the
    // degraded offline version would keep the charts empty after the network
    // came back.
    if details.is_some() {
        if let Ok(json) = serde_json::to_string(&analysis) {
            let _ = db.save_activity_analysis(activity_id, &key, &now, &json);
        }
    }

    Ok(analysis)
}

/* ------------------------------------------------------------------ tags --- */

#[tauri::command]
fn activity_tags(activity_id: i64) -> CmdResult<Vec<String>> {
    let db = Db::open_default().map_err(to_msg)?;
    db.activity_tags(activity_id).map_err(to_msg)
}

/// Replace an activity's tags, returning them as stored — trimmed, lowercased
/// and deduplicated, which is rarely exactly what was typed.
#[tauri::command]
fn set_activity_tags(activity_id: i64, tags: Vec<String>) -> CmdResult<Vec<String>> {
    let db = Db::open_default().map_err(to_msg)?;
    db.set_activity_tags(activity_id, &tags).map_err(to_msg)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagCount {
    tag: String,
    count: i64,
}

/// Every tag in use, commonest first — the suggestion list in the tag editor.
#[tauri::command]
fn all_tags() -> CmdResult<Vec<TagCount>> {
    let db = Db::open_default().map_err(to_msg)?;
    Ok(db
        .all_tags()
        .map_err(to_msg)?
        .into_iter()
        .map(|(tag, count)| TagCount { tag, count })
        .collect())
}

/// The critique already written about one activity, or nothing. Never reaches a
/// model: opening a session is not a request for one.
#[tauri::command]
async fn cached_activity_critique(
    state: tauri::State<'_, AppState>,
    activity_id: i64,
) -> CmdResult<Option<chat::ActivityCritique>> {
    let analysis = activity_analysis(state, activity_id, None).await?;
    chat::cached_activity_critique(&analysis).map_err(to_msg)
}

/// Write the critique of one activity. This is the button, and every call bills
/// a request — kept afterwards until the numbers or the tags behind it move.
#[tauri::command]
async fn activity_critique(
    state: tauri::State<'_, AppState>,
    activity_id: i64,
) -> CmdResult<chat::ActivityCritique> {
    let analysis = activity_analysis(state, activity_id, None).await?;
    chat::activity_critique(&analysis).await.map_err(to_msg)
}

#[tauri::command]
async fn gear_list(state: tauri::State<'_, AppState>) -> CmdResult<serde_json::Value> {
    let client = state.client().await?;
    let profile = client.profile().await.map_err(to_msg)?;
    let profile_id = profile
        .profile_id
        .ok_or_else(|| "Garmin did not return a profile id".to_string())?;

    let gear = client.gear(profile_id).await.map_err(to_msg)?;

    // The gear list carries no distance — that lives on a per-item stats
    // endpoint. A failure on one item shouldn't hide the rest of the list.
    let mut out = Vec::with_capacity(gear.len());
    for item in gear {
        let stats = client.gear_stats(&item.uuid).await.ok();
        out.push(serde_json::json!({ "gear": item, "stats": stats }));
    }
    Ok(serde_json::Value::Array(out))
}

/* ------------------------------------------------------------------ chat --- */

#[tauri::command]
async fn chat_config() -> CmdResult<chat::ChatConfig> {
    let db = Db::open_default().map_err(to_msg)?;
    let (provider, model) = chat::load_config(&db).map_err(to_msg)?;
    let has_key = store::load_openrouter_key().map_err(to_msg)?.is_some();
    let structured = chat::load_structured(&db).map_err(to_msg)?;

    let http = reqwest::Client::new();
    let (ollama_reachable, ollama_models) = chat::probe_ollama(&http).await;

    Ok(chat::ChatConfig {
        provider: provider.map(chat::Provider::as_str),
        model,
        has_key,
        structured,
        ollama_reachable,
        ollama_models,
    })
}

/// Whether the last request that reached the configured provider worked.
///
/// Read rather than probed: an active health check would spend a request — real
/// money on a hosted provider — to answer a question the last real request
/// already answered. `None` means nothing has been asked of it yet this run,
/// which is not the same as it being broken.
#[tauri::command]
fn chat_health() -> CmdResult<Option<chat::AiHealth>> {
    let db = Db::open_default().map_err(to_msg)?;
    let (provider, _) = chat::load_config(&db).map_err(to_msg)?;
    Ok(chat::health(provider))
}

#[tauri::command]
fn set_chat_provider(provider: String, model: String, structured: Option<bool>) -> CmdResult<()> {
    let p =
        chat::Provider::parse(&provider).ok_or_else(|| format!("unknown provider: {provider}"))?;

    // The proxy serves one model, so what the caller sent is not a choice it
    // gets to make. Storing it anyway would leave the picker's last selection
    // in `chat_model` for a provider that will bounce it.
    let model = if p == chat::Provider::Cloud {
        chat::CLOUD_MODEL
    } else {
        &model
    };

    let db = Db::open_default().map_err(to_msg)?;
    chat::save_config(&db, p, model, structured.unwrap_or(false)).map_err(to_msg)
}

// There is no `clear_device_id` command. It existed, sitting in Settings beside
// the OpenRouter key on the reasoning that both are credentials this app holds
// on someone's behalf — but they aren't the same kind of thing. The key is the
// athlete's own and costs them; the install id is what the hosted proxy counts
// against a budget this project pays. Clearing it reset that count, so the
// per-install daily cap was one click from meaning nothing. The id is issued by
// the proxy now (`chat::enroll`) and kept by `store::save_install_id`, so there
// is nothing here that could hand out a fresh one anyway.

/// Ask the hosted coach for this install's id, if it hasn't got one.
///
/// The frontend calls this the moment the hosted coach is picked, so the first
/// question doesn't wait on the enrolment round trip. It is not the reset
/// command wearing a different hat: an install that already has an id does
/// nothing here, so calling it repeatedly gets the same id it already had.
///
/// The error comes back for the record, not to be shown — setup ignores it and
/// carries on, and `chat_health` is where the reason surfaces.
#[tauri::command]
async fn prepare_cloud_chat() -> CmdResult<()> {
    chat::prepare_cloud().await.map_err(to_msg)
}

/// What the model has cost since counting started, per provider.
///
/// Its own command rather than a field on `chat_config`, because that one
/// probes Ollama over the network and this is a read of a few rows — the Settings
/// screen refreshes the totals after a conversation without waiting on a probe.
#[tauri::command]
fn chat_usage() -> CmdResult<chat::UsageReport> {
    let db = Db::open_default().map_err(to_msg)?;
    chat::usage_report(&db).map_err(to_msg)
}

/// Clear one provider's totals — by default the one currently configured, which
/// is the one Settings is showing when the button is there to be pressed.
#[tauri::command]
fn reset_chat_usage(provider: Option<String>) -> CmdResult<()> {
    let db = Db::open_default().map_err(to_msg)?;
    let provider = match provider.as_deref() {
        Some(name) => chat::Provider::parse(name).ok_or("Unknown model provider.")?,
        None => chat::load_config(&db)
            .map_err(to_msg)?
            .0
            .ok_or("No model provider chosen yet.")?,
    };
    chat::reset_usage(&db, provider).map_err(to_msg)
}

#[tauri::command]
fn set_openrouter_key(key: String) -> CmdResult<()> {
    let key = key.trim();
    if key.is_empty() {
        return Err("The key is empty.".into());
    }
    store::save_openrouter_key(key).map_err(to_msg)
}

#[tauri::command]
fn clear_openrouter_key() -> CmdResult<()> {
    store::clear_openrouter_key().map_err(to_msg)
}

/// One OpenRouter model, as much as the picker needs to rank and describe it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelInfo {
    id: String,
    /// OpenRouter's display name, e.g. "Ling-3.0-flash".
    name: String,
    /// Tokens of context, for sorting and for saying so.
    context: u64,
    /// USD per million tokens, which is the unit anyone actually compares in.
    prompt_per_m: f64,
    completion_per_m: f64,
    /// Takes a JSON schema. Not required, but better where it's offered.
    structured: bool,
}

/// Models OpenRouter offers that this app can actually use.
///
/// Filtered on tool support, which isn't a preference: every answer here comes
/// from a function call against the local cache, so a model without tools can
/// only make things up. Structured output is reported rather than required —
/// it improves the follow-up suggestions, and demanding it would rule out
/// perfectly good models, including the default.
#[tauri::command]
async fn openrouter_models() -> CmdResult<Vec<ModelInfo>> {
    let resp = reqwest::Client::new()
        .get("https://openrouter.ai/api/v1/models")
        .send()
        .await
        .map_err(|e| format!("could not reach OpenRouter: {e}"))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("OpenRouter returned an unexpected shape: {e}"))?;

    let has = |m: &serde_json::Value, p: &str| {
        m["supported_parameters"]
            .as_array()
            .is_some_and(|xs| xs.iter().any(|v| v == p))
    };
    // Prices come back as USD per token in a string, which is unreadable at
    // this scale — everything is quoted per million instead.
    let per_m = |m: &serde_json::Value, k: &str| {
        m["pricing"][k]
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0)
            * 1_000_000.0
    };

    let mut out: Vec<ModelInfo> = body["data"]
        .as_array()
        .map(|models| {
            models
                .iter()
                .filter(|m| has(m, "tools"))
                .filter_map(|m| {
                    let id = m["id"].as_str()?.to_string();
                    Some(ModelInfo {
                        name: m["name"].as_str().unwrap_or(&id).to_string(),
                        id,
                        context: m["context_length"].as_u64().unwrap_or(0),
                        prompt_per_m: per_m(m, "prompt"),
                        completion_per_m: per_m(m, "completion"),
                        structured: has(m, "structured_outputs"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by_key(|m| m.name.to_lowercase());
    Ok(out)
}

/// Starts one assistant turn. Text and progress arrive on the `chat:{id}`
/// event channel; this resolves when the turn is over.
///
/// `activity_id` scopes the conversation to one session: the analysis is put in
/// front of the model as context, so "was that too hard?" has an antecedent
/// without the question having to name a date. It is still the same tools and
/// the same cache underneath — the model can and does look past the session
/// when the answer needs the weeks around it.
#[tauri::command]
async fn chat_send(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
    history: Vec<chat::HistoryMessage>,
    activity_id: Option<i64>,
) -> CmdResult<()> {
    // Resolved before the turn starts so a failure to build it is an error on
    // the command rather than a turn that quietly answers without context.
    let context = match activity_id {
        Some(aid) => Some(activity_analysis(state, aid, None).await?),
        None => None,
    };
    chat::run_turn(app, id, history, context.as_ref())
        .await
        .map_err(to_msg)
}

/* ----------------------------------------------------------------- login --- */

#[tauri::command]
async fn garmin_login(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> CmdResult<()> {
    login::run(app).await.map_err(to_msg)?;
    // Drop the cached client so the next call rebuilds it from the new tokens.
    *state.garmin.write().await = None;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // Feeds the frontend the target OS, which is what the window chrome
        // branches on — see `lib/platform.ts`.
        .plugin(tauri_plugin_os::init());

    // `process` is what lets the app restart itself once an update is staged;
    // without it the user would have to quit and reopen by hand.
    #[cfg(desktop)]
    {
        builder = builder
            .plugin(tauri_plugin_updater::Builder::new().build())
            .plugin(tauri_plugin_process::init());
    }

    builder
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            garmin_status,
            garmin_import_tokens,
            garmin_disconnect,
            garmin_profile,
            garmin_activities,
            garmin_hr_zones,
            sync_now,
            cached_activities,
            cached_activities_since,
            cached_activity,
            cached_daily,
            cache_summary,
            nutrition,
            weight,
            set_weight_goal,
            weight_summary,
            workouts,
            create_workout,
            themes_list,
            themes_save,
            themes_delete,
            themes_dir,
            themes_open,
            routes,
            activity_splits,
            activity_details,
            activity_analysis,
            activity_critique,
            cached_activity_critique,
            activity_tags,
            set_activity_tags,
            all_tags,
            gear_list,
            chat_config,
            chat_health,
            chat_usage,
            reset_chat_usage,
            set_chat_provider,
            set_openrouter_key,
            clear_openrouter_key,
            prepare_cloud_chat,
            openrouter_models,
            chat_send,
            chat_sessions,
            chat_session,
            save_chat_session,
            delete_chat_session,
            chat_followups,
            garmin_login,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
