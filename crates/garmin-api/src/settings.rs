//! The Settings view model and the small preferences file.
//!
//! The old app kept the theme in `localStorage`, which the rewrite loses. The
//! new apps keep it in `prefs.json` in the data folder. The core builds the
//! sentences. The UIs only draw them.

use crate::{session, ApiError};
use chrono::{DateTime, Local, NaiveDate, Utc};
use garmin_core::{paths, secrets};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

const PREFS_FILE: &str = "prefs.json";

/// The app version. The release build sets `APP_VERSION` for the compiler, so
/// the number is the release version. A local build shows the crate version.
pub const APP_VERSION: &str = match option_env!("APP_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

/// How the app picks light or dark.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
#[serde(default)]
pub struct Prefs {
    pub theme: ThemeMode,
    /// The sidebar entries in the order the user chose, as stable keys. Empty
    /// means the shipped order.
    pub nav_order: Vec<String>,
    /// The screen the app opens on, as a stable key. It can be in any group.
    /// Empty means the first key of `nav_order`.
    pub default_screen: String,
    /// The palette for light mode, as a stable id. Empty means the shipped one.
    /// The desktop app reads it. The mobile apps ignore it.
    pub theme_light: String,
    /// The palette for dark mode. The same form as `theme_light`.
    pub theme_dark: String,
    /// The mobile tabs, left to right, as stable keys. It lists the tabs that can
    /// move: `More` is not in it and stays last. Empty means the shipped order.
    /// The desktop sidebar has its own `nav_order`.
    // The mobile app has its own tab list: `TABS` in apps/mobile/src/nav/tabs.ts. Keep both in step.
    pub tab_order: Vec<String>,
    /// The mobile tab that the app opens on, as a stable key. Empty means `today`.
    pub start_tab: String,
}

/// The mobile tabs that the person can move. The app lists them too, in `TABS`
/// in apps/mobile/src/nav/tabs.ts. Keep both lists in step.
const MOVABLE_TABS: [&str; 4] = ["today", "activities", "health", "sleep"];

/// One save at a time. A save reads the file, changes one field and writes it
/// back, so two saves at once would lose a change.
static PREFS_LOCK: Mutex<()> = Mutex::new(());

/// Read the saved preferences, change them and save them, as one step.
fn update(change: impl FnOnce(&mut Prefs)) -> Result<Prefs, ApiError> {
    // A panic in another save leaves the file whole, so a poisoned lock is safe to use.
    let _guard = PREFS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut next = prefs();
    change(&mut next);
    save(next)
}

fn prefs_path() -> Result<PathBuf, ApiError> {
    Ok(paths::base_dir()?.join(PREFS_FILE))
}

/// The saved preferences. A missing or broken file gives the defaults.
pub fn prefs() -> Prefs {
    prefs_path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn set_theme(theme: ThemeMode) -> Result<Prefs, ApiError> {
    update(|p| p.theme = theme)
}

/// Save the sidebar order and the default screen. The theme stays as it was.
pub fn set_nav_order(nav_order: Vec<String>, default_screen: String) -> Result<Prefs, ApiError> {
    update(|p| {
        p.nav_order = nav_order;
        p.default_screen = default_screen;
    })
}

/// Save the mobile tab order and the start tab. An empty order and an empty
/// start tab mean the shipped values. The order must list each movable tab once.
pub fn set_tab_order(tab_order: Vec<String>, start_tab: String) -> Result<Prefs, ApiError> {
    let mut sorted: Vec<&str> = tab_order.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    let mut movable = MOVABLE_TABS;
    movable.sort_unstable();
    if !tab_order.is_empty() && sorted != movable {
        return Err(ApiError::BadArguments(
            "tabOrder must list today, activities, health and sleep once each".into(),
        ));
    }
    if !start_tab.is_empty() && start_tab != "more" && !MOVABLE_TABS.contains(&start_tab.as_str()) {
        return Err(ApiError::BadArguments(format!(
            "unknown start tab: {start_tab}"
        )));
    }
    update(|p| {
        p.tab_order = tab_order;
        p.start_tab = start_tab;
    })
}

/// Save the light and the dark palette. The other settings stay as they were.
pub fn set_palettes(theme_light: String, theme_dark: String) -> Result<Prefs, ApiError> {
    update(|p| {
        p.theme_light = theme_light;
        p.theme_dark = theme_dark;
    })
}

fn save(prefs: Prefs) -> Result<Prefs, ApiError> {
    let json =
        serde_json::to_string_pretty(&prefs).map_err(|e| ApiError::Internal(e.to_string()))?;
    paths::ensure_base_dir()?;
    let path = prefs_path()?;
    // Write a temp file and flush it to disk, then rename it. A crash or a
    // power loss never leaves half a file.
    // The name is unique for each process, so no other writer shares it.
    let temp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    write_synced(&temp, &json)
        .and_then(|()| std::fs::rename(&temp, &path))
        .map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            ApiError::Internal(e.to_string())
        })?;
    Ok(prefs)
}

fn write_synced(path: &std::path::Path, data: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::File::create(path)?;
    file.write_all(data.as_bytes())?;
    file.sync_all()
}

/// What the Settings screen draws.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub connected: bool,
    pub account_line: String,
    pub cache_line: String,
    pub data_folder: Option<String>,
    pub importable_token_path: Option<String>,
    pub theme: ThemeMode,
    pub version: String,
    pub privacy_line: String,
}

pub fn settings_view() -> Result<SettingsView, ApiError> {
    let status = session::status()?;
    let (count, last_sync) = cache_facts();
    Ok(SettingsView {
        account_line: account_line(status.connected),
        cache_line: cache_line(count, last_sync.as_deref(), Utc::now()),
        connected: status.connected,
        data_folder: paths::base_dir().ok().map(|p| p.display().to_string()),
        importable_token_path: status.importable_token_path,
        theme: prefs().theme,
        version: APP_VERSION.to_owned(),
        privacy_line: format!(
            "Garmin requests go straight from this app to Garmin, with tokens in {}. \
             They never go through a server of ours.",
            secrets::STORE
        ),
    })
}

/// How long ago the last sync ran, such as "3 hours ago". `None` when the cache
/// has never synced or cannot open.
pub fn synced_ago() -> Option<String> {
    let (_, last_sync) = cache_facts();
    let when = DateTime::parse_from_rfc3339(&last_sync?).ok()?;
    Some(since(when.with_timezone(&Utc), Utc::now()))
}

/// The cached activity count and the last sync time. A cache that cannot open
/// reads as empty, because Settings must still open then.
fn cache_facts() -> (i64, Option<String>) {
    // Opening the cache creates the file. Settings must not do that, because
    // the app reads a missing file as "show the sample data".
    let exists = garmin_core::db::default_path().is_some_and(|p| p.exists());
    let Some(db) = exists.then(crate::open_default_db).and_then(Result::ok) else {
        return (0, None);
    };
    let count = db.activity_count().unwrap_or(0);
    (count, db.sync_state("last_sync").ok().flatten())
}

fn account_line(connected: bool) -> String {
    if connected {
        format!(
            "Connected. Tokens are held in {}, never in the database.",
            secrets::STORE
        )
    } else {
        "Not connected.".into()
    }
}

fn cache_line(count: i64, last_sync: Option<&str>, now: DateTime<Utc>) -> String {
    let synced = last_sync
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|t| since(t.with_timezone(&Utc), now));
    match (count, synced) {
        (0, None) => "Nothing is cached yet. Sync to bring in your data.".into(),
        (n, None) => format!("{} cached, never synced.", activities(n)),
        (0, Some(when)) => format!("No activities cached, last synced {when}."),
        (n, Some(when)) => format!("{} cached, last synced {when}.", activities(n)),
    }
}

fn activities(n: i64) -> String {
    let noun = if n == 1 { "activity" } else { "activities" };
    format!("{} {noun}", crate::text::num(n as f64))
}

/// How long ago, in words. It follows `since()` of the old app.
pub fn since(then: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = (now - then).num_seconds();
    if secs < 45 {
        return "just now".into();
    }
    let plural = |n: i64, unit: &str| format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" });
    let mins = (secs as f64 / 60.0).round() as i64;
    if mins < 60 {
        return plural(mins, "minute");
    }
    let hours = (mins as f64 / 60.0).round() as i64;
    if hours < 24 {
        return plural(hours, "hour");
    }
    // Calendar days in the local zone, so a sync at 23:00 is "yesterday" at 08:00.
    let local = |t: DateTime<Utc>| t.with_timezone(&Local).date_naive();
    let days = (local(now) - local(then)).num_days();
    if days <= 1 {
        "yesterday".into()
    } else if days < 14 {
        format!("{days} days ago")
    } else {
        format!("on {}", date_words(local(then), local(now)))
    }
}

fn date_words(date: NaiveDate, today: NaiveDate) -> String {
    use chrono::Datelike;
    let base = date.format("%b %-d").to_string();
    if date.year() == today.year() {
        base
    } else {
        format!("{base} {}", date.year())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn since_reads_like_the_old_app() {
        let now = Utc::now();
        assert_eq!(since(now - Duration::seconds(10), now), "just now");
        assert_eq!(since(now - Duration::minutes(1), now), "1 minute ago");
        assert_eq!(since(now - Duration::minutes(25), now), "25 minutes ago");
        assert_eq!(since(now - Duration::hours(3), now), "3 hours ago");
        assert_eq!(since(now - Duration::days(5), now), "5 days ago");
    }

    #[test]
    fn the_cache_line_covers_each_state() {
        let now = Utc::now();
        let recent = (now - Duration::hours(2)).to_rfc3339();
        assert_eq!(
            cache_line(0, None, now),
            "Nothing is cached yet. Sync to bring in your data."
        );
        assert_eq!(
            cache_line(1200, None, now),
            "1,200 activities cached, never synced."
        );
        assert_eq!(
            cache_line(3, Some(&recent), now),
            "3 activities cached, last synced 2 hours ago."
        );
        assert_eq!(
            cache_line(0, Some(&recent), now),
            "No activities cached, last synced 2 hours ago."
        );
        assert_eq!(cache_line(1, None, now), "1 activity cached, never synced.");
        assert_eq!(
            cache_line(1, Some(&recent), now),
            "1 activity cached, last synced 2 hours ago."
        );
    }

    #[test]
    fn prefs_round_trip_and_survive_a_broken_file() {
        let dir = std::env::temp_dir().join(format!("garmin-api-prefs-{}", std::process::id()));
        crate::init(&dir);
        // `init` keeps the first folder of the process, so write there.
        let base = paths::base_dir().unwrap();
        let _ = std::fs::remove_file(base.join(PREFS_FILE));
        assert_eq!(prefs().theme, ThemeMode::System);
        assert_eq!(set_theme(ThemeMode::Dark).unwrap().theme, ThemeMode::Dark);
        assert_eq!(prefs().theme, ThemeMode::Dark);
        // Each setting keeps the other one.
        set_nav_order(vec!["sleep".into()], "gear".into()).unwrap();
        assert_eq!(prefs().default_screen, "gear");
        assert_eq!(prefs().theme, ThemeMode::Dark);
        set_theme(ThemeMode::Light).unwrap();
        assert_eq!(prefs().nav_order, vec!["sleep".to_string()]);
        // An old file has no palette keys. The defaults fill them.
        std::fs::write(base.join(PREFS_FILE), r#"{"theme":"dark"}"#).unwrap();
        assert_eq!(prefs().theme_dark, "");
        set_palettes("linen".into(), "custom:x".into()).unwrap();
        assert_eq!(prefs().theme, ThemeMode::Dark);
        assert_eq!(prefs().theme_dark, "custom:x");
        std::fs::write(base.join(PREFS_FILE), "{").unwrap();
        assert_eq!(prefs().theme, ThemeMode::System);
        the_tab_order_is_checked_and_kept_apart_from_the_sidebar();
        saves_in_parallel_all_succeed_and_keep_the_other_fields();
        parallel_saves_lose_no_change();
        let _ = std::fs::remove_file(base.join(PREFS_FILE));
    }

    /// It runs inside the round-trip test: both write the one prefs file of the process.
    fn the_tab_order_is_checked_and_kept_apart_from_the_sidebar() {
        let keys = |k: &[&str]| k.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let order = keys(&["sleep", "today", "health", "activities"]);
        let saved = set_tab_order(order.clone(), "more".into()).unwrap();
        assert_eq!(
            (saved.tab_order, saved.start_tab),
            (order.clone(), "more".into())
        );
        // The sidebar order and the theme stay as they were, and so do the tabs.
        set_nav_order(keys(&["gear"]), "gear".into()).unwrap();
        set_theme(ThemeMode::Dark).unwrap();
        assert_eq!(prefs().tab_order, order);
        assert_eq!(prefs().nav_order, keys(&["gear"]));
        // A bad value is refused, and the saved value stays.
        for (bad, start) in [
            (keys(&["today"]), ""),
            (keys(&["today", "today", "health", "sleep"]), ""),
            (keys(&["today", "activities", "health", "more"]), ""),
            (order.clone(), "gear"),
        ] {
            assert!(matches!(
                set_tab_order(bad, start.into()),
                Err(ApiError::BadArguments(_))
            ));
        }
        assert_eq!(prefs().tab_order, order);
        // An empty order and start tab reset to the shipped values.
        let reset = set_tab_order(Vec::new(), String::new()).unwrap();
        assert!(reset.tab_order.is_empty() && reset.start_tab.is_empty());
    }

    /// It runs inside the round-trip test, in the same folder.
    fn saves_in_parallel_all_succeed_and_keep_the_other_fields() {
        set_theme(ThemeMode::Dark).unwrap();
        let order = |i: usize| {
            let mut tabs: Vec<String> = MOVABLE_TABS.iter().map(|t| t.to_string()).collect();
            tabs.rotate_left(i % 4);
            tabs
        };
        std::thread::scope(|scope| {
            for t in 0..4 {
                scope.spawn(move || {
                    for i in 0..50 {
                        set_tab_order(order(t + i), "today".into()).unwrap();
                        set_palettes(format!("light{t}"), format!("dark{t}")).unwrap();
                    }
                });
            }
        });
        // The file is whole, and a save never dropped the theme.
        let saved = prefs();
        assert_eq!(saved.theme, ThemeMode::Dark);
        assert_eq!(saved.tab_order.len(), 4);
        assert_eq!(saved.start_tab, "today");
        assert!(saved.theme_light.starts_with("light"));
    }

    /// It runs inside the round-trip test. Each thread is the only writer of its field.
    fn parallel_saves_lose_no_change() {
        let order = |i: usize| {
            let mut tabs: Vec<String> = MOVABLE_TABS.iter().map(|t| t.to_string()).collect();
            tabs.rotate_left(i % 4);
            tabs
        };
        std::thread::scope(|scope| {
            scope.spawn(move || {
                for i in 0..100 {
                    set_tab_order(order(i), "today".into()).unwrap();
                    assert_eq!(prefs().tab_order, order(i));
                }
            });
            scope.spawn(|| {
                for i in 0..100 {
                    set_palettes(format!("light{i}"), format!("dark{i}")).unwrap();
                    assert_eq!(prefs().theme_light, format!("light{i}"));
                }
            });
        });
    }
}
