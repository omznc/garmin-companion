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
//! - `garmin-api-fixture --training` writes `training.<persona>.json` for each persona (Insights, Fitness, Reports).
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
    if first == "--training" {
        return write_training();
    }
    if first == "--views" {
        return write_views();
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

/// Write `views.sample.json`: the tiles, extras and charts views of the sample day.
fn write_views() -> Result<(), Box<dyn std::error::Error>> {
    use garmin_core::calendar::{PlanItem, RaceEvent};
    use garmin_core::intraday::{BatteryPoint, HrvReading};
    use garmin_core::nutrition::{Meal, NutritionGoals};
    let day = sample_day();
    let key = day.format("%Y-%m-%d").to_string();
    let at = |n: i64| {
        (day + chrono::Duration::days(n))
            .format("%Y-%m-%d")
            .to_string()
    };
    let dir = std::env::temp_dir().join(format!("garmin-api-views-{}", std::process::id()));
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let db = open_db(&dir.join("cache.sqlite3"))?;
        seed(&db, Persona::Sample, day)?;
        seed_sleep(&db, Persona::Sample, day)?;
        // The sleep debt needs a sleep need, which the persona seed leaves out.
        for mut night in db.sleep_nights_since(&at(-40))? {
            night.need_secs = Some(28_800.0);
            db.upsert_sleep_night(&night)?;
        }
        garmin_api::body::seed_body(&db, day)?;
        let item = |n: i64, title: &str, sport: &str, race: bool| PlanItem {
            date: at(n),
            kind: if race { "event" } else { "workout" }.into(),
            title: Some(title.into()),
            sport: Some(sport.into()),
            is_race: race,
            item_id: Some(100 + n),
            ..Default::default()
        };
        db.replace_plan_items(
            &key,
            &at(14),
            &[
                item(1, "Easy run", "running", false),
                item(3, "Tempo intervals", "running", false),
                item(6, "Long ride", "cycling", false),
            ],
        )?;
        db.replace_race_events(
            &key,
            &at(365),
            &[RaceEvent {
                event_key: "sample-race".into(),
                date: at(63),
                name: Some("City Half Marathon".into()),
                target_secs: Some(6_300.0),
                location: Some("Sample City".into()),
            }],
        )?;
        let meal = |name: &str, time: &str, kcal: f64, p: f64, c: f64, f: f64| Meal {
            name: Some(name.into()),
            time: Some(time.into()),
            kcal: Some(kcal),
            protein_g: Some(p),
            carbs_g: Some(c),
            fat_g: Some(f),
        };
        db.save_nutrition_day(
            &key,
            Some(&NutritionGoals {
                calories: Some(2400.0),
                adjusted_calories: Some(2650.0),
                protein_g: Some(150.0),
                carbs_g: Some(300.0),
                fat_g: Some(80.0),
            }),
            &[
                meal("Oats and berries", "07:40", 520.0, 22.0, 78.0, 12.0),
                meal("Chicken rice bowl", "12:30", 780.0, 48.0, 90.0, 22.0),
            ],
        )?;
        let readings: Vec<HrvReading> = (0..8)
            .map(|i| HrvReading {
                time: format!("{:02}:{:02}", 1 + i / 2, (i % 2) * 30),
                value: 52.0 + f64::from(i % 3) * 4.0,
            })
            .collect();
        db.save_hrv_readings(&key, &readings)?;
        let battery: Vec<BatteryPoint> = (0..=16)
            .map(|i| BatteryPoint {
                minute: i * 60,
                level: f64::from(if i < 7 { 40 + i * 8 } else { 96 - (i - 7) * 7 }),
            })
            .collect();
        db.save_body_battery_curve(&key, &battery)?;
        let views = serde_json::json!({
            "tiles": garmin_api::tiles::tiles_view(&db, day)?,
            "extras": garmin_api::extras::extras_view(&db, day)?,
            "charts": garmin_api::charts::charts_view(&db, day)?,
        });
        write_view(&views, &fixtures_dir().join("views.sample.json"))
    })();
    let _ = std::fs::remove_dir_all(dir);
    result
}

/// Write `training.<persona>.json` for each persona: Insights, Fitness and every Reports week.
fn write_training() -> Result<(), Box<dyn std::error::Error>> {
    use garmin_api::training_fixtures::{seed_training, training_views};
    for persona in Persona::ALL {
        let dir = std::env::temp_dir().join(format!(
            "garmin-api-training-{}-{}",
            persona.name(),
            std::process::id()
        ));
        let db = open_db(&dir.join("cache.sqlite3"))?;
        let day = sample_day();
        seed(&db, persona, day)?;
        seed_training(&db, persona, day)?;
        let result = write_view(
            &training_views(&db, day)?,
            &fixtures_dir().join(persona.training_file_name()),
        );
        let _ = std::fs::remove_dir_all(dir);
        result?;
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
        .and_then(|()| write_activities(&db, persona, out.parent()))
        .and_then(|()| write_places(&db, persona, &beside("places")));
    let _ = std::fs::remove_dir_all(dir);
    result
}

/// The Plan, Routes and Gear views of the persona.
fn write_places(
    db: &garmin_core::Db,
    persona: Persona,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    garmin_api::places::seed_places(db, persona)?;
    let gear = garmin_api::places::persona_gear(persona);
    let views = garmin_api::places::places_views(db, sample_day(), &gear)?;
    write_view(&views, out)
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
