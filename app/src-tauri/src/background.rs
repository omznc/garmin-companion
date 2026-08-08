//! Staying useful with the window shut. Desktop only.
//!
//! Everywhere else in this app, "the coach speaks first" is a promise the
//! platform keeps: Android is handed a plan days in advance and delivers it
//! whether or not the app ever runs again. Desktop has no such queue — the
//! notification API there can show something now and nothing else — so for the
//! promise to hold, *something* has to still be running at six in the evening.
//! That something is this: a tray icon, and a minute-by-minute loop.
//!
//! Which makes the desktop nudge the opposite kind of thing to the phone's. The
//! phone's is frozen text decided days early and honest about its own age. This
//! one is decided at the moment it fires, off a sync taken minutes before, and
//! is the freshest reading the app has ever produced. The two arrive looking
//! identical and are built the opposite way round.
//!
//! None of it is on by default. An app that keeps running after you close it is
//! a thing you agree to, not a thing you discover — so the tray appears when the
//! switch in Settings is turned on and not before, and closing the window really
//! does quit until then.

use crate::{AppState, CmdResult};
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeDelta, Timelike, Utc};
use garmin_core::coach::NotifySettings;
use garmin_core::db::Db;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager};

/// Where the answer to "should closing the window quit?" is kept for the life
/// of the process.
///
/// A static rather than managed state because the window-close handler is
/// installed once, at setup, and needs the *current* answer rather than the one
/// that was true when it was installed. There is one process and one window, so
/// there is one answer.
static RESIDENT: AtomicBool = AtomicBool::new(false);

/// The tray icon's id, so it can be found again to take it away.
const TRAY_ID: &str = "main";

/// How often the loop wakes. Fine-grained enough that a nudge lands within a
/// minute of its hour, coarse enough to be free.
const TICK: Duration = Duration::from_secs(60);

/// How stale the cache may get while the app is running before it's refreshed
/// on its own.
const REFRESH_AFTER: TimeDelta = TimeDelta::hours(4);

/// How fresh it has to be for the day's nudge to be built on it. Tighter than
/// the idle refresh: this is the one reading the coach is about to speak from.
const NUDGE_FRESH: TimeDelta = TimeDelta::minutes(30);

/// Wait this long before a second attempt at the day's nudge. Offline is the
/// common failure and it lasts as long as it lasts; retrying every minute would
/// only spend a battery finding that out.
const RETRY_AFTER: TimeDelta = TimeDelta::minutes(15);

/// Days a background sync asks for — enough to cover a weekend the app spent
/// closed, without being the long pass a manual sync does.
const DAYS: u32 = 7;

/// Whether the app is currently the sort that survives its window closing.
pub fn resident() -> bool {
    RESIDENT.load(Ordering::Relaxed)
}

/// Match the app to the notification switch, at startup and whenever it moves.
///
/// There is no separate setting for this. A desktop nudge and a resident app are
/// the same thing asked for twice: the notification can only arrive at its hour
/// if something is awake at that hour, and nothing else here needs the app to
/// outlive its window.
pub fn apply(app: &tauri::AppHandle, enabled: bool) -> CmdResult<()> {
    // Not on a desktop with nowhere to put the icon. There the app quits with
    // its window as it always did, and the nudge is whatever arrives while it
    // is open — which is worse than the alternative, but the alternative is a
    // process with no window, no icon and no way to stop it.
    let enabled = enabled && tray_visible();
    RESIDENT.store(enabled, Ordering::Relaxed);

    if enabled {
        add_tray(app).map_err(|e| format!("could not add the tray icon: {e}"))?;
    } else if app.tray_by_id(TRAY_ID).is_some() {
        // Taking the icon away while the window is hidden would leave the app
        // running with nothing on screen and no way back to it.
        show_window(app);
        app.remove_tray_by_id(TRAY_ID);
        // And starting at login to sit in a tray it is no longer in would be a
        // process nobody can see and nothing asked for.
        let _ = set_start_at_login(app, false);
    }
    Ok(())
}

/// Read the notification switch and make the app match it. Once, at startup.
pub fn restore(app: &tauri::AppHandle) {
    let enabled = Db::open_default()
        .ok()
        .and_then(|db| NotifySettings::load(&db).ok())
        .is_some_and(|s| s.enabled);

    if let Err(e) = apply(app, enabled) {
        eprintln!("background: {e}");
    }
}

/// Whether the app launches itself at login.
///
/// Asked of the operating system rather than remembered, which is the only way
/// to get an answer that stays true when a login item is removed from outside
/// the app.
pub fn start_at_login(app: &tauri::AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}

pub fn set_start_at_login(app: &tauri::AppHandle, on: bool) -> CmdResult<()> {
    use tauri_plugin_autostart::ManagerExt;

    let manager = app.autolaunch();
    let result = if on {
        manager.enable()
    } else {
        manager.disable()
    };
    // Worth naming rather than swallowing: this writes a login item, a registry
    // key or a `.desktop` file depending on the platform, and when it fails the
    // switch would otherwise sit there looking as though it had worked.
    result.map_err(|e| format!("could not change the login item: {e}"))
}

/* ------------------------------------------------------------------ tray --- */

/// Whether an icon put in the tray would be seen by anyone.
///
/// Everywhere but Linux the answer is yes, because the tray is part of the
/// desktop. On Linux it is a service some other process has to be providing, and
/// GNOME does not provide it without an extension — so the call succeeds, Tauri
/// hands back a `TrayIcon`, and nothing appears anywhere. Believing that icon
/// would mean hiding the window into a place with no way back out of it, so this
/// asks the session bus whether anyone is listening first.
///
/// Answered once. A watcher that arrives later is a desktop extension being
/// switched on mid-session, which is worth a restart rather than a poll.
#[cfg(not(target_os = "linux"))]
pub fn tray_visible() -> bool {
    true
}

#[cfg(target_os = "linux")]
pub fn tray_visible() -> bool {
    use gio::prelude::*;
    use std::sync::OnceLock;

    static ANSWER: OnceLock<bool> = OnceLock::new();
    *ANSWER.get_or_init(|| {
        let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else {
            return false;
        };
        bus.call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "NameHasOwner",
            Some(&("org.kde.StatusNotifierWatcher",).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            // Milliseconds. The bus is local and this is on the startup path.
            500,
            gio::Cancellable::NONE,
        )
        .ok()
        .and_then(|reply| reply.child_value(0).get::<bool>())
        .unwrap_or(false)
    })
}

/// Bring the window back, from wherever it went — hidden, minimised, or behind
/// everything else.
pub fn show_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn add_tray(app: &tauri::AppHandle) -> anyhow::Result<()> {
    if app.tray_by_id(TRAY_ID).is_some() {
        return Ok(());
    }

    let open = MenuItem::with_id(app, "open", "Open Garmin Companion", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "sync", "Sync now", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&open, &refresh, &PredefinedMenuItem::separator(app)?, &quit],
    )?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Garmin Companion")
        .menu(&menu)
        // Left click opens the window; the menu is the right-click gesture
        // everywhere except macOS, where it is both and this is ignored.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_window(app),
            "sync" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    sync(&app, 30).await;
                });
            }
            // The only way out, once the window no longer is one.
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_window(tray.app_handle());
            }
        });

    // The window icon, which is the app icon compiled into the binary. Absent
    // only in a build with no icons configured, where a tray with no picture is
    // still better than no tray.
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    Ok(())
}

/* ------------------------------------------------------------------ loop --- */

/// What the loop remembers between ticks. Deliberately not persisted: the
/// durable "already said this today" is a claim in SQLite, which the in-app path
/// takes too. This only saves the work of finding that out every minute.
#[derive(Default)]
struct Progress {
    nudged_on: Option<NaiveDate>,
    last_attempt: Option<NaiveDateTime>,
}

/// Start the loop. Runs for the life of the process, window or no window.
pub fn start(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut progress = Progress::default();
        loop {
            tokio::time::sleep(TICK).await;
            tick(&app, &mut progress).await;
        }
    });
}

async fn tick(app: &tauri::AppHandle, progress: &mut Progress) {
    let now = Local::now().naive_local();
    let today = now.date();

    let Some((notify, last_sync)) = read(|db| {
        let last = db
            .sync_state("last_sync")?
            .and_then(|raw| DateTime::parse_from_rfc3339(&raw).ok())
            .map(|t| t.with_timezone(&Utc));
        Ok((NotifySettings::load(db)?, last))
    })
    .await
    else {
        return;
    };

    // Never synced counts as old enough for anything.
    let older_than = |d: TimeDelta| last_sync.is_none_or(|t| Utc::now() - t > d);

    // The day's nudge, once its hour has come. The claim in SQLite is what
    // stops this happening twice; `nudged_on` only stops it being *asked* twice.
    let due = notify.enabled
        && progress.nudged_on != Some(today)
        && now.hour() >= notify.hour()
        && progress.last_attempt.is_none_or(|t| now - t >= RETRY_AFTER);

    if due {
        progress.last_attempt = Some(now);
        if older_than(NUDGE_FRESH) {
            sync(app, DAYS).await;
        }
        // A failed sync is not a reason to stay quiet. The nudge would be built
        // on numbers a few hours old, which is what every phone notification is
        // built on and says so.
        if crate::schedule_nudges(app.clone()).await.is_ok() {
            progress.nudged_on = Some(today);
        }
        return;
    }

    // Otherwise, keep the cache warm — so that opening the window shows today
    // rather than whenever it was last opened, and so the evening's nudge is
    // never the first thing to notice the network is down.
    if older_than(REFRESH_AFTER) {
        sync(app, DAYS).await;
    }
}

/// Read something off the cache without holding up the runtime.
///
/// `None` for every failure, including "no database yet": a loop that wakes
/// again in a minute has no use for the reason.
async fn read<T, F>(f: F) -> Option<T>
where
    T: Send + 'static,
    F: FnOnce(&Db) -> anyhow::Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(move || f(&Db::open_default().ok()?).ok())
        .await
        .ok()
        .flatten()
}

/// Pull from Garmin, quietly. False when it didn't happen, for any reason —
/// not signed in, no network, or a sync already running.
async fn sync(app: &tauri::AppHandle, days: u32) -> bool {
    let state = app.state::<AppState>();
    let Ok(client) = state.client().await else {
        return false; // no account connected yet
    };
    // `try_lock` rather than waiting: whatever holds this is a sync of its own,
    // so the work is being done either way and the loop can come back later.
    let Ok(_busy) = state.syncing.try_lock() else {
        return false;
    };

    let handle = tokio::runtime::Handle::current();
    let done = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let db = Db::open_default()?;
        // No progress callback. A sync nobody asked for should not narrate
        // itself over a screen someone is reading.
        let quiet = |_: garmin_core::sync::SyncProgress| {};
        handle.block_on(garmin_core::sync::sync_all_with(
            &client, &db, days, false, &quiet,
        ))?;
        Ok(())
    })
    .await
    .is_ok_and(|r| r.is_ok());

    // The window, if there is one, is showing numbers this just replaced.
    if done {
        let _ = app.emit("background:synced", ());
    }
    done
}
