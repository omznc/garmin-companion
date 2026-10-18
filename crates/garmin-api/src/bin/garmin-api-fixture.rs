//! Write the Today view model as JSON.
//!
//! Usage:
//!
//! - `garmin-api-fixture <database copy> [output file]` reads a real cache.
//!   The output goes to `crates/garmin-api/fixtures/today.json` by default. That file holds real
//!   data. The repository ignores it. Never commit it. `FIXTURE_DATE=YYYY-MM-DD` picks the day.
//! - `garmin-api-fixture --sample [output file]` seeds a temporary database
//!   with made-up rows and writes `crates/garmin-api/fixtures/today.sample.json` by default.
//!   The sample is safe to commit. Its date is fixed, so the output is stable.
//! - `garmin-api-fixture --body` writes `body.sample.json` and `body.empty.json` (Weight, Food, Strength).
//! - `garmin-api-fixture --personas` writes all the made-up persona files
//!   (`today.sample.json`, `today.golfer.json`, `today.gym.json`,
//!   `today.health_only.json`, `today.new_user.json`). All are safe to commit.

use chrono::NaiveDate;
use garmin_api::personas::{activity_fixtures, sample_day, seed, Persona};
use garmin_api::sleep_view::seed_sleep;
use garmin_api::{health_view, open_db, sleep_view, today_view};
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let Some(first) = args.next() else {
        eprintln!(
            "usage: garmin-api-fixture (<database copy> | --sample | --personas) [output file]"
        );
        std::process::exit(2);
    };
    if first == "--personas" {
        for persona in Persona::ALL {
            write_persona(persona, &fixtures_dir().join(persona.file_name()))?;
        }
        return Ok(());
    }
    if first == "--body" {
        return write_body();
    }
    if first == "--sample" {
        let out = args.next().map_or_else(
            || fixtures_dir().join(Persona::Sample.file_name()),
            PathBuf::from,
        );
        return write_persona(Persona::Sample, &out);
    }
    let db = open_db(Path::new(&first))?;
    let out = args
        .next()
        .map_or_else(|| fixtures_dir().join("today.json"), PathBuf::from);
    // `FIXTURE_DATE=YYYY-MM-DD` builds the view for a past day of a cache copy.
    let day = std::env::var("FIXTURE_DATE")
        .ok()
        .and_then(|d| d.parse::<NaiveDate>().ok())
        .unwrap_or_else(garmin_core::today);
    write_view(&today_view(&db, day)?, &out)
}

/// Write `body.sample.json` and `body.empty.json`, the Weight, Food and Strength views.
fn write_body() -> Result<(), Box<dyn std::error::Error>> {
    let today = garmin_core::today();
    for (name, filled) in [("sample", true), ("empty", false)] {
        let dir =
            std::env::temp_dir().join(format!("garmin-api-body-{name}-{}", std::process::id()));
        let db = open_db(&dir.join("cache.sqlite3"))?;
        if filled {
            garmin_api::body::seed_body(&db, today)?;
        }
        let views = garmin_api::body::body_views(&db, today)?;
        let out = fixtures_dir().join(format!("body.{name}.json"));
        std::fs::write(&out, serde_json::to_string_pretty(&views)? + "\n")?;
        eprintln!("wrote {}", out.display());
        let _ = std::fs::remove_dir_all(dir);
    }
    Ok(())
}

fn write_persona(persona: Persona, out: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let day: NaiveDate = sample_day();
    let dir = std::env::temp_dir().join(format!(
        "garmin-api-{}-{}",
        persona.name(),
        std::process::id()
    ));
    let db = open_db(&dir.join("cache.sqlite3"))?;
    seed(&db, persona, day)?;
    seed_sleep(&db, persona, day)?;
    // The Health and Sleep views sit next to the Today file.
    let beside = |kind: &str| out.with_file_name(format!("{kind}.{}.json", persona.name()));
    let result = today_view(&db, day)
        .map_err(Into::into)
        .and_then(|v| write_view(&v, out))
        .and_then(|()| write_view(&health_view(&db, day)?, &beside("health")))
        .and_then(|()| write_view(&sleep_view(&db, day)?, &beside("sleep")))
        .and_then(|()| write_activities(&db, persona, out.parent()));
    let _ = std::fs::remove_dir_all(dir);
    result
}

/// The Activities list and detail files go next to the Today file.
fn write_activities(
    db: &garmin_core::Db,
    persona: Persona,
    dir: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = dir.map_or_else(fixtures_dir, Path::to_path_buf);
    let (list, detail) = activity_fixtures(db)?;
    write_view(&list, &dir.join(persona.activities_file_name()))?;
    let path = dir.join(persona.detail_file_name());
    match detail {
        Some(d) => write_view(&d, &path),
        None => {
            let _ = std::fs::remove_file(&path);
            Ok(())
        }
    }
}

fn write_view(value: &impl serde::Serialize, out: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(out, serde_json::to_string_pretty(value)? + "\n")?;
    eprintln!("wrote {}", out.display());
    Ok(())
}

/// Anchored to the crate, so a run from any folder writes to the ignored path.
fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}
