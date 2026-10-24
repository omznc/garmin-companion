//! The optional sync steps: data that the screens can show without.
//!
//! Every step here follows one rule. A reply of "no data" (a 404, a 403, an
//! empty body) is not a problem. Any other failure is a warning. A 429, a
//! dropped network or a dead token stops only these steps, and the sync still
//! ends (see [`crate::sync::optional`]). These steps run after the steps that decide
//! whether a history is complete, and they never count toward the outage
//! streak. They also never walk the history: a per-day step covers the last
//! few days only.

use anyhow::Result;
use chrono::{Duration, Utc};
use serde_json::Value;

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
    let today = crate::today();
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
    sync_calendar(client, db, report, on).await?;
    sync_planned_workouts(client, db, report, on).await?;
    sync_nutrition(client, db, report, on).await?;
    let last = sync_watch_upload(client, db, report).await?;
    if due(db, "extras_static", STATIC_HOURS) {
        sync_devices(client, db, report, on, last.as_ref()).await?;
        sync_plans(client, db, report, on).await?;
        sync_vo2(client, db, report, on).await?;
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

/// When Garmin last received the watch data. The watch data reaches Garmin
/// through the watch's own sync, usually the Garmin Connect app. This is one
/// small request, so it runs on every sync. The 6 hour gate would leave the
/// age stale. It returns the reply for the devices step.
async fn sync_watch_upload(
    client: &GarminClient,
    db: &Db,
    report: &mut SyncReport,
) -> Result<Option<serde_json::Value>> {
    let last = optional(report, "last used device", client.last_used_device().await)?;
    if let Some(ms) = last.as_ref().and_then(crate::devices::upload_ms) {
        db.set_sync_state(crate::devices::WATCH_UPLOAD_KEY, &ms.to_string())?;
        db.set_sync_state(crate::devices::WATCH_CHECKED_KEY, &Utc::now().to_rfc3339())?;
    }
    if let Some(last) = &last {
        // The devices step runs only every few hours. Keep the stored rows current.
        let stored = db.devices().and_then(|mut d| {
            crate::devices::apply_last_used(&mut d, last);
            db.replace_devices(&d)
        });
        if let Err(e) = stored {
            report.warnings.push(format!("devices: {e}"));
        }
    }
    Ok(last)
}

/// The devices, with the upload time of the last used one.
async fn sync_devices(
    client: &GarminClient,
    db: &Db,
    report: &mut SyncReport,
    on: Progress<'_>,
    last: Option<&serde_json::Value>,
) -> Result<()> {
    announce(on, "devices");
    let Some(list) = optional(report, "devices", client.devices().await)? else {
        return Ok(());
    };
    let mut devices = crate::devices::parse_devices(&list);
    if let Some(last) = last {
        crate::devices::apply_last_used(&mut devices, last);
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
        // A reply with no point list says nothing about the track, so it
        // must not mark the course as done.
        if detail.get("geoPoints").and_then(|p| p.as_array()).is_none() {
            continue;
        }
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

/// The first read of the VO2 max history covers a year. Later reads cover
/// the last month, which is enough for a late upload from the watch.
const VO2_FIRST_DAYS: i64 = 365;
const VO2_LATER_DAYS: i64 = 30;

/// VO2 max history and the fitness age.
async fn sync_vo2(
    client: &GarminClient,
    db: &Db,
    report: &mut SyncReport,
    on: Progress<'_>,
) -> Result<()> {
    announce(on, "vo2 max");
    let today = crate::today();
    let day = |d: chrono::NaiveDate| d.format("%Y-%m-%d").to_string();
    let first = db.sync_state("vo2_history")?.is_none();
    let span = if first {
        VO2_FIRST_DAYS
    } else {
        VO2_LATER_DAYS
    };
    let (start, end) = (day(today - Duration::days(span)), day(today));
    // A "no data" reply still counts as the first read: an empty list is the
    // normal answer for an account that never ran outdoors.
    let history = optional(
        report,
        "vo2 max",
        client.max_metrics_range(&start, &end).await,
    )?;
    // The history is a list. An error object or any other shape stores
    // nothing and leaves the first-read marker unset.
    if let Some(v) = history.filter(Value::is_array) {
        if let Err(e) = db.save_vo2_days(&crate::vo2::parse_vo2_days(&v)) {
            report.warnings.push(format!("vo2 max: {e}"));
        } else if first {
            db.set_sync_state("vo2_history", &end)?;
        }
    }
    let date = day(today);
    if let Some(v) = optional(report, "fitness age", client.fitness_age(&date).await)? {
        if let Some(age) = crate::vo2::parse_fitness_age(&date, &v) {
            if let Err(e) = db.save_fitness_age(&age) {
                report.warnings.push(format!("fitness age: {e}"));
            }
        }
    }
    Ok(())
}

/// How many days of the food log one sync reads.
const NUTRITION_DAYS: i64 = 3;

/// The food log of the last three days.
async fn sync_nutrition(
    client: &GarminClient,
    db: &Db,
    report: &mut SyncReport,
    on: Progress<'_>,
) -> Result<()> {
    announce(on, "nutrition");
    for date in recent_days(NUTRITION_DAYS) {
        let what = format!("food log {date}");
        let Some(v) = optional(report, &what, client.food_log(&date).await)? else {
            continue;
        };
        // A reply that is not a food log (no date) says nothing about the day.
        if v.get("mealDate").is_none() {
            continue;
        }
        let goals = crate::nutrition::parse_goals(&v);
        let meals = crate::nutrition::parse_meals(&v);
        if let Err(e) = db.save_nutrition_day(&date, goals.as_ref(), &meals) {
            report.warnings.push(format!("{what}: {e}"));
        }
    }
    Ok(())
}

/// The month that is `ahead` months after the month of `today`: the year, the
/// zero-based month that the calendar endpoint wants, and the first and last
/// day as `YYYY-MM-DD`.
fn month_window(today: chrono::NaiveDate, ahead: u32) -> Option<(i32, u32, String, String)> {
    use chrono::Datelike;
    let index = today.year() * 12 + today.month0() as i32 + ahead as i32;
    let (year, month0) = (index.div_euclid(12), index.rem_euclid(12) as u32);
    let first = chrono::NaiveDate::from_ymd_opt(year, month0 + 1, 1)?;
    let last =
        chrono::NaiveDate::from_ymd_opt(year + i32::from(month0 == 11), (month0 + 1) % 12 + 1, 1)?
            - Duration::days(1);
    let fmt = |d: chrono::NaiveDate| d.format("%Y-%m-%d").to_string();
    Some((year, month0, fmt(first), fmt(last)))
}

/// The planned workouts and races of this month and the next.
async fn sync_calendar(
    client: &GarminClient,
    db: &Db,
    report: &mut SyncReport,
    on: Progress<'_>,
) -> Result<()> {
    announce(on, "calendar");
    let today = crate::today();
    for ahead in 0..2 {
        let Some((year, month0, first, last)) = month_window(today, ahead) else {
            continue;
        };
        let what = format!("calendar {first}");
        let Some(v) = optional(report, &what, client.calendar(year, month0).await)? else {
            continue;
        };
        // A reply with no item list says nothing about the month, so it must
        // not clear the plan that the cache already holds.
        if v.get("calendarItems").and_then(|i| i.as_array()).is_none() {
            continue;
        }
        let items = crate::calendar::parse_plan_items(&v);
        if let Err(e) = db.replace_plan_items(&first, &last, &items) {
            report.warnings.push(format!("{what}: {e}"));
        }
    }
    Ok(())
}

/// How many days ahead the steps of a planned workout are fetched for.
const PLAN_AHEAD_DAYS: i64 = 14;

/// How old the stored steps of a workout may be before they are read again.
const STEPS_MAX_AGE_SECS: i64 = 24 * 3600;

/// The steps of each workout that is planned in the next two weeks.
async fn sync_planned_workouts(
    client: &GarminClient,
    db: &Db,
    report: &mut SyncReport,
    on: Progress<'_>,
) -> Result<()> {
    announce(on, "planned workouts");
    let today = crate::today();
    let day = |o: i64| (today + Duration::days(o)).format("%Y-%m-%d").to_string();
    let ids: Vec<i64> = db
        .plan_items(&day(0), &day(PLAN_AHEAD_DAYS))?
        .iter()
        .filter_map(|i| i.workout_id)
        .collect();
    let now = Utc::now().timestamp();
    for id in db.workouts_to_fetch(&ids, now, STEPS_MAX_AGE_SECS)? {
        let what = format!("workout {id}");
        let Some(v) = optional(report, &what, client.workout(id).await)? else {
            continue;
        };
        // As for the calendar: a reply with no segment list stores nothing.
        if v.get("workoutSegments")
            .and_then(|s| s.as_array())
            .is_none()
        {
            continue;
        }
        let steps = crate::calendar::parse_workout_steps(id, &v);
        if let Err(e) = db.save_workout_steps(&steps, now) {
            report.warnings.push(format!("{what}: {e}"));
        }
    }
    Ok(())
}

/// The training plans and the races of the next year.
async fn sync_plans(
    client: &GarminClient,
    db: &Db,
    report: &mut SyncReport,
    on: Progress<'_>,
) -> Result<()> {
    announce(on, "plans");
    if let Some(v) = optional(report, "training plans", client.training_plans().await)? {
        // As for the calendar: a reply with no plan list changes nothing.
        if v.get("trainingPlanList")
            .and_then(|l| l.as_array())
            .is_some()
        {
            let plans = crate::calendar::parse_training_plans(&v);
            if let Err(e) = db.replace_training_plans(&plans) {
                report.warnings.push(format!("training plans: {e}"));
            }
        }
    }
    let today = crate::today();
    let (from, to) = (
        today.format("%Y-%m-%d").to_string(),
        (today + Duration::days(365)).format("%Y-%m-%d").to_string(),
    );
    if let Some(v) = optional(report, "events", client.calendar_events(&from, &to).await)? {
        if v.is_array() || v.get("events").is_some() {
            let events = crate::calendar::parse_race_events(&v);
            if let Err(e) = db.replace_race_events(&from, &to, &events) {
                report.warnings.push(format!("events: {e}"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_recent_days_start_at_the_local_today() {
        let day = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let days = crate::with_as_of(day, || recent_days(2));
        assert_eq!(days, ["2026-01-01", "2025-12-31"]);
    }

    #[test]
    fn the_month_window_wraps_the_year() {
        let d = |y, m, day| chrono::NaiveDate::from_ymd_opt(y, m, day).unwrap();
        assert_eq!(
            month_window(d(2026, 3, 15), 0),
            Some((2026, 2, "2026-03-01".into(), "2026-03-31".into()))
        );
        assert_eq!(
            month_window(d(2026, 12, 31), 1),
            Some((2027, 0, "2027-01-01".into(), "2027-01-31".into()))
        );
        assert_eq!(
            month_window(d(2028, 1, 5), 1).map(|w| w.3),
            Some("2028-02-29".into())
        );
        assert_eq!(
            month_window(d(2026, 11, 2), 1).map(|w| (w.0, w.1, w.3)),
            Some((2026, 11, "2026-12-31".into()))
        );
    }

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
        assert_eq!(days[0], crate::today().format("%Y-%m-%d").to_string());
    }
}
