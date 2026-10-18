//! Write the Today view model as JSON.
//!
//! Usage:
//!
//! - `garmin-api-fixture <database copy> [output file]` reads a real cache.
//!   The output goes to `fixtures/today.json` by default. That file holds real
//!   data. The repository ignores it. Never commit it.
//! - `garmin-api-fixture --sample [output file]` seeds a temporary database
//!   with made-up rows and writes `fixtures/today.sample.json` by default.
//!   The sample is safe to commit. Its date is fixed, so the output is stable.

use chrono::{Duration, NaiveDate};
use garmin_api::{open_db, today_view};
use garmin_core::{db::Db, DailyMetrics};
use serde_json::json;
use std::path::{Path, PathBuf};

/// The day the sample describes. A fixed date keeps the file diff-free.
const SAMPLE_DAY: (i32, u32, u32) = (2026, 3, 11);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let Some(first) = args.next() else {
        eprintln!("usage: garmin-api-fixture (<database copy> | --sample) [output file]");
        std::process::exit(2);
    };
    let (db, today, default_out, scratch) = if first == "--sample" {
        let day = NaiveDate::from_ymd_opt(SAMPLE_DAY.0, SAMPLE_DAY.1, SAMPLE_DAY.2).unwrap();
        let dir = std::env::temp_dir().join(format!("garmin-api-sample-{}", std::process::id()));
        let db = open_db(&dir.join("cache.sqlite3"))?;
        seed(&db, day)?;
        (db, day, "fixtures/today.sample.json", Some(dir))
    } else {
        let db = open_db(Path::new(&first))?;
        (db, garmin_core::today(), "fixtures/today.json", None)
    };

    let out = args
        .next()
        .map_or_else(|| PathBuf::from(default_out), PathBuf::from);
    let view = today_view(&db, today)?;
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&out, serde_json::to_string_pretty(&view)? + "\n")?;
    if let Some(dir) = scratch {
        let _ = std::fs::remove_dir_all(dir);
    }
    eprintln!("wrote {}", out.display());
    Ok(())
}

/// Made-up rows: 40 days of recovery data, a food log on four days, and a run
/// every second day. Resting HR rises in the last three mornings, so the
/// attention list has an entry.
fn seed(db: &Db, today: NaiveDate) -> Result<(), Box<dyn std::error::Error>> {
    for age in 0..40i64 {
        let date = today - Duration::days(age);
        let wobble = (age * 7 % 5) as f64;
        let mut d = DailyMetrics {
            date: date.format("%Y-%m-%d").to_string(),
            resting_hr: Some(if age < 3 { 56.0 } else { 50.0 + wobble / 2.0 }),
            hrv_last_night: Some(if age < 5 { 41.0 } else { 52.0 + wobble }),
            training_readiness: Some(60.0 + wobble * 4.0),
            sleep_secs: Some(25_200.0 + wobble * 900.0),
            sleep_score: Some(70.0 + wobble * 3.0),
            steps: Some(7_000 + (age * 311 % 4_000)),
            stress_avg: Some(28.0 + wobble),
            body_battery_high: Some(80.0 - wobble * 3.0),
            body_battery_low: Some(20.0 + wobble),
            total_burn_kcal: Some(2_600.0 + wobble * 40.0),
            active_kcal: Some(500.0),
            bmr_kcal: Some(2_100.0 + wobble * 40.0),
            ..DailyMetrics::default()
        };
        if age % 3 == 1 && age < 12 {
            d.consumed_kcal = Some(1_700.0 + wobble * 50.0);
        }
        db.upsert_daily(&d)?;
    }
    for n in 0..10i64 {
        let date = today - Duration::days(n * 2);
        let hard = 300.0 + (n * 130 % 700) as f64;
        let activity = json!({
            "activityId": 9_000_000 + n,
            "activityName": "Morning Run",
            "startTimeLocal": format!("{} 07:15:00", date.format("%Y-%m-%d")),
            "distance": 6_000.0 + (n * 450 % 3_000) as f64,
            "duration": 2_100.0 + (n * 90 % 600) as f64,
            "movingDuration": 2_050.0,
            "averageHR": 148.0,
            "maxHR": 171.0,
            "averageRunningCadenceInStepsPerMinute": 158.0 + (n % 4) as f64,
            "aerobicTrainingEffect": 3.1,
            "activityType": { "typeKey": "running" },
            "hrTimeInZone_1": 200.0,
            "hrTimeInZone_2": 1_100.0 - hard / 2.0,
            "hrTimeInZone_3": hard,
            "hrTimeInZone_4": hard / 3.0,
            "hrTimeInZone_5": 0.0,
        });
        db.upsert_activity(&serde_json::from_value(activity)?)?;
    }
    Ok(())
}
