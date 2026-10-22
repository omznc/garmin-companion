//! The Health and Sleep data: both views, read from the same source as Today.
//! The blocks in `blocks/health.rs` and `blocks/sleep.rs` draw them.

use anyhow::{ensure, Context, Result};

use garmin_api::{HealthView, SleepView};

use crate::data::Source;

/// The made-up users of `garmin-api`, as the Health and Sleep views.
const PERSONAS: [(&str, &str, &str); 5] = [
    (
        "sample",
        include_str!("../../../crates/garmin-api/fixtures/health.sample.json"),
        include_str!("../../../crates/garmin-api/fixtures/sleep.sample.json"),
    ),
    (
        "golfer",
        include_str!("../../../crates/garmin-api/fixtures/health.golfer.json"),
        include_str!("../../../crates/garmin-api/fixtures/sleep.golfer.json"),
    ),
    (
        "gym",
        include_str!("../../../crates/garmin-api/fixtures/health.gym.json"),
        include_str!("../../../crates/garmin-api/fixtures/sleep.gym.json"),
    ),
    (
        "health_only",
        include_str!("../../../crates/garmin-api/fixtures/health.health_only.json"),
        include_str!("../../../crates/garmin-api/fixtures/sleep.health_only.json"),
    ),
    (
        "new_user",
        include_str!("../../../crates/garmin-api/fixtures/health.new_user.json"),
        include_str!("../../../crates/garmin-api/fixtures/sleep.new_user.json"),
    ),
];

pub struct Wellness {
    pub health: HealthView,
    pub sleep: SleepView,
}

/// Read both views for `day`. The as-of date hides data after that day.
fn from_db(db: &garmin_core::Db, day: chrono::NaiveDate) -> Result<Wellness> {
    garmin_core::with_as_of(day, || {
        Ok(Wellness {
            health: garmin_api::health_view(db, day).context("cannot build Health")?,
            sleep: garmin_api::sleep_view(db, day).context("cannot build Sleep")?,
        })
    })
}

/// Read both views from the same source as the Today screen.
pub fn load(source: &Source) -> Result<Wellness> {
    match source {
        Source::Default => {
            let db = garmin_api::open_default_db().context("cannot open the cache")?;
            from_db(&db, chrono::Local::now().date_naive())
        }
        Source::Persona(name) => {
            let (_, health, sleep) = PERSONAS
                .iter()
                .find(|(n, ..)| n == name)
                .with_context(|| format!("no persona {name}"))?;
            Ok(Wellness {
                health: serde_json::from_str(health).context("the health file does not parse")?,
                sleep: serde_json::from_str(sleep).context("the sleep file does not parse")?,
            })
        }
        Source::Db { path, date } => {
            ensure!(path.is_file(), "no cache copy at {}", path.display());
            let db = garmin_api::open_db(path).context("cannot open the cache copy")?;
            from_db(
                &db,
                date.unwrap_or_else(|| chrono::Local::now().date_naive()),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::PERSONAS as TODAY_PERSONAS;

    fn persona(name: &str) -> Wellness {
        load(&Source::Persona(name.into())).unwrap()
    }

    #[test]
    fn every_today_persona_has_health_and_sleep_files() {
        for (name, _) in TODAY_PERSONAS {
            persona(name);
        }
    }

    #[test]
    fn a_watch_without_a_sensor_has_no_row_for_it() {
        let keys = |n: &str| -> Vec<String> {
            persona(n)
                .health
                .metrics
                .into_iter()
                .map(|m| m.key)
                .collect()
        };
        assert!(keys("sample").contains(&"spo2".to_string()));
        assert!(!keys("golfer").contains(&"spo2".to_string()));
        assert!(!keys("golfer").contains(&"respiration".to_string()));
    }

    #[test]
    fn the_sleep_screen_needs_no_activity() {
        let sleep = persona("health_only").sleep;
        assert!(!sleep.empty);
        let night = sleep.last_night.unwrap();
        assert_eq!(night.stages.len(), 4);
        assert_eq!(sleep.week.bars.len(), 7);
        assert_eq!(sleep.month.bars.len(), 30);
    }
}
