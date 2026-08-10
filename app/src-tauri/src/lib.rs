mod chat;
mod login;
/// The last step of turning a screen into a shareable image — the sharesheet on
/// Android, the clipboard on a desktop. The card itself is drawn in the
/// frontend; see `components/ShareCard.tsx`.
mod share;

/// The tray, the login item, and the loop that fires the coach's nudge at the
/// hour it was asked for. Desktop only — the phone has the system do all three.
#[cfg(desktop)]
mod background;

/// Public because `main` has to call into it before `run()` — see the module
/// docs for why that ordering is the whole point.
#[cfg(target_os = "linux")]
pub mod linux;

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
    /// Held for the length of a sync, so that only one runs at a time.
    ///
    /// The frontend already refuses to start a second — see `lib/syncProgress`
    /// — but it is no longer the only thing that starts one: the background loop
    /// syncs on its own schedule, and the two would otherwise meet in the middle
    /// of the same tables. The loop takes this without waiting and gives up if
    /// it can't have it, because a sync it wanted is a sync already happening.
    syncing: tokio::sync::Mutex<()>,
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
    // Waits, where the background loop wouldn't: this one was asked for, and
    // someone is watching a progress bar for it.
    let _busy = state.syncing.lock().await;

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
    db.daily_since(&garmin_core::days_ago(days.unwrap_or(30)))
        .map_err(to_msg)
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

/* -------------------------------------------------------------- strength --- */

/// Recent strength sessions, summarised from their sets.
///
/// There is no load in this data — see `garmin_core::strength` for why — so
/// nothing here reports volume, and the screen must not imply one.
#[tauri::command]
fn strength_sessions(limit: Option<u32>) -> CmdResult<query::StrengthReport> {
    let db = Db::open_default().map_err(to_msg)?;
    query::strength_trend(&db, limit.unwrap_or(20)).map_err(to_msg)
}

/// One session with its sets in order, for the set-by-set timeline. `None` when
/// the sets haven't been synced yet.
#[tauri::command]
fn strength_session(
    activity_id: i64,
) -> CmdResult<Option<(garmin_core::StrengthSession, Vec<garmin_core::ExerciseSet>)>> {
    let db = Db::open_default().map_err(to_msg)?;
    query::strength_session(&db, activity_id).map_err(to_msg)
}

/* -------------------------------------------------------------- findings --- */

/// The deep findings, computed in `garmin-core` so the Insights screen and the
/// coach are reading the same analysis rather than two implementations of it.
///
/// The window is a year because most of these need one: a weekday pattern wants
/// months of weekdays, and a fitness trend at a fixed heart rate wants every
/// comparable run there has ever been.
#[tauri::command]
fn findings(days: Option<u32>) -> CmdResult<Vec<garmin_core::findings::Finding>> {
    let db = Db::open_default().map_err(to_msg)?;
    let from = garmin_core::days_ago(days.unwrap_or(365));
    let daily = db.daily_since(&from).map_err(to_msg)?;
    let activities = db.activities_since(&from).map_err(to_msg)?;
    Ok(garmin_core::findings::all(
        &daily,
        &activities,
        chrono::Local::now().date_naive(),
    ))
}

/* --------------------------------------------------------------- fitness --- */

#[tauri::command]
fn personal_records() -> CmdResult<Vec<garmin_core::PersonalRecord>> {
    let db = Db::open_default().map_err(to_msg)?;
    query::personal_records(&db).map_err(to_msg)
}

/// Garmin's own verdict — status, acute/chronic load, load balance, VO2 max and
/// race predictions — plus however much history the cache has accumulated.
#[tauri::command]
fn fitness(days: Option<u32>) -> CmdResult<query::FitnessReport> {
    let db = Db::open_default().map_err(to_msg)?;
    query::fitness(&db, days.unwrap_or(90)).map_err(to_msg)
}

/* ----------------------------------------------------------------- sleep --- */

/// Last night in full, the window behind it, and what the two say.
///
/// A month by default: long enough for bedtime consistency to mean something,
/// short enough that the answer is about how you're sleeping now.
#[tauri::command]
fn sleep(days: Option<u32>) -> CmdResult<garmin_core::sleep::SleepReport> {
    let db = Db::open_default().map_err(to_msg)?;
    query::sleep(&db, days.unwrap_or(30)).map_err(to_msg)
}

/* ----------------------------------------------------------------- coach --- */

#[tauri::command]
fn goals() -> CmdResult<garmin_core::Goals> {
    let db = Db::open_default().map_err(to_msg)?;
    garmin_core::Goals::load(&db).map_err(to_msg)
}

#[tauri::command]
fn set_goals(goals: garmin_core::Goals) -> CmdResult<garmin_core::Goals> {
    let db = Db::open_default().map_err(to_msg)?;
    goals.save(&db).map_err(to_msg)?;
    Ok(goals)
}

/// The week against the goals, and anything the coach has to say about it.
///
/// Local date rather than UTC: a nudge about "this week" has to agree with the
/// calendar on the wall, and at 01:00 in Europe/Paris those differ.
#[tauri::command]
fn coach() -> CmdResult<garmin_core::coach::CoachReport> {
    let db = Db::open_default().map_err(to_msg)?;
    garmin_core::coach::for_today(&db, chrono::Local::now().date_naive()).map_err(to_msg)
}

/// Put one nudge away for the day. It comes back tomorrow if the condition
/// behind it hasn't cleared — dismissing is not disagreeing.
#[tauri::command]
fn dismiss_nudge(id: String) -> CmdResult<()> {
    let db = Db::open_default().map_err(to_msg)?;
    let today = chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string();
    db.dismiss_nudge(&id, &today).map_err(to_msg)
}

/// Whether we may post a notification, asking for the right if we haven't yet.
///
/// Android 13 and up needs `POST_NOTIFICATIONS` granted at runtime, and without
/// it `show()` succeeds and nothing appears — a silent failure, which is the
/// worst shape for this to take.
///
/// The ask deliberately happens here, deep inside sending, rather than at
/// launch. Android only shows the dialog once or twice before it starts
/// refusing on the user's behalf for good, so the ask is worth spending well: by
/// the time this runs there is a real nudge waiting, and the prompt arrives
/// attached to a reason instead of on a cold first launch before the app has
/// ever had anything to say.
///
/// Blocking — it waits on a dialog — so callers hand it to a blocking thread.
/// On desktop there is no such permission and the plugin answers `Granted`.
fn notifications_allowed(app: &tauri::AppHandle) -> bool {
    use tauri::plugin::PermissionState;
    use tauri_plugin_notification::NotificationExt;

    match app.notification().permission_state() {
        Ok(PermissionState::Granted) => true,
        // What Android reports once it has stopped showing the dialog at all.
        // Asking again is a round trip that can only return this same answer.
        Ok(PermissionState::Denied) => false,
        // `Prompt`, or `PromptWithRationale` after one refusal. The rationale
        // is the nudge itself, which is why we only get here holding one.
        Ok(_) => matches!(
            app.notification().request_permission(),
            Ok(PermissionState::Granted)
        ),
        // A launch is not worth failing over a permission we couldn't read.
        Err(_) => false,
    }
}

/// When the coach may interrupt, and whether it may at all.
#[tauri::command]
fn notification_settings() -> CmdResult<garmin_core::coach::NotifySettings> {
    let db = Db::open_default().map_err(to_msg)?;
    garmin_core::coach::NotifySettings::load(&db).map_err(to_msg)
}

/// On desktop this is also the tray switch. Nothing there can deliver a
/// notification at six in the evening unless the app is still running at six in
/// the evening, so wanting the nudge is what puts the app in the tray, and
/// turning the nudge off takes it back out.
#[tauri::command]
fn set_notification_settings(
    app: tauri::AppHandle,
    settings: garmin_core::coach::NotifySettings,
) -> CmdResult<garmin_core::coach::NotifySettings> {
    let db = Db::open_default().map_err(to_msg)?;
    settings.save(&db).map_err(to_msg)?;

    #[cfg(desktop)]
    background::apply(&app, settings.enabled)?;
    #[cfg(mobile)]
    let _ = app;

    Ok(settings)
}

/// What `schedule_nudges` left in place.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct NudgeSchedule {
    /// Everything now queued with the system, soonest first. Empty is the
    /// common case and not an error.
    planned: Vec<garmin_core::coach::PlannedNudge>,
    /// False when the platform refused, so the frontend can say why nothing is
    /// queued instead of showing an empty list that looks like a bug.
    permitted: bool,
    /// False on desktop, where the plugin has no scheduling at all.
    supported: bool,
    /// Whether the app will still be there at the hour to show it. Always true
    /// on a phone, where the system holds the plan; on a desktop it means the
    /// tray, and so is false when this one hasn't got one.
    resident: bool,
}

/// The id block the coach's scheduled notifications occupy.
///
/// Fixed and contiguous so rescheduling is cancel-then-schedule over a known
/// range: the app cannot ask the system what it queued last run on every
/// platform, but it can always cancel a range it chose itself.
#[cfg(mobile)]
const NUDGE_NOTIFICATION_IDS: std::ops::Range<i32> = 7100..7116;

/// Hand the system the coach's next few days of nudges.
///
/// This is the whole proactive half of the coach, and it works the way it does
/// because the app has no background execution: nothing evaluates the rules
/// while the app is closed, so every notification has to be queued in advance
/// from the last plan that was made. Called on launch and after every sync, and
/// each call cancels the previous plan before laying down a new one — so the
/// text is never older than the last time the app was open.
///
/// Desktop takes the other path. The plugin ignores `schedule` there, so the
/// nudge is shown when its hour arrives and the app is the thing awake to notice
/// — which is what `background` exists to make possible. Called on launch, after
/// every sync, and once a minute by the background loop.
#[tauri::command]
async fn schedule_nudges(app: tauri::AppHandle) -> CmdResult<NudgeSchedule> {
    let now = chrono::Local::now().naive_local();

    // Read first, and let the connection go before anything can block: what
    // follows may sit on a permission dialog for as long as it takes someone to
    // notice their phone.
    let (settings, planned) = {
        let db = Db::open_default().map_err(to_msg)?;
        let settings = garmin_core::coach::NotifySettings::load(&db).map_err(to_msg)?;
        let report = garmin_core::coach::for_today(&db, now.date()).map_err(to_msg)?;
        let planned = garmin_core::coach::plan_notifications(&report, &settings, now);
        (settings, planned)
    };

    deliver(app, planned, settings).await
}

#[cfg(mobile)]
async fn deliver(
    app: tauri::AppHandle,
    planned: Vec<garmin_core::coach::PlannedNudge>,
    _settings: garmin_core::coach::NotifySettings,
) -> CmdResult<NudgeSchedule> {
    use tauri_plugin_notification::{NotificationExt, Schedule};

    // Clear the old plan first and unconditionally — including when there is
    // nothing to replace it with. A nudge that has stopped being true has to
    // stop being scheduled, and switching notifications off has to actually
    // silence the ones already queued.
    let ids: Vec<i32> = NUDGE_NOTIFICATION_IDS.collect();
    let _ = app.notification().cancel(ids);

    if planned.is_empty() {
        return Ok(NudgeSchedule {
            planned,
            permitted: true,
            supported: true,
            resident: true,
        });
    }

    // Asking only once there is something to say — see `notifications_allowed`.
    let handle = app.clone();
    let permitted = tauri::async_runtime::spawn_blocking(move || notifications_allowed(&handle))
        .await
        .unwrap_or(false);
    if !permitted {
        return Ok(NudgeSchedule {
            planned: Vec::new(),
            permitted: false,
            supported: true,
            resident: true,
        });
    }

    let mut queued = Vec::new();
    for (slot, nudge) in planned.into_iter().enumerate() {
        let Some(id) = NUDGE_NOTIFICATION_IDS.start.checked_add(slot as i32) else {
            break;
        };
        // A local wall-clock time is a instant only once the zone is applied,
        // and the plugin wants it as one.
        let Some(at) = local_instant(nudge.at) else {
            continue;
        };

        let result = app
            .notification()
            .builder()
            .id(id)
            .title(&nudge.title)
            .body(&nudge.body)
            .schedule(Schedule::At {
                date: at,
                repeating: false,
                // Doze can otherwise hold a nudge until the phone is next
                // picked up, which for an evening notification often means the
                // following morning — by which time it is about the wrong day.
                allow_while_idle: true,
            })
            .show();
        if result.is_ok() {
            queued.push(nudge);
        }
    }

    Ok(NudgeSchedule {
        planned: queued,
        permitted: true,
        supported: true,
        resident: true,
    })
}

/// A local wall-clock time as the absolute instant the system schedules against.
///
/// `single()` is `None` twice a year: an hour that DST skipped never happens, and
/// one it repeats happens twice. Taking the earliest of an ambiguous pair and
/// giving up on an impossible one costs at most a single day's nudge, which is
/// the right price for not guessing.
#[cfg(mobile)]
fn local_instant(at: chrono::NaiveDateTime) -> Option<time::OffsetDateTime> {
    use chrono::TimeZone;

    let local = chrono::Local
        .from_local_datetime(&at)
        .earliest()
        .or_else(|| chrono::Local.from_local_datetime(&at).single())?;
    time::OffsetDateTime::from_unix_timestamp(local.timestamp()).ok()
}

/// Desktop: no scheduling, so the first entry of the plan is shown now and the
/// rest are dropped. The once-a-day claim is a compare-and-set in SQLite rather
/// than a flag in the frontend, so opening the app twice still produces one
/// notification a day.
///
/// "Now" is not any moment this is called, though — only one at or after the
/// hour in Settings. That hour used to be ignored here, because the only time a
/// desktop nudge could be shown was while the app happened to be open and
/// waiting for six o'clock would have meant showing almost nothing. With the
/// background loop there is something awake at six o'clock, so the hour means
/// what it says; and opening the app later in the day still catches up, because
/// later is also "at or after".
#[cfg(desktop)]
async fn deliver(
    app: tauri::AppHandle,
    planned: Vec<garmin_core::coach::PlannedNudge>,
    settings: garmin_core::coach::NotifySettings,
) -> CmdResult<NudgeSchedule> {
    use chrono::Timelike;
    use tauri_plugin_notification::NotificationExt;

    let none = |permitted| NudgeSchedule {
        planned: Vec::new(),
        permitted,
        supported: false,
        resident: background::resident(),
    };

    if chrono::Local::now().hour() < settings.hour() {
        return Ok(none(true));
    }

    let Some(nudge) = planned.into_iter().next() else {
        return Ok(none(true));
    };

    let handle = app.clone();
    let permitted = tauri::async_runtime::spawn_blocking(move || notifications_allowed(&handle))
        .await
        .unwrap_or(false);
    if !permitted {
        return Ok(none(false));
    }

    {
        let db = Db::open_default().map_err(to_msg)?;
        let today = chrono::Local::now()
            .date_naive()
            .format("%Y-%m-%d")
            .to_string();
        if !db
            .claim_nudge_notification(&nudge.nudge_id, &today)
            .map_err(to_msg)?
        {
            return Ok(none(true));
        }
    }

    app.notification()
        .builder()
        .title(&nudge.title)
        .body(&nudge.body)
        .show()
        .map_err(|e| format!("could not show the notification: {e}"))?;

    Ok(NudgeSchedule {
        planned: vec![nudge],
        permitted: true,
        supported: false,
        resident: background::resident(),
    })
}

/* ------------------------------------------------------------ background --- */

/// Whether the app launches itself at login. Always false on a phone, which has
/// no login item and doesn't need one; the commands exist on both platforms
/// because the handler list does.
#[tauri::command]
fn start_at_login(app: tauri::AppHandle) -> CmdResult<bool> {
    #[cfg(desktop)]
    return Ok(background::start_at_login(&app));
    #[cfg(mobile)]
    {
        let _ = app;
        Ok(false)
    }
}

#[tauri::command]
fn set_start_at_login(app: tauri::AppHandle, on: bool) -> CmdResult<bool> {
    #[cfg(desktop)]
    return background::set_start_at_login(&app, on).map(|()| background::start_at_login(&app));
    #[cfg(mobile)]
    {
        let _ = (app, on);
        Ok(false)
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

/// The written opening of the Today screen.
///
/// Regenerated at most once a day — the fingerprint carries the date, so the
/// paragraph rewrites when the calendar turns or when a sync moves the numbers,
/// and not when the screen is merely reopened.
#[tauri::command]
async fn today_summary(force: Option<bool>) -> CmdResult<chat::TodaySummary> {
    chat::today_summary(force.unwrap_or(false))
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
    analysis_for(&state, activity_id, force.unwrap_or(false)).await
}

/// The body of the command above, callable without a `tauri::State`.
///
/// Split out for `chat::activity_analysis`, which offers the same read to the
/// model as a tool. The alternative was a second implementation, and the
/// difference between two of these is exactly the kind that goes unnoticed: the
/// athlete reads one number on the activity screen and the coach quotes another
/// for the same session.
pub(crate) async fn analysis_for(
    state: &AppState,
    activity_id: i64,
    force: bool,
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
        let cached = if force {
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

/// Answer a question the model asked mid-turn, which unparks it.
///
/// Returns whether anything was still waiting: the turn may have been stopped or
/// timed out between the card being drawn and the button being pressed, and the
/// frontend uses the answer to decide whether the card locks or greys out.
#[tauri::command]
fn chat_answer(id: String, call_id: String, answers: Vec<String>) -> CmdResult<bool> {
    Ok(chat::answer_ask(&id, &call_id, answers))
}

/// Stop a turn in flight. Whatever has already streamed stands as the answer.
///
/// Deliberately infallible: an id that no longer names a live turn means it
/// finished while the button was being pressed, which is the outcome that was
/// asked for.
#[tauri::command]
fn chat_cancel(id: String) -> CmdResult<()> {
    chat::cancel(&id);
    Ok(())
}

/* ----------------------------------------------------------------- login --- */

#[tauri::command]
async fn garmin_login(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> CmdResult<()> {
    login::run(app).await.map_err(to_msg)?;
    // Drop the cached client so the next call rebuilds it from the new tokens.
    *state.garmin.write().await = None;
    Ok(())
}

/// Why the last sign-in failed, for a frontend that wasn't running when it did.
///
/// Only mobile can be in that position: signing in there navigates the one
/// webview to Garmin and back, so the page that called `garmin_login` is gone
/// before there is anything to tell it. Desktop gets the error as the command's
/// own `Err` and this always answers `None` — the setup screen calls it either
/// way rather than branching on the platform for one string.
#[tauri::command]
fn garmin_login_error() -> CmdResult<Option<String>> {
    #[cfg(mobile)]
    {
        Ok(login::take_last_error())
    }
    #[cfg(desktop)]
    {
        Ok(None)
    }
}

/* ------------------------------------------------------ android update --- */

/// Where the Android build looks for a new version.
///
/// The second hardcoded reference to the repo slug — `plugins.updater.endpoints`
/// in `tauri.conf.json` is the other, and it is the desktop equivalent of this
/// line. They are separate because they describe separate artifacts, but they
/// move together: if the repo is ever renamed, both change or half the installs
/// stop hearing about releases. RELEASING says so in one place.
///
/// `releases/latest/download/…` rather than a pinned URL, because GitHub
/// redirects it to whichever release is currently published — which is also why
/// a draft release is invisible to this until someone hits publish.
const ANDROID_MANIFEST: &str =
    "https://github.com/omznc/garmin-companion/releases/latest/download/latest-android.json";

/// What `.github/workflows/release.yml` writes beside the APK.
#[derive(Serialize, serde::Deserialize)]
pub struct ApkRelease {
    version: String,
    url: String,
    /// Lowercased on the way through, since it's compared against a digest this
    /// process computes rather than against the string GitHub stored.
    sha256: String,
}

/// Read the published Android manifest.
///
/// Asked for from Rust rather than from the page, which is the entire reason
/// this command exists. `releases/latest/download/…` is a redirect to a host
/// that sends no `Access-Control-Allow-Origin`, so the identical request made
/// by `fetch` in the webview is refused before its body can be read — not
/// because the release, the manifest or the network were wrong, but because a
/// document served from `tauri.localhost` isn't allowed to look at github.com.
/// It fails the same way for every release, which is what made it look like
/// there was never a new one. `reqwest` is not a browser and is under no such
/// rule.
///
/// `None` means there is nothing published to read — no release yet, or one
/// whose Android job hasn't finished uploading. An `Err` means the asking
/// itself failed. Those are kept apart because the frontend turns the first
/// into "up to date", and it should only ever say that when it knows.
#[tauri::command]
async fn latest_apk() -> CmdResult<Option<ApkRelease>> {
    let resp = reqwest::Client::new()
        .get(ANDROID_MANIFEST)
        .send()
        .await
        .map_err(|e| format!("couldn't reach the update server: {e}"))?;

    // A 404 is the ordinary "no manifest attached to the current release", not
    // a fault worth reporting.
    if !resp.status().is_success() {
        return Ok(None);
    }
    let Ok(m) = resp.json::<ApkRelease>().await else {
        return Ok(None);
    };
    if m.version.is_empty() || m.url.is_empty() || m.sha256.is_empty() {
        return Ok(None);
    }
    Ok(Some(ApkRelease {
        sha256: m.sha256.to_lowercase(),
        ..m
    }))
}

/// Where a fetched APK ended up, and whether this call is what put it there.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StagedApk {
    path: String,
    /// False when the file was already on disk from an earlier launch. The
    /// frontend uses it to decide whether to offer the install immediately or
    /// wait — see `lib/updater.ts`.
    fresh: bool,
}

/// Fetch the Android APK for `version` into the cache, and hand back the path.
///
/// Android is the only caller — a desktop build updates itself through
/// `tauri-plugin-updater`, which does all of this and the install too. That
/// plugin can't be used here because the last step is a `PackageInstaller`
/// session rather than a file swap (see `ApkInstaller` on the Kotlin side), and
/// what's left once you remove that step is small enough to be this.
///
/// The download is resumable only in the coarsest sense: a launch that gets
/// interrupted leaves a `.part` behind and starts over next time. Worth having
/// anyway, because the *completed* file survives — a phone that downloaded an
/// update yesterday and was closed before installing it doesn't pay for it
/// twice.
///
/// `sha256` is checked against what arrived rather than trusted from it. It is
/// not the security boundary — Android refuses any APK not signed with this
/// app's key, whatever this function says — it's the difference between a
/// truncated download failing here, with a sentence, and failing inside the
/// system installer as "App not installed".
#[tauri::command]
async fn download_apk(
    app: tauri::AppHandle,
    url: String,
    version: String,
    sha256: String,
) -> CmdResult<StagedApk> {
    use futures_util::StreamExt;
    use sha2::{Digest, Sha256};
    use std::io::Write;
    use tauri::Manager;

    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("nowhere to put the download: {e}"))?
        .join("updates");
    std::fs::create_dir_all(&dir).map_err(|e| format!("nowhere to put the download: {e}"))?;

    let want = sha256.to_lowercase();
    let apk = dir.join(format!("garmin-companion_{version}.apk"));

    // Anything that isn't the version being asked for is a download that was
    // superseded or installed, and is dead weight in the cache — tens of
    // megabytes of it. Swept here rather than after a successful install,
    // because a successful install means this process is being replaced.
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            if e.path() != apk {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }

    if apk.is_file() && digest_of(&apk)? == want {
        return Ok(StagedApk {
            path: apk.to_string_lossy().into_owned(),
            fresh: false,
        });
    }

    let resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("couldn't reach the download: {e}"))?
        .error_for_status()
        .map_err(|e| format!("the download isn't there: {e}"))?;

    // Absent whenever the response is chunked, which is why every progress
    // report below is allowed to have no denominator.
    let total = resp.content_length().unwrap_or(0);

    let part = apk.with_extension("part");
    let mut file =
        std::fs::File::create(&part).map_err(|e| format!("couldn't open the download: {e}"))?;
    let mut hasher = Sha256::new();
    let mut stream = resp.bytes_stream();
    let mut received: u64 = 0;
    let mut announced: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("the download stopped early: {e}"))?;
        hasher.update(&chunk);
        file.write_all(&chunk)
            .map_err(|e| format!("couldn't write the download: {e}"))?;
        received += chunk.len() as u64;

        // Per chunk would be thousands of events across a webview bridge for a
        // bar that is 260 pixels wide. A quarter of a megabyte is finer than
        // one of those pixels on any APK this app will ever be.
        if received - announced >= 256 * 1024 {
            announced = received;
            let _ = app.emit("apk-download", ApkProgress { received, total });
        }
    }
    file.flush()
        .map_err(|e| format!("couldn't write the download: {e}"))?;
    drop(file);

    let got = hex(&hasher.finalize());
    if got != want {
        let _ = std::fs::remove_file(&part);
        return Err("the download didn't arrive intact".to_string());
    }

    // Renamed only once it's whole, so the name never refers to a partial file
    // — which is what lets the check at the top of this function trust one it
    // finds on a later launch.
    std::fs::rename(&part, &apk).map_err(|e| format!("couldn't finish the download: {e}"))?;
    let _ = app.emit(
        "apk-download",
        ApkProgress {
            received,
            total: received,
        },
    );

    Ok(StagedApk {
        path: apk.to_string_lossy().into_owned(),
        fresh: true,
    })
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApkProgress {
    received: u64,
    /// Zero when the server didn't say, which the frontend shows as an
    /// indeterminate bar rather than as 0%.
    total: u64,
}

fn digest_of(path: &std::path::Path) -> CmdResult<String> {
    use sha2::{Digest, Sha256};

    let mut file =
        std::fs::File::open(path).map_err(|e| format!("couldn't read the download: {e}"))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)
        .map_err(|e| format!("couldn't read the download: {e}"))?;
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Publishes whether a transparent pixel in this window shows the desktop
/// behind it, which decides whether the app cuts its own rounded corners.
///
/// A plugin with an init script rather than a command, for the same reason
/// `tauri-plugin-os` is one: the corner radius has to be right on the first
/// paint, and an IPC round-trip would round the window a frame after it was
/// already on screen. Read by `lib/platform.ts`.
///
/// Only Linux has anything to decide — see `linux::composites_alpha`. macOS
/// has `macos-private-api` on and Windows never cuts corners in CSS at all.
fn surface_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    #[cfg(target_os = "linux")]
    let composites = linux::composites_alpha();
    #[cfg(not(target_os = "linux"))]
    let composites = true;

    tauri::plugin::Builder::new("surface")
        .js_init_script(format!(
            "Object.defineProperty(window,'__GARMIN_COMPOSITES_ALPHA__',{{value:{composites}}});"
        ))
        .build()
}

/// Passed by the login item, and by nothing else. See the `autostart` plugin
/// below and `background`'s module docs.
#[cfg(desktop)]
const BACKGROUND_FLAG: &str = "--background";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();

    // Before every other plugin, which is this one's own requirement.
    //
    // It earns its place the moment the app can outlive its window: opening it
    // again from the launcher would otherwise start a second copy, with a second
    // background loop syncing into the same database, while the first sat in the
    // tray wondering where everyone went. Instead the second launch hands its
    // arguments to the first and stops, and the first shows itself — which is
    // also the way back in on a desktop whose tray never displayed the icon.
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            background::show_window(app);
        }));
    }

    #[allow(unused_mut)]
    let mut builder = builder
        .plugin(tauri_plugin_opener::init())
        // Feeds the frontend the target OS, which is what the window chrome
        // branches on — see `lib/platform.ts`.
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(surface_plugin());

    // `process` is what lets the app restart itself once an update is staged;
    // without it the user would have to quit and reopen by hand.
    //
    // `autostart` writes the login item behind the switch in Settings. The flag
    // is how the app knows, on the next login, that it was started by the system
    // rather than by a person — and so should go straight to the tray instead of
    // putting a window in front of someone who was opening their laptop to do
    // something else.
    #[cfg(desktop)]
    {
        builder = builder
            .plugin(tauri_plugin_updater::Builder::new().build())
            .plugin(tauri_plugin_process::init())
            // Where "share" lands on a desktop, which has no sharesheet to
            // hand an image to — see `share.rs`.
            .plugin(tauri_plugin_clipboard_manager::init())
            .plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                Some(vec![BACKGROUND_FLAG]),
            ))
            .setup(|app| {
                use tauri::Manager;

                let handle = app.handle().clone();
                background::restore(&handle);
                background::start(&handle);

                if let Some(window) = app.get_webview_window("main") {
                    // The window is built hidden — see `tauri.conf.json` — so
                    // that a login-time launch never shows one for a frame.
                    // Every other launch shows it here, as early as there is
                    // anything to show.
                    //
                    // And so does a login-time launch with no tray icon to have
                    // gone to, which is not hypothetical: a GNOME desktop with
                    // no AppIndicator extension has nowhere to put one. A window
                    // nobody asked for beats a process nobody can find.
                    let hide =
                        std::env::args().any(|a| a == BACKGROUND_FLAG) && background::resident();
                    if !hide {
                        let _ = window.show();
                    }

                    // Closing the window is only quitting when there is nowhere
                    // else for the app to be. When there is a tray icon, it is
                    // where the app goes.
                    let handle = handle.clone();
                    window.on_window_event(move |event| {
                        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                            if background::resident() {
                                api.prevent_close();
                                if let Some(w) = handle.get_webview_window("main") {
                                    let _ = w.hide();
                                }
                            }
                        }
                    });
                }

                Ok(())
            });
    }

    // Android hands an app its private directory at runtime; there is no
    // convention to derive one from, and `dirs::data_dir()` there answers
    // `None`. `garmin-core` would have nowhere to put the cache or the themes,
    // so it is told before anything opens either.
    //
    // Also where the main window gets built on mobile, rather than being
    // created from the config: it needs a navigation handler for the sign-in
    // flow, and that can only be attached to a window as it is made. See
    // `login`, and `tauri.android.conf.json`, which leaves `windows` empty so
    // there is no second one.
    #[cfg(mobile)]
    {
        builder = builder.setup(|app| {
            use tauri::Manager;

            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            garmin_core::paths::set_base_dir(dir);

            tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("Garmin Companion")
            .on_navigation(login::intercept)
            .build()?;

            Ok(())
        });
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
            today_summary,
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
            chat_answer,
            chat_cancel,
            chat_sessions,
            chat_session,
            save_chat_session,
            delete_chat_session,
            chat_followups,
            garmin_login,
            garmin_login_error,
            latest_apk,
            download_apk,
            strength_sessions,
            findings,
            strength_session,
            personal_records,
            fitness,
            sleep,
            goals,
            set_goals,
            coach,
            dismiss_nudge,
            schedule_nudges,
            notification_settings,
            set_notification_settings,
            start_at_login,
            set_start_at_login,
            share::share_image,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
