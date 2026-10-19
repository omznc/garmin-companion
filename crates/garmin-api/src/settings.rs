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

const PREFS_FILE: &str = "prefs.json";

/// The app version. The release build sets `APP_VERSION` for the compiler, so
/// the number is the release version. A local build shows the crate version.
pub const APP_VERSION: &str = match option_env!("APP_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

/// How the app picks light or dark.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub theme: ThemeMode,
    /// The sidebar entries in the order the user chose, as stable keys. The
    /// first key is the screen the app opens on. Empty means the shipped order.
    pub nav_order: Vec<String>,
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
    save(Prefs { theme, ..prefs() })
}

/// Save the sidebar order. The theme stays as it was.
pub fn set_nav_order(nav_order: Vec<String>) -> Result<Prefs, ApiError> {
    save(Prefs {
        nav_order,
        ..prefs()
    })
}

fn save(prefs: Prefs) -> Result<Prefs, ApiError> {
    let json =
        serde_json::to_string_pretty(&prefs).map_err(|e| ApiError::Internal(e.to_string()))?;
    paths::ensure_base_dir()?;
    std::fs::write(prefs_path()?, json).map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(prefs)
}

/// What the Settings screen draws.
#[derive(Debug, Clone, PartialEq, Serialize)]
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
        set_nav_order(vec!["sleep".into()]).unwrap();
        assert_eq!(prefs().theme, ThemeMode::Dark);
        set_theme(ThemeMode::Light).unwrap();
        assert_eq!(prefs().nav_order, vec!["sleep".to_string()]);
        std::fs::write(base.join(PREFS_FILE), "{").unwrap();
        assert_eq!(prefs().theme, ThemeMode::System);
        let _ = std::fs::remove_file(base.join(PREFS_FILE));
    }
}
