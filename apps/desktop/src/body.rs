//! The body views (Weight, Food and Strength), read from the cache or a
//! persona. The blocks draw them.

use anyhow::{ensure, Context, Result};
use garmin_api::body::BodyViews;

use crate::data::Source;

const SAMPLE: &str = include_str!("../../../crates/garmin-api/fixtures/body.sample.json");
const EMPTY: &str = include_str!("../../../crates/garmin-api/fixtures/body.empty.json");

/// The three views. Both sources fill all of them.
pub struct BodyData(pub BodyViews);

/// Made-up personas that show a body log use the sample. The others have none.
pub fn load_body(source: &Source) -> Result<BodyData> {
    match source {
        Source::Default => {
            let db = garmin_api::open_default_db().context("cannot open the cache")?;
            let today = chrono::Local::now().date_naive();
            Ok(BodyData(
                garmin_api::body::body_views(&db, today).context("cannot build the body views")?,
            ))
        }
        Source::Persona(name) => {
            let json = if matches!(name.as_str(), "sample" | "gym") {
                SAMPLE
            } else {
                EMPTY
            };
            Ok(BodyData(
                serde_json::from_str(json).context("the body fixture does not parse")?,
            ))
        }
        Source::Db { path, date } => {
            ensure!(path.is_file(), "no cache copy at {}", path.display());
            let db = garmin_api::open_db(path).context("cannot open the cache copy")?;
            let today = date.unwrap_or_else(|| chrono::Local::now().date_naive());
            Ok(BodyData(
                garmin_api::body::body_views(&db, today).context("cannot build the body views")?,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_fixtures_parse_and_the_empty_one_says_why() {
        let sample: BodyViews = serde_json::from_str(SAMPLE).unwrap();
        let none: BodyViews = serde_json::from_str(EMPTY).unwrap();
        assert!(sample.weight.empty.is_none() && sample.strength.shown);
        assert!(none.weight.empty.is_some() && none.food.empty.is_some());
        assert!(!none.strength.shown);
    }
}
