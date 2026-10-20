//! The Insights, Fitness and Reports screens, from `app/src/screens/Insights.tsx`,
//! `Fitness.tsx` and `Reports.tsx`.
//!
//! The API builds the figures and the sentences, and it decides which
//! sections a user sees. A user who does not run gets no running analysis,
//! because the API leaves it out. This file lays the views out.

use anyhow::{ensure, Context, Result};
use garmin_api::fitness::FitnessView;
use garmin_api::insights::InsightsView;
use garmin_api::reports::ReportView;
use garmin_api::training_fixtures::TrainingViews;

use crate::data::Source;

/// The made-up users of `garmin-api`, as the three training views.
const PERSONAS: [(&str, &str); 5] = [
    (
        "sample",
        include_str!("../../../crates/garmin-api/fixtures/training.sample.json"),
    ),
    (
        "golfer",
        include_str!("../../../crates/garmin-api/fixtures/training.golfer.json"),
    ),
    (
        "gym",
        include_str!("../../../crates/garmin-api/fixtures/training.gym.json"),
    ),
    (
        "health_only",
        include_str!("../../../crates/garmin-api/fixtures/training.health_only.json"),
    ),
    (
        "new_user",
        include_str!("../../../crates/garmin-api/fixtures/training.new_user.json"),
    ),
];

pub struct Training {
    pub insights: InsightsView,
    pub fitness: FitnessView,
    /// The week on show. Earlier and Later load another one.
    pub report: ReportView,
}

fn persona(name: &str) -> Result<TrainingViews> {
    let (_, json) = PERSONAS
        .iter()
        .find(|(n, _)| *n == name)
        .with_context(|| format!("no persona {name}"))?;
    serde_json::from_str(json).context("the training file does not parse")
}

fn from_db(db: &garmin_core::Db, day: chrono::NaiveDate, offset: usize) -> Result<Training> {
    Ok(Training {
        insights: garmin_api::insights::insights_view(db, day).context("cannot build Insights")?,
        fitness: garmin_api::fitness::fitness_view(db, day).context("cannot build Fitness")?,
        report: garmin_api::reports::report_view(db, day, offset)
            .context("cannot build the report")?,
    })
}

/// Read the three views from the same source as the Today screen.
pub fn load(source: &Source, offset: usize) -> Result<Training> {
    match source {
        Source::Default => {
            let db = garmin_api::open_default_db().context("cannot open the cache")?;
            from_db(&db, chrono::Local::now().date_naive(), offset)
        }
        Source::Persona(name) => {
            let mut views = persona(name)?;
            let at = offset.min(views.reports.len().saturating_sub(1));
            Ok(Training {
                insights: views.insights,
                fitness: views.fitness,
                report: views.reports.swap_remove(at),
            })
        }
        Source::Db { path, date } => {
            ensure!(path.is_file(), "no cache copy at {}", path.display());
            let db = garmin_api::open_db(path).context("cannot open the cache copy")?;
            from_db(
                &db,
                date.unwrap_or_else(|| chrono::Local::now().date_naive()),
                offset,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_persona_parses_and_the_runner_has_weeks() {
        for (name, _) in PERSONAS {
            let t = load(&Source::Persona(name.into()), 0).unwrap();
            assert_eq!(t.report.offset, 0, "{name}");
        }
        let t = load(&Source::Persona("sample".into()), 3).unwrap();
        assert_eq!(t.report.offset, 3);
        let past = load(&Source::Persona("sample".into()), 999).unwrap();
        assert_eq!(past.report.offset, past.report.weeks - 1);
    }
}
