//! The optional sync steps: data that the screens can show without.
//!
//! Every step here follows one rule. A reply of "no data" (a 404, a 403, an
//! empty body) is not a problem. Any other failure is a warning. Only a 429, a
//! dropped network or a dead token stops the sync, as for the older steps, and
//! see [`crate::sync::optional`]. These steps run after the steps that decide
//! whether a history is complete, and they never count toward the outage
//! streak. They also never walk the history: a per-day step covers the last
//! few days only.

use anyhow::Result;
use chrono::{Duration, Utc};

use crate::client::GarminClient;
use crate::db::Db;
use crate::sync::{optional, Progress, SyncProgress, SyncReport};

/// The days that a per-day curve is fetched for: today and yesterday.
const CURVE_DAYS: i64 = 2;

fn announce(on: Progress<'_>, detail: &str) {
    on(SyncProgress {
        phase: "extras",
        detail: detail.to_owned(),
        done: 0,
        total: None,
    });
}

/// The day strings of the last `n` days, today first.
fn recent_days(n: i64) -> Vec<String> {
    let today = Utc::now().date_naive();
    (0..n)
        .map(|o| (today - Duration::days(o)).format("%Y-%m-%d").to_string())
        .collect()
}

/// Run every optional step. The first fatal failure stops the rest.
pub(crate) async fn sync_extras(
    client: &GarminClient,
    db: &Db,
    report: &mut SyncReport,
    on: Progress<'_>,
) -> Result<()> {
    sync_body_battery(client, db, report, on).await?;
    sync_courses(client, db, report, on).await?;
    if due(db, "extras_static", STATIC_HOURS) {
        sync_devices(client, db, report, on).await?;
        db.set_sync_state("extras_static", &Utc::now().to_rfc3339())?;
    }
    Ok(())
}

/// How long the slow-changing data (devices, plans, VO2 max) is left alone
/// between two syncs. A sync on a timer would otherwise ask for it every time.
const STATIC_HOURS: i64 = 6;

/// Whether the step stored under `key` is due. A missing or unreadable time
/// means due.
fn due(db: &Db, key: &str, hours: i64) -> bool {
    let at = db
        .sync_state(key)
        .ok()
        .flatten()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok());
    at.is_none_or(|t| Utc::now().signed_duration_since(t) >= Duration::hours(hours))
}

/// The devices, with the upload time of the last used one.
async fn sync_devices(
    client: &GarminClient,
    db: &Db,
    report: &mut SyncReport,
    on: Progress<'_>,
) -> Result<()> {
    announce(on, "devices");
    let Some(list) = optional(report, "devices", client.devices().await)? else {
        return Ok(());
    };
    let mut devices = crate::devices::parse_devices(&list);
    if let Some(last) = optional(report, "last used device", client.last_used_device().await)? {
        crate::devices::apply_last_used(&mut devices, &last);
    }
    if let Err(e) = db.replace_devices(&devices) {
        report.warnings.push(format!("devices: {e}"));
    }
    Ok(())
}

/// How many course tracks one sync fetches. A new account with many courses
/// gets its thumbnails over a few syncs.
const COURSE_TRACKS_PER_SYNC: usize = 10;

/// The course list, then the track of each new or changed course.
async fn sync_courses(
    client: &GarminClient,
    db: &Db,
    report: &mut SyncReport,
    on: Progress<'_>,
) -> Result<()> {
    announce(on, "courses");
    let Some(list) = optional(report, "courses", client.courses().await)? else {
        return Ok(());
    };
    let courses = crate::courses::parse_course_list(&list);
    let mut ids = Vec::new();
    for c in &courses {
        ids.push(c.course_id);
        if let Err(e) = db.upsert_course(c) {
            report.warnings.push(format!("course {}: {e}", c.course_id));
        }
    }
    // A reply that is a list is the whole list, so a course that is gone from
    // it is gone from Garmin. Any other reply says nothing about it.
    // A list of items that all failed to parse is a changed shape, not an
    // empty account, so it removes nothing.
    let whole = list
        .as_array()
        .is_some_and(|l| l.is_empty() || !ids.is_empty());
    if whole {
        if let Err(e) = db.retain_courses(&ids) {
            report.warnings.push(format!("courses: {e}"));
        }
    }
    for id in db.courses_missing_track(COURSE_TRACKS_PER_SYNC)? {
        let what = format!("course {id}");
        let Some(detail) = optional(report, &what, client.course(id).await)? else {
            continue;
        };
        let track = crate::courses::parse_course_track(&detail);
        if let Err(e) = db.save_course_track(id, &track) {
            report.warnings.push(format!("{what}: {e}"));
        }
    }
    Ok(())
}

/// The Body Battery curve of today and yesterday.
async fn sync_body_battery(
    client: &GarminClient,
    db: &Db,
    report: &mut SyncReport,
    on: Progress<'_>,
) -> Result<()> {
    announce(on, "body battery");
    for date in recent_days(CURVE_DAYS) {
        let what = format!("body battery {date}");
        let Some(v) = optional(report, &what, client.daily_stress(&date).await)? else {
            continue;
        };
        let points = crate::intraday::parse_body_battery(&v);
        if let Err(e) = db.save_body_battery_curve(&date, &points) {
            report.warnings.push(format!("{what}: {e}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(code: u16) -> anyhow::Error {
        anyhow::Error::new(crate::client::HttpStatus {
            path: "/x".into(),
            status: code,
            body: String::new(),
        })
    }

    #[test]
    fn no_data_is_silent_and_a_fatal_failure_stops() {
        let mut report = SyncReport::default();
        for code in [404, 403, 204] {
            assert!(optional(&mut report, "x", Err(status(code)))
                .unwrap()
                .is_none());
        }
        assert!(optional(&mut report, "x", Ok(serde_json::Value::Null))
            .unwrap()
            .is_none());
        assert!(report.warnings.is_empty());

        // A server error is a warning, never an abort.
        assert!(optional(&mut report, "x", Err(status(503)))
            .unwrap()
            .is_none());
        assert_eq!(report.warnings.len(), 1);

        assert!(optional(&mut report, "x", Err(status(429))).is_err());
        let dead = anyhow::Error::new(crate::auth::AuthFailed("refresh failed".into()));
        assert!(optional(&mut report, "x", Err(dead)).is_err());
        let transport = reqwest::Client::new().get("not a url").build().unwrap_err();
        assert!(optional(&mut report, "x", Err(anyhow::Error::new(transport))).is_err());

        let body = serde_json::json!({ "a": 1 });
        assert_eq!(
            optional(&mut report, "x", Ok(body.clone())).unwrap(),
            Some(body)
        );
    }

    #[test]
    fn the_recent_days_start_with_today() {
        let days = recent_days(2);
        assert_eq!(days.len(), 2);
        assert_eq!(
            days[0],
            Utc::now().date_naive().format("%Y-%m-%d").to_string()
        );
    }
}
