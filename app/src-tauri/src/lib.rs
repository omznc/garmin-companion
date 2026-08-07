mod chat;
mod login;

use anyhow::Result;
use garmin_core::{db::Db, query, store, CachedActivity, GarminClient};
use serde::Serialize;
use std::sync::Arc;
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
async fn sync_now(
    state: tauri::State<'_, AppState>,
    days: Option<u32>,
    full: Option<bool>,
) -> CmdResult<serde_json::Value> {
    let client = state.client().await?;
    let (days, full) = (days.unwrap_or(30), full.unwrap_or(false));

    let handle = tokio::runtime::Handle::current();
    let report = tokio::task::spawn_blocking(move || -> Result<_> {
        let db = Db::open_default()?;
        handle.block_on(garmin_core::sync::sync_all(&client, &db, days, full))
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

/// Saved Garmin workouts, for the Plan screen.
#[tauri::command]
fn workouts() -> CmdResult<Vec<garmin_core::db::Workout>> {
    let db = Db::open_default().map_err(to_msg)?;
    db.workouts().map_err(to_msg)
}

/// Cached GPS traces grouped into repeated routes.
#[tauri::command]
fn routes() -> CmdResult<Vec<query::Route>> {
    let db = Db::open_default().map_err(to_msg)?;
    query::routes(&db).map_err(to_msg)
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

    let http = reqwest::Client::new();
    let (ollama_reachable, ollama_models) = chat::probe_ollama(&http).await;

    Ok(chat::ChatConfig {
        provider: provider.map(|p| match p {
            chat::Provider::Openrouter => "openrouter",
            chat::Provider::Ollama => "ollama",
        }),
        model,
        has_key,
        ollama_reachable,
        ollama_models,
    })
}

#[tauri::command]
fn set_chat_provider(provider: String, model: String) -> CmdResult<()> {
    let p = match provider.as_str() {
        "openrouter" => chat::Provider::Openrouter,
        "ollama" => chat::Provider::Ollama,
        other => return Err(format!("unknown provider: {other}")),
    };
    let db = Db::open_default().map_err(to_msg)?;
    chat::save_config(&db, p, &model).map_err(to_msg)
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

/// Models OpenRouter currently offers that advertise tool support. Every tool
/// in this app is a function call, so a model without it can't answer anything.
#[tauri::command]
async fn openrouter_models() -> CmdResult<Vec<String>> {
    let resp = reqwest::Client::new()
        .get("https://openrouter.ai/api/v1/models")
        .send()
        .await
        .map_err(|e| format!("could not reach OpenRouter: {e}"))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("OpenRouter returned an unexpected shape: {e}"))?;

    let mut ids: Vec<String> = body["data"]
        .as_array()
        .map(|models| {
            models
                .iter()
                .filter(|m| {
                    m["supported_parameters"]
                        .as_array()
                        .is_some_and(|p| p.iter().any(|v| v == "tools"))
                })
                .filter_map(|m| m["id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    ids.sort();
    Ok(ids)
}

/// Starts one assistant turn. Text and progress arrive on the `chat:{id}`
/// event channel; this resolves when the turn is over.
#[tauri::command]
async fn chat_send(
    app: tauri::AppHandle,
    id: String,
    history: Vec<chat::HistoryMessage>,
) -> CmdResult<()> {
    chat::run_turn(app, id, history).await.map_err(to_msg)
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
    let mut builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());

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
            workouts,
            routes,
            activity_splits,
            activity_details,
            gear_list,
            chat_config,
            set_chat_provider,
            set_openrouter_key,
            clear_openrouter_key,
            openrouter_models,
            chat_send,
            garmin_login,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
