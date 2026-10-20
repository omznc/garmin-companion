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
